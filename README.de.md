# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Ein Ort für die Logs Ihres Servers: wo sie liegen, wie viel Platz sie belegen und wie lange sie aufbewahrt werden.**

Loglem liest die Logs Ihres Linux-Servers (`.log`-Dateien und Podman-Container), kopiert sie in ein durchsuchbares Archiv und löscht die archivierten Einträge automatisch, sobald die von Ihnen gewählte Aufbewahrungsdauer abgelaufen ist. Sie verwalten alles über eine lokal auf dem Server erreichbare Webseite oder über ein per Tastatur bedienbares Terminalmenü, ohne Konfigurationsdateien zu bearbeiten.

> **Status: experimentell.** Es funktioniert und ist getestet, aber es gibt noch keine veröffentlichte Version, und einige Teile müssen noch vollständig auf einem echten Server getestet werden. Siehe die [Einschränkungen](docs/de/limitations.md).

## Inhalt

- [Warum Loglem](#warum-loglem)
- [Was es tut](#was-es-tut)
- [Schnellstart](#schnellstart)
- [Installation](#installation)
- [Dokumentation](#dokumentation)
- [Lizenz](#lizenz)

## Warum Loglem

Logs sammeln sich an vielen Stellen: ein Ordner für den Webserver, einer für die Anwendung, die Ausgabe jedes Containers. Niemand weiß, wo sie liegen, wie groß sie geworden sind oder wann sie gelöscht werden sollten.

Mit Loglem:

- **Sie wissen immer, wo die Logs liegen.** Eine Liste zeigt jede Quelle, den ursprünglichen Ort, den Ort des Archivs, die Größe und den Stand der letzten Erfassung.
- **Sie bestimmen, wie lange die Logs jeder Quelle aufbewahrt werden.** 28 Tage, 6 Monate oder 2 Jahre, je Quelle. Abgelaufene Einträge werden für Sie entfernt: Die Festplatte läuft nicht voll, und alte Daten bleiben nicht aus Versehen liegen.
- **Sie können suchen.** Text und Datumsbereiche in allem, was erfasst wurde, auch nachdem die Originaldatei rotiert oder der Container entfernt wurde.
- **Es bleibt unauffällig.** Eine einzige ausführbare Datei von etwa 4 MB, rund 10 MB RAM im Leerlauf, kein Datenbankserver, kein Konto, kein Cloud-Dienst. Ihre Logs verlassen nie die Maschine.
- **Es ist standardmäßig sicher.** Originaldateien werden nur gelesen, nie verändert. Das Löschen eines Originals ist optional und gilt nur für Dateien, die Sie ausdrücklich nach Abschluss aller Schreibzugriffe an Loglem übergeben.
- **Es ist einfach zu bedienen.** Alles läuft über eine Seite, die nur auf dem Server selbst erreichbar ist, oder über ein Terminalmenü, das auch per SSH funktioniert. Befehle für Skripte und Automatisierung gibt es ebenfalls.
- **Es ersetzt einen Haufen Skripte.** Logs zu sammeln, aufzuräumen, aufzubewahren und zu durchsuchen bedeutet meist eine Mischung aus Rotationsregeln, geplanten Aufgaben und improvisierten Befehlen. Loglem erledigt das an einem Ort, mit denselben Einstellungen auf der Webseite, im Terminal und in der Kommandozeile.
- **Es zieht mit Ihnen um.** Pfade, Container und Zeiträume sind Einstellungen, kein Code: Exportieren Sie die Konfiguration und importieren Sie sie auf einer anderen Maschine.

## Was es tut

1. **Sie fügen eine Quelle hinzu:** einen Ordner mit `.log`-Dateien, eine einzelne Datei, einen Podman-Container oder das Zugriffslog von Caddy.
2. **Loglem erfasst neue Zeilen** standardmäßig alle 10 Sekunden und speichert sie in einem kleinen SQLite-Archiv für diese Quelle. Es merkt sich, wo es aufgehört hat, sodass nichts doppelt gelesen wird.
3. **Sie suchen und prüfen** auf der Webseite oder im Terminal und sehen den Zustand jeder Quelle.
4. **Alte Einträge werden entfernt,** wenn sie die für diese Quelle eingestellte Dauer überschreiten.
5. **Wenn etwas schiefgeht,** werden die Fehler von Loglem mit ihrer Ursache gespeichert und auf einer Diagnose-Seite angezeigt. Eine fehlerhafte Quelle hält die anderen nicht auf.

Es läuft als Hintergrunddienst (ein systemd-Benutzerdienst) oder bei Bedarf. Es läuft unter Linux, ohne grafische Oberfläche.

## Schnellstart

Nach der [Installation](#installation):

```sh
# Öffnet das geführte Terminalmenü (Pfeiltasten, Eingabe, Esc)
loglem

# Oder einen Log-Ordner per Kommandozeile hinzufügen und einmal lesen
loglem source add-folder --name web-logs --path /srv/example/logs --days 28
loglem collect
loglem status

# Suchen
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16

# Öffnet die Webseite (gibt ihre eigene Adresse aus, zum Beispiel http://127.0.0.1:53142)
loglem web
```

Damit auch nach dem Schließen des Terminals weiter erfasst wird:

```sh
loglem service install --start
```

Terminalmenü und Webseite verwenden derzeit italienische Beschriftungen. Details unter [Loglem verwenden](docs/de/usage.md).

## Installation

Sobald eine Version veröffentlicht ist, laden Sie im Reiter **Releases** dieses Repositorys das Paket für Ihre Maschine herunter:

| Maschine | Paketsuffix |
|---|---|
| Linux Intel/AMD 64-Bit | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Die Pakete benötigen glibc 2.39 oder neuer (zum Beispiel Ubuntu 24.04).

Prüfen Sie vor der Installation die Signatur mit dem öffentlichen Schlüssel des Herausgebers (`release-public.pem`), den Sie über einen unabhängigen Weg erhalten haben:

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

Alles liegt in `~/Loglem`: das Programm und ein Ordner `data/` mit Einstellungen, Archiven und Diagnoseprotokollen. Das Installationsprogramm aktiviert von sich aus keinen Dienst. Für Podman-Quellen muss Podman installiert und für denselben Benutzer nutzbar sein.

Zum Selbstkompilieren siehe [CONTRIBUTING.md](CONTRIBUTING.md).

## Dokumentation

Die Dokumentation gibt es in: [English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Seite | Inhalt |
|---|---|
| [Loglem verwenden](docs/de/usage.md) | Schritt-für-Schritt-Anleitung: Quellen hinzufügen, Suche, Aufbewahrung, Betrieb als Dienst, Bereinigung, Umzug auf einen anderen Rechner, Fehlerbehebung und Updates |
| [Befehlsreferenz](docs/de/cli.md) | Alle Befehle |
| [Einschränkungen](docs/de/limitations.md) | Was es nicht tut oder nicht garantiert |

## Lizenz

[MIT](LICENSE).
