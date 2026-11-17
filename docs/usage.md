# Using Loglem

Short steps. All commands are in [cli.md](cli.md).

## 1. Add a source

A **source** is where logs come from. Add one from the web page (**Add source**), the terminal menu, or the command line.

| Source | Command |
|---|---|
| Folder of `.log` files | `loglem source add-folder --name web --path /var/log/myapp` |
| One file (and its rotated copies) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Caddy access log | `loglem source add-caddy --name site --path /var/log/caddy/access.log` |
| Podman container | `loglem source add-podman --name db --container db` |
| Docker container | `loglem source add-docker --name db --container db` |
| Kubernetes pods | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Linux journal | `loglem source add-journal --name ssh --identifier sshd` |

Run `loglem discover` to find Caddy, Nginx and Apache logs on this server. The web page offers them with one click.

Compressed `.gz` files are read too. Loglem imports old history once and never duplicates a line.

## 2. Search and follow live

Open **Explore logs**. Type text, pick dates, or open **Advanced filters**: severity, IP address, path, status code.

New lines appear at the top by themselves. In a terminal:

```sh
loglem search --text login --ignore-case --status 404 --source site
loglem tail --source site --ip 203.0.113.7
```

For statistics of an access log (requests per day, top pages, status codes, top IPs): `loglem stats site`, or the **Statistics** button on the source card.

## 3. Choose how long logs are kept

Each source has a duration in days, months or years. Old copies are deleted for you. The original files are not touched.

```sh
loglem source set site --days 28
```

## 4. Keep Loglem running

`loglem serve` collects logs and serves the web page. The page keeps the same address, also after a restart.

```sh
loglem service install --start    # Linux: systemd. macOS: launchd.
loglem service status
loglem check                      # exit code 1 if something is wrong
```

The service restarts itself after a crash. On a clean stop it reads every source one last time. See [stopping.md](stopping.md) for what happens while Loglem is off.

## 5. Let Loglem delete the original files

Off by default. Loglem never touches the file a program is writing to. It deletes only finished files, after this check:

1. Loglem copies the file into its archive.
2. It checks the copy against the original, byte by byte. Missing or expired lines stop the check.
3. It waits until nobody has changed the file for **24 hours**.
4. Only then it deletes the original.

If any step fails, the original stays.

**Caddy.** Add the source with `add-caddy`. Loglem reads rotated files (also `.gz`) and deletes them after the check. Use the `log` block shown by the page. `roll_keep_for 60d` is a safety net.

**Other programs.** Make the program put *finished* files in a separate folder, then:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Never point this at a folder a program still writes to.

## 6. Open the page from another computer

Off by default: the page answers only on this computer. To open it elsewhere:

```sh
loglem access user-add anna --role admin     # reader = can only look
loglem access address 10.0.0.1               # an address of this server, such as a VPN one
```

Restart Loglem. Five wrong passwords lock that computer out for 15 minutes. The link is **not encrypted**: use a VPN, or a reverse proxy with HTTPS:

```sh
loglem access proxy --trust 127.0.0.1 --base-path /loglem
```

This prints a Caddyfile block. To follow live from another computer: `loglem tail --server http://10.0.0.1:8790 --user anna`.

## 7. Alerts and backup

```sh
loglem alerts add-pattern bad-logins --text "Failed password" --count 5 --minutes 10
loglem alerts command 'echo "$LOGLEM_ALERTS" | mail -s Loglem me@example.com'
loglem backup /backups/loglem-2026-11-17
loglem backup-verify /backups/loglem-2026-11-17
```

Alerts show on the page, in `loglem check` and in `data/health.json`. **Mark as seen** hides one until it comes back. A backup also keeps lines that have since expired.

## 8. Where your data is

```text
data/
├── config.json     sources and settings
├── archives/       saved logs, one database per source
├── audit/          who searched, exported or erased
├── proof/          seal registers (proof mode)
└── diagnostics/    Loglem's own errors
```

Use `--data-dir /absolute/path` or `LOGLEM_DATA_DIR` for another place. Move settings with `loglem config export` and `config import`. Move logs by stopping Loglem and copying `data/`.

Archives from older versions are converted when Loglem starts. The saved text stays the same, the file gets smaller.

## 9. When something goes wrong

- A source shows an error: the others keep working. Open its card, or run `loglem status`.
- Loglem's own errors: **Problems** in the page, or `loglem diagnostics`.
- Port in use: `loglem config web-port 8790`, then restart.
- Check the program: `loglem self-check`.

## 10. Update

Press **Check** in the page **System**, or:

```sh
loglem update check
loglem update install
```

Only signed releases are installed (Linux). Your `data/` is kept. The old program stays as `loglem.previous`.

## 11. In a container

```sh
podman build -t loglem -f Containerfile .
podman run -d --name loglem -p 127.0.0.1:8790:8790 \
  -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='a long password' \
  -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro loglem
```

Open `http://127.0.0.1:8790` and add `/logs/myapp` as a folder source. Data stay in the `loglem-data` volume. Podman, Docker and the host journal are not reachable from inside: for those, run Loglem on the host, or mount the log files into `/logs`. To clean up original files, mount the folder without `:ro`.

Personal data, GDPR tools and proof mode: [privacy.md](privacy.md).
