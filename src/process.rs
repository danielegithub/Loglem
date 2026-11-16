//! Bounded subprocess I/O. No shell interpolation, output() buffering, or indefinite waits.
use crate::MAX_LINE_BYTES;
use anyhow::{bail, Context, Result};
use std::{
    io::{BufRead, BufReader, Read},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn pump(reader: impl Read, stderr: bool, sender: mpsc::SyncSender<Result<(bool, Vec<u8>)>>) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = Vec::new();
        match reader
            .by_ref()
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => break,
            Ok(n) if n > MAX_LINE_BYTES => {
                let _ = sender.send(Err(anyhow::anyhow!("Subprocess line exceeds 1 MiB")));
                break;
            }
            Ok(_) => {
                if sender.send(Ok((stderr, line))).is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = sender.send(Err(e.into()));
                break;
            }
        }
    }
}

pub fn stream(
    command: &mut Command,
    timeout: Duration,
    mut consume: impl FnMut(bool, Vec<u8>) -> Result<()>,
) -> Result<ExitStatus> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Cannot start subprocess")?;
    let (sender, receiver) = mpsc::sync_channel(4);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let sender2 = sender.clone();
    let out_thread = thread::spawn(move || pump(stdout, false, sender));
    let err_thread = thread::spawn(move || pump(stderr, true, sender2));
    let started = Instant::now();
    let result = (|| -> Result<ExitStatus> {
        loop {
            if started.elapsed() > timeout {
                bail!(
                    "Subprocess exceeded its {} second timeout",
                    timeout.as_secs()
                );
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    let (stderr, bytes) = line?;
                    consume(stderr, bytes)?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() > timeout {
                bail!("Subprocess did not exit");
            }
            thread::sleep(Duration::from_millis(25));
        }
    })();
    if result.is_err() {
        #[cfg(unix)]
        {
            // The child was placed in its own process group above. Terminate its
            // descendants too, so inherited pipes cannot keep reader threads stuck.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = child.kill();
    }
    drop(receiver); // Release producers even if a callback failed with a full channel.
    let _ = child.wait();
    let _ = out_thread.join();
    let _ = err_thread.join();
    result
}

pub fn output(command: &mut Command, timeout: Duration) -> Result<Vec<u8>> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let status = stream(command, timeout, |is_err, line| {
        if stdout.len() + stderr.len() + line.len() > 8 * 1024 * 1024 {
            bail!("Subprocess output exceeds 8 MiB");
        }
        if is_err {
            stderr.extend(line);
        } else {
            stdout.extend(line);
        }
        Ok(())
    })?;
    if !status.success() {
        bail!(
            "Command failed: {}",
            crate::util::safe_text(&String::from_utf8_lossy(&stderr))
        );
    }
    Ok(stdout)
}
