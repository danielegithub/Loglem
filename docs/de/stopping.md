# Wenn Loglem stoppt

Loglem kann aus sein: ein Absturz, ein Neustart, ein Update, ein Fehler. Das verlieren Sie.

| Quelle | Während Loglem aus ist | Was verloren geht |
|---|---|---|
| Dateien, Caddy, Nginx | Die Programme schreiben und rotieren weiter. | Nichts, bis das Programm alte Dateien selbst löscht (`roll_keep`, `roll_keep_for`, logrotate). |
| Podman, Docker | Das Container-Programm behält die Logs. | Zeilen, die es gelöscht hat, und alle, wenn der Container entfernt oder neu erstellt wurde. |
| Kubernetes | Die Pods behalten die Logs. | Die Zeilen eines ersetzten Pods. Folgen Sie `app=name`, nicht einem einzelnen Pod. |
| Linux-Journal | Das Journal behält die Einträge. | Einträge, die das Journal wegen Platz oder Alter gelöscht hat. |

Solange Loglem aus ist, werden abgelaufene Zeilen nicht gelöscht und Originaldateien nicht aufgeräumt.

## Was Loglem tut

- **Beim erneuten Start** liest es ab dem gespeicherten Punkt. Die Karte sagt „Fortgesetzt nach 3 Stunden Pause: N Zeilen nachgeholt“.
- **Lücken sind sichtbar.** Können Zeilen fehlen, sagen es die Karte, das Warnfeld und `loglem check`, mit geschätztem Anfang und Ende.
- **Normaler Stopp.** Bei einem normalen Stopp (`systemctl stop`, ein Update) liest es jede Quelle ein letztes Mal, höchstens 30 Sekunden lang.
- **Absturz.** Beim nächsten Start sagt es, dass der letzte Stopp nicht sauber war. Es macht am letzten gespeicherten Punkt weiter. Eine Zeile wird zusammen mit ihrer Leseposition gespeichert, also wird nichts doppelt gelesen.
- **Automatischer Neustart.** Der Dienst startet nach einem Absturz neu, höchstens 10-mal in 5 Minuten.
- **Warnung.** `loglem check` schlägt fehl, wenn Loglem aus ist oder eine Quelle seit 15 Minuten nicht gelesen wurde. Starten Sie es aus cron oder einem Monitor.
- **Caddy-Spielraum.** Loglem warnt, wenn `roll_keep_for` unter 7 Tagen liegt oder ungelesene rotierte Dateien nahe an `roll_keep` kommen.

## Für mehr Sicherheit

- Container-Logs: Folgen Sie dem Container per Name über das Journal, wenn er dorthin schreibt (die Seite bietet es an). Das überlebt ein neues Release.
- Kubernetes: Nutzen Sie `--selector app=name`.
- Halten Sie `roll_keep_for` von Caddy bei 30 Tagen oder mehr.
- Starten Sie `loglem check` alle paar Minuten.
