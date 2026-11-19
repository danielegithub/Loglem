# Limitations

What Loglem does not do, or does not promise.

## Platforms

- Linux and macOS. The service is systemd (Linux) or launchd (macOS). Signed updates: Linux only.
- Windows: the code is written to be portable, but it has not been built or tested.
- No release is published yet.
- In a container, Loglem reads only the log files you mount. Podman, Docker, Kubernetes and the host journal need Loglem on the host.

## What it reads

- UTF-8 text lines up to 1 MiB. Other encodings are not read.
- Severity, status code and path filters read the text of the line. They know Caddy (JSON), Nginx and Apache (combined), and the journal. Other formats are only found by text.
- Statistics work on Caddy JSON and Nginx/Apache combined logs.
- No regular expressions yet.
- Statistics rankings use up to 256 addresses and 256 paths per summary. When the counters fill up, the page shows "approximate ranking". Request counts and status codes stay exact.

## What can be lost

- Podman, Docker and Kubernetes keep logs only while the container or pod exists. If it is removed while Loglem is off, those lines are lost. Loglem shows a **gap** on the card.
- For Kubernetes, follow the application by label (`app=web`). A single pod disappears at every release.
- If Loglem is off, entries do not expire and nothing is cleaned up. See [stopping.md](stopping.md).
- A full disk: files not yet archived are kept. Loglem cannot free space.

## Data safety

- Archives are SQLite files on a local disk, not a network share. Stop Loglem before you copy them by hand, or use `loglem backup`.
- Deleting a line from Loglem does not delete it from backups or from the original files.
- Loglem does not encrypt data on disk. Use an encrypted disk and encrypted backups.
- The link to the web page is not encrypted. Use a VPN or HTTPS reverse proxy.
- Loglem cannot know if a program has finished with a file. Cleanup waits 24 hours and never touches the file in use.
- Proof mode shows that a sealed day did not change. It does not prove the lines were true. See [privacy.md](privacy.md).

## What was tested

Tested on macOS with real Podman, journald (in a Linux container), Caddy, Chrome and a real time-stamp authority: adding sources from the page, search, live view, login, restart without lost or duplicated lines, expiry, Caddy rotation, reverse proxy, backup and restore, proof mode, erase.

Tested on real Linux (Linux Mint 22.3, systemd 255, glibc 2.39, x86_64): journald, Podman 4.9 (journald and file drivers, recreated and removed containers), logrotate (`create`, `delaycompress`, `dateext`, `copytruncate`), Caddy 2.11 with its own rotation, the systemd service (install, restart after `kill -9`, clean restart, `check`, uninstall), backup, verification and restore. Details and numbers: [performance.md](performance.md).

Never tested on: a real Kubernetes cluster, Docker, Nginx and Apache, ARM64, a real reboot of the machine, and an update from a published release.
