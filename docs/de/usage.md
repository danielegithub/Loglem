# Loglem verwenden

Wenige Schritte. Alle Befehle stehen in [cli.md](cli.md).

## 1. Eine Quelle hinzufügen

Eine **Quelle** ist der Ort, aus dem Logs kommen. Fügen Sie sie über die Webseite (**Quelle hinzufügen**), das Terminalmenü oder die Befehlszeile hinzu.

| Quelle | Befehl |
|---|---|
| Ordner mit `.log`-Dateien | `loglem source add-folder --name web --path /var/log/myapp` |
| Eine Datei (und ihre rotierten Kopien) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Caddy-Zugriffslog | `loglem source add-caddy --name site --path /var/log/caddy/access.log` |
| Podman-Container | `loglem source add-podman --name db --container db` |
| Docker-Container | `loglem source add-docker --name db --container db` |
| Kubernetes-Pods | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Linux-Journal | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` sucht die Logs von Caddy, Nginx und Apache auf diesem Server. Die Webseite bietet sie mit einem Klick an.

Komprimierte `.gz`-Dateien werden auch gelesen. Loglem importiert alten Verlauf einmal und dupliziert nie eine Zeile.

## 2. Suchen und live verfolgen

Öffnen Sie **Logs erkunden**. Geben Sie einen Text ein, wählen Sie Datumsangaben, oder öffnen Sie **Erweiterte Filter**: Schweregrad, IP-Adresse, Pfad, Statuscode.

Neue Zeilen erscheinen oben von selbst. Im Terminal:

```sh
loglem search --text login --ignore-case --status 404 --source site
loglem tail --source site --ip 203.0.113.7
```

Für Statistiken eines Zugriffslogs (Anfragen pro Tag, meistbesuchte Seiten, Statuscodes, häufigste IPs): `loglem stats site`, oder die Schaltfläche **Statistiken** auf der Quellenkarte.

## 3. Aufbewahrungsdauer wählen

Jede Quelle hat eine Dauer in Tagen, Monaten oder Jahren. Alte Kopien werden für Sie gelöscht. Die Originaldateien werden nicht angefasst.

```sh
loglem source set site --days 28
```

## 4. Loglem laufen lassen

`loglem serve` sammelt die Logs und stellt die Webseite bereit. Die Seite behält ihre Adresse, auch nach einem Neustart.

```sh
loglem service install --start    # Linux: systemd. macOS: launchd.
loglem service status
loglem check                      # Exit-Code 1, wenn etwas nicht stimmt
```

Der Dienst startet nach einem Absturz von selbst neu. Bei einem normalen Stopp liest er jede Quelle ein letztes Mal. Was passiert, solange Loglem aus ist: [stopping.md](stopping.md).

## 5. Loglem die Originaldateien löschen lassen

Standardmäßig aus. Loglem fasst die Datei, in die ein Programm gerade schreibt, nie an. Es löscht nur fertige Dateien, nach dieser Prüfung:

1. Loglem kopiert die Datei in sein Archiv.
2. Es vergleicht die Kopie mit dem Original, Byte für Byte. Eine fehlende oder abgelaufene Zeile stoppt die Prüfung.
3. Es wartet, bis die Datei **24 Stunden** lang niemand geändert hat.
4. Erst dann löscht es das Original.

Schlägt ein Schritt fehl, bleibt das Original.

**Caddy.** Fügen Sie die Quelle mit `add-caddy` hinzu. Loglem liest die rotierten Dateien (auch `.gz`) und löscht sie nach der Prüfung. Nutzen Sie den `log`-Block, den die Seite anzeigt. `roll_keep_for 60d` ist ein Sicherheitsnetz.

**Andere Programme.** Lassen Sie das Programm *fertige* Dateien in einem eigenen Ordner ablegen, dann:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Geben Sie nie einen Ordner an, in den ein Programm noch schreibt.

## 6. Die Seite von einem anderen Computer öffnen

Standardmäßig aus: Die Seite antwortet nur auf diesem Computer. Um sie anderswo zu öffnen:

```sh
loglem access user-add anna --role admin     # reader = darf nur ansehen
loglem access address 10.0.0.1               # eine Adresse dieses Servers, zum Beispiel die des VPN
```

Starten Sie Loglem neu. Fünf falsche Passwörter sperren diesen Computer für 15 Minuten. Die Verbindung ist **nicht verschlüsselt**: Nutzen Sie ein VPN oder einen Reverse-Proxy mit HTTPS:

```sh
loglem access proxy --trust 127.0.0.1 --base-path /loglem
```

Das gibt einen Caddyfile-Block aus. Um von einem anderen Computer live zu folgen: `loglem tail --server http://10.0.0.1:8790 --user anna`.

## 7. Warnungen und Sicherung

```sh
loglem alerts add-pattern fehlanmeldungen --text "Failed password" --count 5 --minutes 10
loglem alerts command 'echo "$LOGLEM_ALERTS" | mail -s Loglem me@example.com'
loglem backup /backups/loglem-2026-11-17
loglem backup-verify /backups/loglem-2026-11-17
```

Warnungen erscheinen auf der Seite, in `loglem check` und in `data/health.json`. **Als gesehen markieren** blendet eine aus, bis sie wiederkommt. Eine Sicherung enthält auch Zeilen, die inzwischen abgelaufen sind.

## 8. Wo Ihre Daten liegen

```text
data/
├── config.json     Quellen und Einstellungen
├── archives/       gespeicherte Logs, eine Datenbank je Quelle
├── audit/          wer gesucht, exportiert oder gelöscht hat
├── proof/          Siegelregister (Beweismodus)
└── diagnostics/    Loglems eigene Fehler
```

Für einen anderen Ort nutzen Sie `--data-dir /absoluter/pfad` oder `LOGLEM_DATA_DIR`. Einstellungen verschieben Sie mit `loglem config export` und `config import`. Logs verschieben Sie, indem Sie Loglem stoppen und `data/` kopieren.

Archive älterer Versionen werden beim Start umgewandelt. Der gespeicherte Text bleibt gleich, die Datei wird kleiner.

## 9. Wenn etwas schiefgeht

- Eine Quelle zeigt einen Fehler: Die anderen laufen weiter. Öffnen Sie ihre Karte oder `loglem status`.
- Loglems eigene Fehler: **Probleme** auf der Seite, oder `loglem diagnostics`.
- Port belegt: `loglem config web-port 8790`, dann neu starten.
- Programm prüfen: `loglem self-check`.

## 10. Aktualisieren

Drücken Sie auf der Seite **System** auf **Prüfen**, oder:

```sh
loglem update check
loglem update install
```

Es werden nur signierte Versionen installiert (Linux). `data/` bleibt erhalten. Das alte Programm bleibt als `loglem.previous`.

## 11. In einem Container

```sh
podman build -t loglem -f Containerfile .
podman run -d --name loglem -p 127.0.0.1:8790:8790 \
  -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='ein langes Passwort' \
  -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro loglem
```

Öffnen Sie `http://127.0.0.1:8790` und fügen Sie `/logs/myapp` als Ordnerquelle hinzu. Die Daten bleiben im Volume `loglem-data`. Podman, Docker und das Journal des Hosts sind von innen nicht erreichbar: Dafür starten Sie Loglem auf dem Host, oder binden die Logdateien in `/logs` ein. Um Originaldateien aufzuräumen, binden Sie den Ordner ohne `:ro` ein.

Personenbezogene Daten, DSGVO-Werkzeuge und Beweismodus: [privacy.md](privacy.md).
