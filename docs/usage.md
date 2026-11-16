# Using Loglem

A step-by-step guide. For the list of commands see [cli.md](cli.md); for what Loglem does not do see [limitations.md](limitations.md).

## Contents

- [1. Add your first source](#1-add-your-first-source)
- [2. Search the logs](#2-search-the-logs)
- [3. Choose how long logs are kept](#3-choose-how-long-logs-are-kept)
- [4. Keep Loglem running](#4-keep-loglem-running)
- [5. Let Loglem clean up the original files](#5-let-loglem-clean-up-the-original-files)
- [6. Change, pause or remove a source](#6-change-pause-or-remove-a-source)
- [7. Where your data is and how to move it](#7-where-your-data-is-and-how-to-move-it)
- [8. When something goes wrong](#8-when-something-goes-wrong)
- [9. Update Loglem](#9-update-loglem)

## 1. Add your first source

A **source** is where logs come from: a folder of `.log` files, a single file, a Podman container, or the access log of Caddy. You can add one from the web page, from the terminal menu, or from the command line.

### From the web page

```sh
loglem web
```

Loglem prints the page address (for example `http://127.0.0.1:53142`) and the folder where it keeps its data. Open that address in a browser. The address stays the same after a restart. If the port is taken by another program, Loglem stops and says so; choose another with `loglem config web-port 8790`. Loglem never uses port 80 or 443.

The page works only on the machine where Loglem runs. On a remote server, open it through an SSH tunnel (`ssh -L 53142:127.0.0.1:53142 user@server`, then browse to `http://127.0.0.1:53142`).

The menu on the left has four pages:

- **Sorgenti:** your sources, with status, number of lines and links to their logs and settings.
- **Esplora i log:** search and read the logs of a source. The list refreshes by itself; a checkbox pauses it.
- **Aggiungi sorgente:** a guided form. Choose the type, pick the folder or container, give it a name, say what kind of log it is and how long to keep it, then press **Salva e apri i log**. Loglem reads the logs straight away.
- **Problemi:** Loglem's own errors, with their cause (see [section 8](#8-when-something-goes-wrong)).

### From the command line

```sh
# a folder of .log files (new files are picked up automatically)
loglem source add-folder --name web-logs --path /srv/example/logs --days 28

# a single file; also read its renamed copies, such as api.log-1
loglem source add-file --name api --path /srv/example/logs/api.log \
  --rotated-pattern 'api.log-*' --days 14

# a Podman container
loglem podman-list
loglem source add-podman --name app --container my-container --days 14

# read now and check
loglem collect
loglem status
```

Notes:

- A folder source reads the `.log` files directly inside the folder; subfolders, compressed files and other extensions are ignored.
- A file source needs a name ending in `.log`. Compressed files (`.gz`) are not read.
- For Podman, Loglem remembers the container's full ID. If you recreate a container with the same name, add it as a new source (or see [section 6](#6-change-pause-or-remove-a-source)). The history already collected stays available after the container is removed. Use `--connection NAME` to choose a named Podman connection.
- Loglem only reads your original files. It does not change them unless you turn on cleanup (section 5).

### From the terminal menu

Run `loglem`. Use the arrow keys (or `j`/`k`), Enter to choose and Esc to go back. The labels are currently in Italian.

```text
> Le mie sorgenti — posizione, spazio e log salvati
  Aggiungi cartella, file o container
  Cerca nei log
  Conservazione — per quanti giorni tenere i log
  Leggi i log adesso
  Raccolta automatica (servizio Linux)
  Impostazioni
  Aggiornamenti
  Esci
```

Choose **Aggiungi cartella, file o container**, then the type, then browse to the folder (or choose **Scrivi / incolla un percorso…** to type it). Check the summary and save with **Salva e leggi i log adesso**. Esc cancels before saving.

## 2. Search the logs

On the web page, open **Esplora i log**, choose the source and type your text. You can limit by date (UTC). The newest lines come first.

From the command line:

```sh
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16
```

- The search is for exact text and is case-sensitive; it is not a regular expression.
- `--to 2026-11-16` includes the whole of that day (UTC).
- Add `--json` for output that other programs can read, and `--limit` for up to 1000 results.
- If a line has no date Loglem can recognise, it is still searchable and is shown with the time it was read. Loglem recognises common JSON logs (`ts`, `timestamp`, `time`, `@timestamp`).

## 3. Choose how long logs are kept

Each source has its own retention: a number of days, months or years. When an entry gets older than that, Loglem deletes it from its archive.

```sh
loglem source set web-logs --days 7
loglem source configure web-logs --amount 6 --unit months --log-kind access
loglem source configure web-logs --amount 2 --unit years
```

- In the web page and the terminal menu (**Conservazione**) the same setting is a form that also shows when entries will expire.
- Expired entries are removed while Loglem is running (see section 4). `loglem prune` shows what would be deleted; `loglem prune --apply` deletes it now.
- Retention counts from the log's own date or from when Loglem first read it, whichever is earlier, so a line dated in the future cannot stay forever.
- Pausing a source stops new reading but not retention.
- The kind of log (access, generic, errors) only describes the source; it does not set a legal retention period for you.
- The duration applies to Loglem's copy. Your original files are not deleted unless you turn on cleanup (section 5).

## 4. Keep Loglem running

`loglem web` collects logs only while it runs. To collect all the time, even after you close the terminal or reboot, install it as a service:

```sh
loglem service install --start
loglem service status
```

This creates a **systemd user service** for the current user. It reads new lines every 10 seconds (change it with `loglem config poll 5`), removes expired entries, and serves the web page at the same address.

- On a server, ask an administrator to run `sudo loginctl enable-linger USERNAME` once, so the service runs even when the user is logged out.
- The service has only that user's permissions. Make sure that user can read the log folders; Loglem does not run as root.
- Only one Loglem runs per data folder. If the service is already running, `loglem web` does not start a second one; it prints the service's address and exits.
- Starting Loglem with a different `--data-dir` shows different sources. The data folder in use is always shown in the web page and the terminal menu.

## 5. Let Loglem clean up the original files

By default Loglem does not delete your original files. You can let it delete original files **after it has copied and verified them**. It does so only for files nobody has modified for at least 24 hours, never for the file a program is writing to, and it keeps any file it cannot verify (the source card shows a warning). Expiry of the archive is a separate step, set in section 3.

### Caddy

In **Aggiungi sorgente** choose **Il log di accesso di Caddy**, open the log folder (often `/var/log/caddy`) and pick the file Caddy writes to, usually `access.log`. Loglem then also reads the files Caddy creates when it rolls the log (for example `access-2026-11-16T10-00-00.000.log`) and deletes them after verification. At the end the page shows the block to put in your Caddyfile:

```caddyfile
log {
	output file /var/log/caddy/access.log {
		roll_size 50MiB
		roll_uncompressed
		roll_keep 100
		roll_keep_for 60d
	}
}
```

- `roll_uncompressed` is required: Loglem only verifies uncompressed files. If it finds `.gz` files it reports them on the card.
- `roll_keep_for 60d` is a safety net: if Loglem is off for a long time, Caddy still deletes the oldest files.
- Loglem does not edit your Caddyfile; you paste the block yourself.

From the command line: `loglem source add-caddy --name site --path /var/log/caddy/access.log --days 90`. For a file source that already has a rotation pattern: `loglem source configure site --remove-rotated true`.

### Other programs

Ask the program to move each finished file, closed and uncompressed, into a dedicated folder. In the source's settings turn on **Archivia ed elimina i file di una cartella dedicata** and confirm. Never point this at a folder a program is still writing to.

```sh
loglem source configure web --completed-dir /absolute/completed-files --handoff-confirmed
# turn it off again
loglem source configure web --completed-dir ""
```

For a single finished file, without the 24-hour wait:

```sh
loglem archive-completed --source web --file /srv/example/logs/access-old.log --producer-closed --remove
```

Without `--remove` the command archives and verifies the file without deleting the original. Loglem never deletes the originals of a Podman container.

## 6. Change, pause or remove a source

In the web page, press **Impostazioni** on the source card. From the command line:

```sh
loglem source rename web "Public website"
loglem source pause web          # stop reading, keep the history searchable
loglem source resume web
loglem source detach web         # disconnect; the archive stays searchable
loglem source configure web --container NEW_CONTAINER   # point to a replacement container
loglem source configure web --path /new/path            # new file or folder
```

- Renaming changes only the label; the history is kept.
- To keep a source's archive in another folder: `loglem source configure web --archive-dir /absolute/folder`. Loglem copies it, checks the copy and then removes the old files; it never overwrites an existing database. Use `--archive-dir ""` to go back to the default folder. Close other programs that have the archive open first.

## 7. Where your data is and how to move it

By default everything is in the Loglem folder:

```text
Loglem/
├── loglem          the program
└── data/
    ├── config.json   your sources and settings
    ├── archives/     the saved logs (one database per source)
    ├── diagnostics/  Loglem's own error log
    └── updates/
```

To use another location, pass `--data-dir /absolute/path` before the command, or set `LOGLEM_DATA_DIR`.

To move to another machine:

```sh
loglem config export ./loglem-config.json     # on the old machine
loglem config import ./loglem-config.json     # on the new machine
```

This moves your **settings** (sources, durations), not the logs. Adjust paths and permissions on the new machine. To move the logs too, stop Loglem and copy the whole `data/` folder. Importing a configuration never removes the retention rule of a source that already has an archive; pause that source instead.

## 8. When something goes wrong

- **A source shows an error:** the others keep working and the lines already saved stay readable. Open the source card for the message, or run `loglem status`.
- **The page is empty or a source says "cannot read":** check that the user running Loglem can read the folder or file.
- **See Loglem's own errors:** **Problemi** in the web page, or `loglem diagnostics`. Each entry has the time, what Loglem was doing, the cause and technical details. The file is `data/diagnostics/loglem.log` (about 5 MB, then rotated; readable only by its owner).
- **Check the installation:** `loglem self-check`.
- **Port already in use:** `loglem config web-port 8790`, then restart.

## 9. Update Loglem

The simplest way is to let Loglem update itself from a signed release. Your `data/` folder is kept. (`install.sh` refuses to run over an existing installation; to replace the program by hand, stop the service first.)

```sh
loglem update check
loglem update install
loglem update status
```

It works only for a Loglem installed in a folder the user can write to and registered with `loglem service install`. Before replacing anything it checks the signature, version and size; if the new version does not start, the previous program is kept next to it (`loglem.previous`). For a locally built program, first tell Loglem where to look and which key to trust: `loglem update configure --repository OWNER/REPOSITORY --public-key BASE64_KEY`. Updates that change the data format are refused.
