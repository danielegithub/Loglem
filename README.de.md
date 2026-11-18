# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Ein Ort für die Logs Ihres Servers: wo sie liegen, wie viel Platz sie brauchen und wie lange sie bleiben.**

Loglem liest Ihre Logs, kopiert sie in ein durchsuchbares Archiv und löscht die Kopien, wenn die gewählte Zeit abgelaufen ist. Sie nutzen eine Webseite oder ein Terminalmenü. Keine Konfigurationsdateien.

> **Status: experimentell.** Es funktioniert und ist auf macOS getestet. Es gibt noch keine veröffentlichte Version, und einige Teile sind auf einem echten Linux nicht getestet. Siehe die [Einschränkungen](docs/de/limitations.md).

## Inhalt

- [Warum Loglem](#warum-loglem)
- [Was es liest](#was-es-liest)
- [Sicheres Aufräumen der Originaldateien](#sicheres-aufräumen-der-originaldateien)
- [Schnellstart](#schnellstart)
- [Installation](#installation)
- [Dokumentation](#dokumentation)
- [Lizenz](#lizenz)

## Warum Loglem

Logs sammeln sich an vielen Orten. Niemand weiß, wo sie liegen, wie groß sie sind oder wann man sie löschen sollte.

- **Sie sehen alles.** Eine Liste: jede Quelle, wo sie liegt, die Größe, der Zustand.
- **Sie bestimmen die Dauer.** 28 Tage, 6 Monate, 2 Jahre, je Quelle. Alte Kopien verschwinden von selbst.
- **Sie können suchen.** Text, Datum, IP, Pfad, Statuscode. Auch nachdem die Datei rotiert oder der Container entfernt wurde. Neue Zeilen erscheinen live.
- **Es bleibt klein.** Ein Programm von etwa 4 MB. Etwa 5 MB RAM (auf Linux gemessen). Das Archiv braucht etwa ein Zehntel des Platzes der Original-Logs. Kein Datenbankserver, kein Konto, keine Cloud. Ihre Logs bleiben auf Ihrer Maschine. [Messungen (auf Englisch)](docs/performance.md).
- **Es ist standardmäßig sicher.** Originaldateien werden nur gelesen. Sie zu löschen ist optional und wird geprüft (siehe unten).
- **Es ist einfach.** Eine Webseite, ein Terminalmenü, das über SSH funktioniert, und Befehle für Skripte.
- **Es zeigt, was schiefläuft.** Eine gestoppte Quelle, eine Lücke in den Logs, eine volle Platte: Sie sehen es, und `loglem check` meldet es Ihrem Monitoring.
- **Es hilft bei personenbezogenen Daten.** IPs maskieren, die Zeilen einer Person löschen oder exportieren, sehen, wer die Logs angesehen hat. Siehe [Datenschutz](docs/de/privacy.md).

## Was es liest

| Quelle | Hinweise |
|---|---|
| Ordner und Textdateien `.log` | Auch rotierte Dateien, auch `.gz` (der Verlauf wird einmal importiert, nie doppelt). |
| Webserver, Datenbanken, Mail, SSH, Cron… | Erkannt über ihre Rezepte in `src/recipes/`: Nginx, Apache, Caddy, Traefik, HAProxy, PostgreSQL, MySQL und weitere. Eines hinzuzufügen ist eine einzige Datei. |
| Podman, Docker | Ein Container, oder per Name über das Journal. |
| Kubernetes | Ein Pod, oder alle Pods einer Anwendung. |
| Linux-Journal | Alles, oder einzelne Dienste, Programme oder Schweregrade. |

## Sicheres Aufräumen der Originaldateien

Standardmäßig aus. Wenn Sie es für eine Quelle einschalten, löscht Loglem eine fertige Datei erst nach dieser Prüfung:

1. Es **kopiert** die Datei in sein Archiv.
2. Es **vergleicht die Kopie mit dem Original, Byte für Byte.** Fehlt eine Zeile, ist sie abgelaufen oder hat sie sich geändert, bricht es ab.
3. Es wartet, bis die Datei **24 Stunden** lang niemand geändert hat.
4. Erst dann löscht es das Original.

Die Datei, in die ein Programm noch schreibt, wird nie angefasst. Schlägt ein Schritt fehl, bleibt das Original, wo es ist. Gilt für rotierte Dateien jedes Programms und für Ordner mit fertigen Dateien.

## Schnellstart

Nach der [Installation](#installation):

```sh
loglem                                  # Menü im Terminal
loglem discover                         # sucht Logs in der Konfiguration bekannter Programme
loglem source add-folder --name web --path /srv/example/logs --days 28
loglem collect
loglem search --source web --text '/login' --from 2026-11-01
loglem web                              # liest und öffnet die Webseite (zeigt die Adresse)
```

Damit es nach dem Schließen des Terminals weiterläuft (Linux: systemd, macOS: launchd):

```sh
loglem service install --start
```

Menü und Webseite sind auf Italienisch. Mehr in [Loglem verwenden](docs/de/usage.md).

## Installation

Sobald eine Version veröffentlicht ist, laden Sie das Paket für Ihre Maschine im Reiter **Releases** herunter:

| Maschine | Ende des Paketnamens |
|---|---|
| Linux Intel/AMD 64 Bit | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Die Pakete brauchen glibc 2.39 oder neuer (zum Beispiel Ubuntu 24.04). Updates, Zeitstempel und `loglem tail --server` nutzen den Befehl `curl` des Systems.

Prüfen Sie die Signatur mit dem öffentlichen Schlüssel des Herausgebers (`release-public.pem`), den Sie über einen unabhängigen Weg erhalten haben:

```sh
openssl pkeyutl -verify -pubin -inkey release-public.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
sha256sum --check --ignore-missing SHA256SUMS
```

Dann installieren:

```sh
tar xzf loglem-*.tar.gz
cd loglem-*
sh install.sh
~/Loglem/loglem
```

Alles liegt in `~/Loglem`: das Programm und ein Ordner `data/`. Das Installationsprogramm startet keinen Dienst. Zum Selbstkompilieren siehe [CONTRIBUTING.md](CONTRIBUTING.md).

**In einem Container.** Bauen Sie ihn mit `podman build -t loglem -f Containerfile .` und starten Sie ihn mit eingebundenem Logordner. Siehe [Loglem verwenden](docs/de/usage.md#11-in-einem-container).

## Dokumentation

[English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Seite | Inhalt |
|---|---|
| [Loglem verwenden](docs/de/usage.md) | Wenige Schritte für alles |
| [Befehle](docs/de/cli.md) | Alle Befehle |
| [Personenbezogene Daten und Beweismodus](docs/de/privacy.md) | DSGVO-Werkzeuge, Löschen, Export, Zugriffsprotokoll, versiegelte Tage |
| [Wenn Loglem stoppt](docs/de/stopping.md) | Was Sie verlieren und was Loglem tut |
| [Einschränkungen](docs/de/limitations.md) | Was es nicht kann oder nicht verspricht |

## Lizenz

[MIT](LICENSE).
