# Commands

`loglem COMMAND --help` shows every option. Without a command, Loglem opens the terminal menu. Put `--data-dir /absolute/path` before the command to use another data folder (or set `LOGLEM_DATA_DIR`).

## Sources and reading

| Command | What it does |
|---|---|
| `source add-folder / add-file / add-caddy / add-podman / add-docker / add-kubernetes / add-journal` | Add a source. |
| `source list`, `source set NAME --days N` | List sources, change the duration. |
| `source configure NAME …` | Name, folder for finished files, rotated-file cleanup, path. |
| `discover` | Find Caddy, Nginx and Apache logs. |
| `podman-list`, `docker-list`, `kubernetes-list` | List containers or pods. |
| `collect` | Read once. |
| `serve` / `web` | Read all the time and serve the web page. |
| `status [--json]` | Locations, sizes, dates, state, gaps. |
| `check [--minutes N]` | Exit code 1 if Loglem is off, a source is not read, or an alert fires. |

## Find and follow

| Command | What it does |
|---|---|
| `search` | `--text --ignore-case --level error --ip --path --status --from --to --source --limit --json` |
| `tail` | Same filters, live. `--server URL --user NAME` follows a remote Loglem. |
| `stats SOURCE [--days N] [--json]` | Requests per day, pages, status codes, IPs, slow pages. |

## Look after the data

| Command | What it does |
|---|---|
| `alerts list / add-pattern / add-size / add-free-space / remove / command / seen` | Alert rules and notification. |
| `backup DIR`, `backup-verify DIR`, `restore DIR [--replace]` | Copy, check and restore data. |
| `proof enable / disable / verify` | Proof mode: seal each day. |
| `privacy set SOURCE …`, `privacy report` | Mask IPs, keep JSON fields, list what is kept. |
| `erase … [--originals]`, `export …`, `audit [--days N]` | GDPR: erase (also from original files) or export one person's lines, see who looked. Also in the page **Personal data**. |
| `prune [--apply]` | Show or delete expired lines. |
| `archive-completed` | Archive and check one finished file by hand. |

## Run and maintain

| Command | What it does |
|---|---|
| `service install [--start] / start / stop / status` | Service: systemd on Linux, launchd on macOS. |
| `access user-add / user-remove / user-list / address / proxy` | Other computers: users, address, reverse proxy. |
| `config export / import / poll / web-port` | Settings. |
| `update check / install / status / configure` | Signed updates (Linux). |
| `diagnostics`, `self-check`, `init` | Loglem's own errors, a quick test, an empty configuration. |
