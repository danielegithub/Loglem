# Befehle

`loglem BEFEHL --help` zeigt alle Optionen. Ohne Befehl öffnet Loglem das Terminalmenü. Setzen Sie `--data-dir /absoluter/pfad` vor den Befehl, um einen anderen Datenordner zu nutzen (oder setzen Sie `LOGLEM_DATA_DIR`).

## Quellen und Lesen

| Befehl | Was er tut |
|---|---|
| `source add-folder / add-file / add-caddy / add-podman / add-docker / add-kubernetes / add-journal` | Fügt eine Quelle hinzu. |
| `source list`, `source set NAME --days N` | Listet Quellen auf, ändert die Dauer. |
| `source configure NAME …` | Name, Ordner für fertige Dateien, Aufräumen rotierter Dateien, Pfad. |
| `discover` | Sucht die Logs von Caddy, Nginx und Apache. |
| `podman-list`, `docker-list`, `kubernetes-list` | Listet Container oder Pods auf. |
| `collect` | Liest einmal. |
| `serve` / `web` | Liest dauerhaft und stellt die Webseite bereit. |
| `status [--json]` | Orte, Größen, Daten, Zustand, Lücken. |
| `check [--minutes N]` | Exit-Code 1, wenn Loglem aus ist, eine Quelle nicht gelesen wird oder eine Warnung auslöst. |

## Suchen und verfolgen

| Befehl | Was er tut |
|---|---|
| `search` | `--text --ignore-case --level error --ip --path --status --from --to --source --limit --json` |
| `tail` | Dieselben Filter, live. `--server URL --user NAME` folgt einem entfernten Loglem. |
| `stats QUELLE [--days N] [--json]` | Anfragen pro Tag, Seiten, Statuscodes, IPs, langsame Seiten. |

## Daten pflegen

| Befehl | Was er tut |
|---|---|
| `alerts list / add-pattern / add-size / add-free-space / remove / command / seen` | Warnregeln und Benachrichtigung. |
| `backup DIR`, `backup-verify DIR`, `restore DIR [--replace]` | Sichert, prüft und stellt Daten wieder her. |
| `proof enable / disable / verify` | Beweismodus: versiegelt jeden Tag. |
| `privacy set QUELLE …`, `privacy report` | Maskiert IPs, behält JSON-Felder, listet auf, was gespeichert ist. |
| `erase … [--originals]`, `export …`, `audit [--days N]` | DSGVO: löscht (auch aus den Originaldateien) oder exportiert die Zeilen einer Person, zeigt, wer nachgesehen hat. Auch auf der Seite **Personenbezogene Daten**. |
| `prune [--apply]` | Zeigt oder löscht abgelaufene Zeilen. |
| `archive-completed` | Archiviert und prüft eine fertige Datei von Hand. |

## Betreiben und pflegen

| Befehl | Was er tut |
|---|---|
| `service install [--start] / start / stop / status` | Dienst: systemd unter Linux, launchd unter macOS. |
| `access user-add / user-remove / user-list / address / proxy` | Andere Computer: Benutzer, Adresse, Reverse-Proxy. |
| `config export / import / poll / web-port` | Einstellungen. |
| `update check / install / status / configure` | Signierte Updates (Linux). |
| `diagnostics`, `self-check`, `init` | Loglems Fehler, ein Schnelltest, eine leere Konfiguration. |
