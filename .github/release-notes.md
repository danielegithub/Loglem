Experimental release of Loglem. Read the limitations before use (`docs/limitations.md`).

- Reads log files (also rotated and `.gz`), Caddy, Nginx, Apache, Podman, Docker, Kubernetes and the Linux journal into one SQLite archive per source.
- Web page and terminal menu. Search with filters, live view, statistics, `loglem tail`.
- Retention per source. Optional cleanup of original files: copied, checked byte by byte, then deleted only after 24 hours without changes.
- Service for Linux (systemd) and macOS (launchd). Login, roles and reverse proxy for other computers.
- Alerts, backup and restore, proof mode with time stamps.
- Personal data: IP masking, erase and export of one person's lines, access log.
- Signed binary updates (Linux).

Not tested on a real Linux machine: the systemd service, a real Kubernetes cluster, and updates from a published release.

Download the `.tar.gz` matching your Linux architecture. Verify the signed checksums with the publisher's independently obtained public key, extract the package, and run `sh install.sh`. Raw `loglem-*` files are also provided for the updater.
