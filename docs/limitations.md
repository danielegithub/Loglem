# Limitations

What Loglem does not do, or does not guarantee, in this version.

**Supported**

- Linux only. Windows is not supported; on macOS there is no automatic service (use `loglem web`).
- Sources: `.log` text files (UTF-8), Podman containers and Caddy access logs. Compressed files (`.gz`) are not read. Docker, Kubernetes, Nginx and the systemd journal are not supported yet.
- Lines up to 1 MiB.
- Search is for exact, case-sensitive text and a date range. There are no filters by severity, IP address, path or status code yet.
- The web page can only be opened from the machine where Loglem runs (use an SSH tunnel for a remote server).

**Data safety**

- Podman logs are read in snapshots. If a container is deleted before Loglem reads it, or Loglem is stopped for a long time, some lines can be lost.
- Entries expire only while Loglem is running (or when you run `loglem prune --apply`).
- Deleting an entry from Loglem's archive does not erase it from file-system snapshots or backups that already copied it.
- Archives are SQLite files and must be on a local disk, not a network share. Do not copy them while Loglem is running; stop it first.
- If the disk is full, originals that are not yet archived are kept; Loglem cannot free space or stop other programs writing.
- Loglem cannot tell by itself that a program has finished with a file. Its cleanup waits 24 hours and never touches the file being written, but only enable it for folders and Caddy logs set up as described in the guide.
- Archives are not tamper-proof: Loglem is not an evidence archive.

**Tested and not yet tested**

- Tested with real Podman and a real browser: adding sources from the page, search, restart without lost or duplicated lines, expiry and simulated Caddy-style rotation.
- Not tested yet: the systemd service on a real Linux machine, real Caddy, and returning to the previous version after a failed update.
- There is no published release yet.
