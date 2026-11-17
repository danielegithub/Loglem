# Einschränkungen

Was Loglem nicht kann oder nicht verspricht.

## Plattformen

- Linux und macOS. Der Dienst ist systemd (Linux) oder launchd (macOS). Signierte Updates: nur Linux.
- Windows: Der Code ist portabel geschrieben, wurde aber nicht kompiliert oder getestet.
- Es gibt noch keine veröffentlichte Version.
- In einem Container liest Loglem nur die Logdateien, die Sie einbinden. Podman, Docker, Kubernetes und das Journal des Hosts brauchen Loglem auf dem Host.

## Was es liest

- UTF-8-Textzeilen bis 1 MiB. Andere Kodierungen werden nicht gelesen.
- Die Filter für Schweregrad, Statuscode und Pfad lesen den Text der Zeile. Sie kennen Caddy (JSON), Nginx und Apache (combined) und das Journal. Andere Formate findet man nur über den Text.
- Statistiken funktionieren mit Caddy-Logs als JSON und Nginx/Apache im Format combined.
- Reguläre Ausdrücke gibt es noch nicht.
- Statistiken halten jede verschiedene IP und Seite des Zeitraums im Speicher. Bei Logs mit Millionen verschiedener IPs braucht es mehr RAM (etwa 56 Byte je verschiedene IP oder Seite).

## Was verloren gehen kann

- Podman, Docker und Kubernetes behalten Logs nur, solange der Container oder Pod existiert. Wird er entfernt, während Loglem aus ist, gehen diese Zeilen verloren. Loglem zeigt eine **Lücke** auf der Karte.
- Folgen Sie bei Kubernetes der Anwendung über das Label (`app=web`). Ein einzelner Pod verschwindet bei jedem Release.
- Ist Loglem aus, laufen Zeilen nicht ab und es wird nichts aufgeräumt. Siehe [stopping.md](stopping.md).
- Volle Platte: Dateien, die noch nicht archiviert sind, bleiben erhalten. Loglem kann keinen Platz schaffen.

## Datensicherheit

- Archive sind SQLite-Dateien auf einer lokalen Platte, nicht auf einer Netzfreigabe. Stoppen Sie Loglem, bevor Sie sie von Hand kopieren, oder nutzen Sie `loglem backup`.
- Eine Zeile in Loglem zu löschen, löscht sie nicht aus Sicherungen oder aus den Originaldateien.
- Loglem verschlüsselt keine Daten auf der Platte. Nutzen Sie eine verschlüsselte Platte und verschlüsselte Sicherungen.
- Die Verbindung zur Webseite ist nicht verschlüsselt. Nutzen Sie ein VPN oder einen Reverse-Proxy mit HTTPS.
- Loglem kann nicht wissen, ob ein Programm mit einer Datei fertig ist. Das Aufräumen wartet 24 Stunden und fasst die Datei in Benutzung nie an.
- Der Beweismodus zeigt, dass sich ein versiegelter Tag nicht geändert hat. Er beweist nicht, dass die Zeilen wahr waren. Siehe [privacy.md](privacy.md).

## Was getestet wurde

Getestet auf macOS mit Podman, journald (in einem Linux-Container), Caddy, Chrome und einer echten Zeitstempel-Autorität: Quellen über die Seite hinzufügen, Suche, Live-Ansicht, Anmeldung mit Passwort, Neustart ohne verlorene oder doppelte Zeilen, Ablauf, Caddy-Rotation, Reverse-Proxy, Sicherung und Wiederherstellung, Beweismodus, Löschen.

Nicht getestet: der systemd-Dienst auf einem echten Linux, ein echter Kubernetes-Cluster (es wurde ein unechtes `kubectl` benutzt), der launchd-Agent von macOS, Windows und ein Update aus einer veröffentlichten Version.
