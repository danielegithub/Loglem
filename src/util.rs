use anyhow::{bail, Context, Result};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn safe_text(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn private_dir(path: &Path) -> Result<()> {
    if path.exists() {
        if !fs::symlink_metadata(path)?.is_dir() {
            bail!("{} is not a directory", path.display());
        }
    } else {
        fs::create_dir_all(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

pub fn private_file(path: &Path) -> Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    Ok(opts.open(path)?)
}

pub fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    Ok(())
}

/// Same-directory replacement: the old configuration remains valid after a failed write.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    private_dir(parent)?;
    let temp = parent.join(format!(
        ".loglem-{}-{}.tmp",
        std::process::id(),
        crate::timestamps::now_ns()
    ));
    let result = (|| -> Result<()> {
        let mut file = private_file(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        sync_dir(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub struct Lock {
    _file: File,
}
impl Lock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(path)?;
        file.try_lock_exclusive()
            .with_context(|| format!("Another operation is using {}", path.display()))?;
        Ok(Self { _file: file })
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Shared access for collection/search; exclusive access while relocating an archive.
pub struct DataAccess {
    _file: File,
}
impl DataAccess {
    fn open(dir: &Path) -> Result<File> {
        private_dir(dir)?;
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        Ok(opts.open(dir.join("data-access.lock"))?)
    }
    pub fn read(dir: &Path) -> Result<Self> {
        let file = Self::open(dir)?;
        FileExt::try_lock_shared(&file).context("Archivio in manutenzione; riprova tra poco")?;
        Ok(Self { _file: file })
    }
    pub fn write(dir: &Path) -> Result<Self> {
        let file = Self::open(dir)?;
        let start = std::time::Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => break,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < std::time::Duration::from_secs(10) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(25))
                }
                Err(error) => {
                    return Err(error)
                        .context("Raccolta o ricerca ancora in corso; riprova dopo la lettura")
                }
            }
        }
        Ok(Self { _file: file })
    }
}

pub fn file_digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn bytes_on_disk(path: &Path) -> u64 {
    [
        PathBuf::from(path),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ]
    .iter()
    .filter_map(|p| fs::metadata(p).ok())
    .map(|m| m.len())
    .sum()
}
