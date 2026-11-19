# Using Loglem

Short steps. All commands are in [cli.md](cli.md).

## 1. Create a site and connect its logs

On the home page choose **Nuovo sito** and enter a site or project name. Open **Aggiungi un log** and choose the file, folder or service to read. The site page shows the original location, archive, retention period and latest entries. **Cerca nei log del sito** searches only this site.

| Source | Command |
|---|---|
| Folder of `.log` files | `loglem source add-folder --name web --path /var/log/myapp` |
| One file (and its rotated copies) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Any log, guided step by step (recommended) | `loglem add` |
| Podman container | `loglem source add-podman --name db --container db` |
| Docker container | `loglem source add-docker --name db --container db` |
| Kubernetes pods | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Linux journal | `loglem source add-journal --name ssh --identifier sshd` |

Run `loglem discover` to find the logs that known programs list in their configuration (see `src/recipes/`). The **Add** page does it for you and explains why it proposes each log.

Compressed `.gz` files are read too. Loglem imports old history once and never duplicates a line.

Set the site or project when adding a source. On the command line, pass `--site example.com` to any `source add-*` command. To move an existing source, use `loglem source assign-site myapp example.com`. The home page groups logs by site. Unassigned journal sources start in the system group. During upgrade, Loglem saves the previous configuration as `config.json.bak-YYYYMMDD`.

Open a site and expand **Copie** to see paths, sizes and who manages retention. Shared files count once in the site total. Loglem flags pairs when more than half of the larger sample matches, using up to 200 recent rows per source. You can pause either collection. The external search runs on demand for up to eight seconds in the log folders, `/var/log`, `/srv`, the local journal and local Podman and Docker. Only distinct files verified in full contribute to the identical-byte total.

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

The service is a user unit of systemd. On a server, run `loginctl enable-linger USER` once, so it starts at boot and keeps running after you close SSH. `loglem service install` tells you when this is needed.

The service restarts itself after a crash. On a clean stop it reads every source one last time. See [stopping.md](stopping.md) for what happens while Loglem is off.

The service also runs the automatic check (every 5 minutes, no cron needed). The Sites page and **System → Service** show whether it is running, since when, and how many times it restarted. To remove the service: `loglem service uninstall`. Your data stays.

## 5. Let Loglem delete the original files

Off by default. Loglem never touches the file a program is writing to. It deletes only finished files, after this check:

1. Loglem copies the file into its archive.
2. It checks the copy against the original, byte by byte. Missing or expired lines stop the check.
3. It waits until nobody has changed the file for **24 hours**.
4. Only then it deletes the original.

If any step fails, the original stays.

**Rotated files.** New sources keep their originals. To remove verified rotated files, enable cleanup in the source settings.

**Other programs.** Make the program put *finished* files in a separate folder, then:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Never point this at a folder a program still writes to.

**Who deletes the originals?** One place: the cleanup box in the source settings. Loglem reads the logrotate rules and says whether a rotation already exists. If logrotate (or the program) already rotates and removes the files, keep Loglem's cleanup **off**: two systems deleting the same files is pointless and hides mistakes. Loglem warns you when both are on, when the originals live longer than its own copy, and when rotated files have names it does not read.

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

### Status and email alerts

Loglem checks itself every 5 minutes (change it in **System → Automatic check**, or `loglem alerts monitor --every 10`). The page shows "All good, checked at 14:35" or what is wrong.

Email alerts are optional and use [Resend](https://resend.com):

```sh
loglem alerts email-set --from noreply@example.com --to admin@example.com   # asks for the key
loglem alerts email-test
loglem alerts email-off            # also deletes the key
```

Create a **sending-only** key limited to your domain, and verify the domain in Resend. The sender can be `noreply@` on a subdomain. Replies are not read. Loglem sends one mail when a problem opens, one when it closes, and a reminder if it stays open for more than 24 hours. Mails contain no log lines. At most 6 an hour. The key is kept in a private file (`secrets/resend.key`): never in the settings, exports, backups or logs.

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
