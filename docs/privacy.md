# Personal data and proof mode

Logs hold personal data. An IP address is personal data. Loglem gives you tools to follow the rules. It does **not** make an installation compliant by itself: why you keep logs, for how long, and what you tell people stays your decision.

## Keep less

- **Short retention.** Set the days per source. Expired lines are deleted.
- **Mask IPs.** `203.0.113.77` becomes `203.0.113.0`. Do it at once, or after N days:

```sh
loglem privacy set site --mask-ips-after-days 0
loglem privacy set site --keep-fields request.uri,status
```

- **Keep only some JSON fields.** All other fields are dropped.
- **See what you keep:** `loglem privacy report`. Put it in your register of processing.

Masking changes only the saved copy. The cleanup check of original files still works. In proof mode, masking is allowed only "at once" (later it would change sealed lines).

## Erase one person

Someone asks you to delete their data. Use the page **Personal data**, or:

```sh
loglem erase --ip 203.0.113.7                                    # preview only
loglem erase --ip 203.0.113.7 --apply --reason "art. 17 request of 17/11/2026, case 3"
loglem erase --ip 203.0.113.7 --originals --apply --reason "…"   # also from the original files
```

Loglem removes the lines from **every** source. The reason goes in the audit log. The IP is never written there.

**Original files** (`--originals`, or the box on the page). Loglem rewrites finished log files without those lines. It does this only if:

- the file is not the one a program is writing to,
- nobody changed it for 24 hours,
- the whole file is already in the archive.

It writes a new file next to the old one, checks it, then swaps it in. If anything is wrong, the original stays. Skipped files are listed with the reason. Repeat the command after the next rotation. `.gz` files are rewritten too.

It does **not** touch:

- Podman, Docker, Kubernetes and journal logs. They are not Loglem's files, and the journal cannot drop single entries. Shorten their retention, or recreate the container.
- backups already made (they expire with their own rotation),
- the file a program is writing to, until it is rotated.

In `--reason` write a case number, not an email address: an email is personal data too.

## Export one person

Someone asks what you hold on them:

```sh
loglem export --ip 203.0.113.7 --output /safe/place/request-3.txt   # add --json for JSON
```

The file is readable only by its owner. Delete it after you send it.

## Who looked

`loglem audit` (or the files in `data/audit/`) lists every login, search, export, erase and settings change, with user and time. Search values are **not** written, only the kind of filter ("text, ip"). Entries are kept for 13 months.

## Disk and backups

- Loglem does not encrypt its files. Use an encrypted disk.
- `loglem backup` writes plain files. Encrypt them before they leave the server (for example with restic).
- Do not put copies of logs with personal data in storage you cannot delete from (such as a locked bucket).

## Proof mode

For a source that needs proof that its lines were not changed:

```sh
loglem proof enable site --tsa https://freetsa.org/tsr     # --tsa is optional
loglem proof verify site
```

Each finished day gets a fingerprint (SHA-256) of its lines. It also contains the fingerprint of the day before, so changing one day breaks all the following ones. With `--tsa`, an outside authority signs each fingerprint with the time; it sees only the fingerprint.

`loglem proof verify` recomputes every day and says **intact**, **ALTERED** or **expired**. You can also check a copy: `loglem proof verify --archive copy.db --chain copy.chain`.

What it proves: a sealed day did not change after the seal (or after the time stamp). It does not prove the lines were true when written.

Lines expire only as whole days, and each expiry is written in the register.

**Erase in proof mode.** The erase wins. Loglem removes the lines, **recomputes** the fingerprint of that day and of every following day, and adds a note to the register: when, why, how many lines, old fingerprint → new fingerprint. `proof verify` shows the note. The old time stamps stay valid for the old version. New ones are requested for the new fingerprints.
