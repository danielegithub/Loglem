Experimental Linux terminal release of Loglem.

- File and Podman log collection into per-source SQLite archives.
- Keyboard-driven terminal interface and non-interactive commands.
- Source status, literal text/date search, and per-source retention.
- Linux systemd user-service integration and signed binary update code.

Read the limitations in the README before use. Automatic cleanup of active producer files is not enabled. A filename or a date is not proof that a producer has closed a file.

Download the `.tar.gz` matching your Linux architecture. Verify the signed checksums using the publisher's independently obtained public key, extract the package, and run `sh install.sh`. Raw `loglem-*` files are also provided for the updater.

