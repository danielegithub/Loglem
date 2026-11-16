# Command reference

Run `loglem --help` or `loglem COMMAND --help` for the options accepted by the installed version. With no command, Loglem opens the keyboard-driven terminal interface; `loglem tui` does the same explicitly. The main command groups are:

| Command | Purpose |
|---|---|
| `init` | Create an empty configuration. |
| `source list` | List configured sources. Use `source add-folder`, `add-file`, `add-podman`, or `add-caddy` (Caddy log with rotated-file cleanup) to add one. |
| `podman-list` | Discover containers and pods; optionally select a named connection with `--connection`. |
| `status [--json]` | Show archive locations, sizes, dates, and collection status. |
| `collect` / `serve` | Collect once, or run the service (collection plus web page at a fixed address) until shutdown. `web` does the same, or prints the address if the service is already running. |
| `search [options]` | Search archived text and date ranges; `--json` returns machine-readable output. |
| `prune [--apply]` | Preview expired entries, or delete them from archives with `--apply`. |
| `config export FILE` / `config import FILE` | Export or import source configuration. |
| `config poll SECONDS` | Set the automatic collection interval. |
| `config web-port PORT` | Choose the fixed local port of the web page (from the next start). |
| `service install [--start]` | Register a systemd user service; `start`, `stop`, and `status` manage it. |
| `diagnostics` | Print persisted Loglem errors and stack traces. |
| `self-check` | Check the executable and embedded SQLite schema without changing user data. |

Pass `--data-dir /absolute/path` before the command to select a data directory. `LOGLEM_DATA_DIR` provides the same setting for automation. Commands to manage sources and updates are described in [usage.md](usage.md).
