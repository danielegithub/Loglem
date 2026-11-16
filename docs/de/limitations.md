# Einschränkungen

Was Loglem in dieser Version nicht kann oder nicht garantiert.

**Was unterstützt wird**

- Nur Linux. Windows wird nicht unterstützt; unter macOS gibt es keinen automatischen Dienst (benutzen Sie `loglem web`).
- Quellen: `.log`-Textdateien (UTF-8), Podman-Container und Zugriffslogs von Caddy. Komprimierte Dateien (`.gz`) werden nicht gelesen. Docker, Kubernetes, Nginx und das systemd-Journal werden noch nicht unterstützt.
- Zeilen bis 1 MiB.
- Die Suche findet exakten Text unter Beachtung der Groß- und Kleinschreibung sowie Datumsbereiche. Filter nach Schweregrad, IP-Adresse, Pfad oder Antwortcode gibt es noch nicht.
- Die Webseite lässt sich nur auf dem Rechner öffnen, auf dem Loglem läuft (für einen entfernten Server einen SSH-Tunnel verwenden).

**Datensicherheit**

- Podman-Logs werden als Momentaufnahmen gelesen. Wird ein Container gelöscht, bevor Loglem ihn liest, oder läuft Loglem längere Zeit nicht, können Zeilen verloren gehen.
- Einträge laufen nur ab, solange Loglem läuft (oder wenn Sie `loglem prune --apply` ausführen).
- Das Löschen eines Eintrags aus dem Archiv von Loglem löscht ihn nicht aus Dateisystem-Snapshots oder Backups, die ihn schon kopiert haben.
- Archive sind SQLite-Dateien und müssen auf einer lokalen Platte liegen, nicht auf einer Netzwerkfreigabe. Kopieren Sie sie nicht, während Loglem läuft; stoppen Sie es zuerst.
- Ist die Platte voll, bleiben noch nicht archivierte Originale erhalten; Loglem kann weder Platz schaffen noch andere Programme am Schreiben hindern.
- Loglem kann nicht selbst erkennen, dass ein Programm mit einer Datei fertig ist. Das Aufräumen wartet 24 Stunden und berührt nie die Datei, in die gerade geschrieben wird; schalten Sie es aber nur für Ordner und Caddy-Logs ein, die wie im Leitfaden beschrieben eingerichtet sind.
- Archive sind nicht manipulationssicher: Loglem ist kein Beweisarchiv.

**Getestet und noch nicht getestet**

- Getestet mit echtem Podman und einem echten Browser: Quellen über die Seite hinzufügen, Suche, Neustart ohne verlorene oder doppelte Zeilen, Ablauf und simulierte Rotation im Caddy-Stil.
- Noch nicht getestet: der systemd-Dienst auf einer realen Linux-Maschine, Caddy im tatsächlichen Betrieb und die Rückkehr zur vorherigen Version nach einem fehlgeschlagenen Update.
- Es gibt noch keine veröffentlichte Version.
