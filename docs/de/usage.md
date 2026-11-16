# Loglem verwenden

Eine Schritt-für-Schritt-Anleitung. Die Liste der Befehle steht in [cli.md](cli.md); was Loglem nicht kann, steht in [limitations.md](limitations.md).

## Inhalt

- [1. Die erste Quelle hinzufügen](#1-die-erste-quelle-hinzufügen)
- [2. In den Logs suchen](#2-in-den-logs-suchen)
- [3. Festlegen, wie lange Logs aufbewahrt werden](#3-festlegen-wie-lange-logs-aufbewahrt-werden)
- [4. Loglem dauerhaft laufen lassen](#4-loglem-dauerhaft-laufen-lassen)
- [5. Loglem die Originaldateien aufräumen lassen](#5-loglem-die-originaldateien-aufräumen-lassen)
- [6. Eine Quelle ändern, pausieren oder entfernen](#6-eine-quelle-ändern-pausieren-oder-entfernen)
- [7. Wo Ihre Daten liegen und wie Sie sie umziehen](#7-wo-ihre-daten-liegen-und-wie-sie-sie-umziehen)
- [8. Wenn etwas nicht klappt](#8-wenn-etwas-nicht-klappt)
- [9. Loglem aktualisieren](#9-loglem-aktualisieren)

## 1. Die erste Quelle hinzufügen

Eine **Quelle** ist der Ort, von dem Logs stammen: ein Ordner mit `.log`-Dateien, eine einzelne Datei, ein Podman-Container oder das Zugriffslog von Caddy. Sie können sie über die Webseite, das Terminalmenü oder die Kommandozeile hinzufügen.

### Über die Webseite

```sh
loglem web
```

Loglem gibt die Adresse der Seite aus (zum Beispiel `http://127.0.0.1:53142`) und den Ordner, in dem es seine Daten ablegt. Öffnen Sie diese Adresse im Browser. Die Adresse bleibt nach einem Neustart dieselbe. Ist der Port von einem anderen Programm belegt, bricht Loglem ab und sagt es; wählen Sie mit `loglem config web-port 8790` einen anderen. Loglem benutzt nie Port 80 oder 443.

Die Seite funktioniert nur auf dem Rechner, auf dem Loglem läuft. Auf einem entfernten Server öffnen Sie sie über einen SSH-Tunnel (`ssh -L 53142:127.0.0.1:53142 benutzer@server`, dann `http://127.0.0.1:53142` aufrufen).

Das Menü links hat vier Seiten:

- **Sorgenti:** Ihre Quellen mit Zustand, Zeilenanzahl und Links zu ihren Logs und Einstellungen.
- **Esplora i log:** die Logs einer Quelle durchsuchen und lesen. Die Liste aktualisiert sich selbst; mit einem Kontrollkästchen lässt sie sich anhalten.
- **Aggiungi sorgente:** ein geführtes Formular. Wählen Sie Typ, Ordner oder Container, einen Namen, die Art des Logs und wie lange es aufbewahrt wird, und drücken Sie **Salva e apri i log**. Loglem liest die Logs sofort.
- **Problemi:** die Fehler von Loglem selbst samt Ursache (siehe [Abschnitt 8](#8-wenn-etwas-nicht-klappt)).

### Über die Kommandozeile

```sh
# ein Ordner mit .log-Dateien (neue Dateien werden automatisch erfasst)
loglem source add-folder --name web-logs --path /srv/example/logs --days 28

# eine einzelne Datei; liest auch ihre umbenannten Kopien, etwa api.log-1
loglem source add-file --name api --path /srv/example/logs/api.log \
  --rotated-pattern 'api.log-*' --days 14

# ein Podman-Container
loglem podman-list
loglem source add-podman --name app --container my-container --days 14

# jetzt lesen und prüfen
loglem collect
loglem status
```

Hinweise:

- Eine Ordnerquelle liest die `.log`-Dateien, die direkt im Ordner liegen; Unterordner, komprimierte Dateien und andere Endungen werden ignoriert.
- Der Dateiname muss auf `.log` enden. Komprimierte Dateien (`.gz`) werden nicht gelesen.
- Bei Podman merkt sich Loglem die vollständige ID des Containers. Wenn Sie einen Container mit demselben Namen neu erstellen, fügen Sie ihn als neue Quelle hinzu (oder siehe [Abschnitt 6](#6-eine-quelle-ändern-pausieren-oder-entfernen)). Die bereits gesammelte Historie bleibt auch nach dem Entfernen des Containers erhalten. Mit `--connection NAME` wählen Sie eine benannte Podman-Verbindung.
- Loglem liest Ihre Originaldateien nur. Es verändert sie nicht, es sei denn, Sie schalten das Aufräumen ein (Abschnitt 5).
- Die Bezeichnungen und Schaltflächen der Seite und des Menüs sind derzeit auf Italienisch.

### Über das Terminalmenü

Starten Sie `loglem`. Bedienen Sie es mit den Pfeiltasten (oder `j`/`k`), Eingabe zum Auswählen und Esc zum Zurückgehen.

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

Wählen Sie **Aggiungi cartella, file o container**, dann den Typ und navigieren Sie zum Ordner (oder wählen Sie **Scrivi / incolla un percorso…**, um ihn einzutippen). Prüfen Sie die Zusammenfassung und speichern Sie mit **Salva e leggi i log adesso**. Esc bricht vor dem Speichern ab.

## 2. In den Logs suchen

Öffnen Sie auf der Webseite **Esplora i log**, wählen Sie die Quelle und geben Sie Ihren Text ein. Sie können nach Datum (UTC) eingrenzen. Die neuesten Zeilen stehen oben. Über die Kommandozeile:

```sh
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16
```

- Die Suche findet exakten Text und unterscheidet Groß- und Kleinschreibung; es ist kein regulärer Ausdruck.
- `--to 2026-11-16` schließt den ganzen Tag (UTC) ein.
- Mit `--json` erhalten Sie eine für andere Programme lesbare Ausgabe, mit `--limit` bis zu 1000 Treffer.
- Hat eine Zeile kein Datum, das Loglem erkennt, bleibt sie durchsuchbar und wird mit dem Zeitpunkt angezeigt, zu dem sie gelesen wurde. Loglem erkennt gängige JSON-Logs (`ts`, `timestamp`, `time`, `@timestamp`).

## 3. Festlegen, wie lange Logs aufbewahrt werden

Jede Quelle hat ihre eigene Aufbewahrungsdauer: eine Anzahl von Tagen, Monaten oder Jahren. Überschreitet ein Eintrag diese Dauer, löscht Loglem ihn aus seinem Archiv.

```sh
loglem source set web-logs --days 7
loglem source configure web-logs --amount 6 --unit months --log-kind access
loglem source configure web-logs --amount 2 --unit years
```

- Auf der Webseite und im Terminalmenü (**Conservazione**) ist dieselbe Einstellung ein Formular, das auch zeigt, wann Einträge ablaufen.
- Abgelaufene Einträge werden entfernt, solange Loglem läuft (siehe Abschnitt 4). `loglem prune` zeigt, was gelöscht würde; `loglem prune --apply` löscht es sofort.
- Die Aufbewahrung zählt ab dem Datum der Zeile oder ab dem ersten Lesen durch Loglem, je nachdem, was früher ist: Eine Zeile mit Datum in der Zukunft bleibt nicht ewig.
- Das Pausieren einer Quelle stoppt neues Lesen, nicht die Aufbewahrung.
- Die Art des Logs (Zugriff, allgemein, Fehler) beschreibt nur die Quelle; sie legt keine gesetzliche Frist für Sie fest.
- Die Dauer gilt für die Kopie von Loglem. Ihre Originaldateien werden nicht gelöscht, es sei denn, Sie schalten das Aufräumen ein (Abschnitt 5).

## 4. Loglem dauerhaft laufen lassen

`loglem web` sammelt Logs nur, solange es läuft. Damit immer gesammelt wird, auch nach dem Schließen des Terminals oder einem Neustart, installieren Sie es als Dienst:

```sh
loglem service install --start
loglem service status
```

Das legt einen **systemd-Benutzerdienst** für den aktuellen Benutzer an. Er liest neue Zeilen alle 10 Sekunden (änderbar mit `loglem config poll 5`), entfernt abgelaufene Einträge und stellt die Webseite unter derselben Adresse bereit.

- Bitten Sie auf einem Server einen Administrator, einmal `sudo loginctl enable-linger BENUTZER` auszuführen, damit der Dienst auch läuft, wenn der Benutzer nicht angemeldet ist.
- Der Dienst hat nur die Rechte dieses Benutzers. Prüfen Sie, dass er die Log-Ordner lesen kann; Loglem läuft nicht als root.
- Pro Datenordner läuft nur ein Loglem. Läuft der Dienst bereits, startet `loglem web` kein zweites: Es gibt die Adresse des Dienstes aus und beendet sich.
- Wird Loglem mit einem anderen `--data-dir` gestartet, sehen Sie andere Quellen. Der verwendete Datenordner steht immer auf der Webseite und im Terminalmenü.

## 5. Loglem die Originaldateien aufräumen lassen

Standardmäßig löscht Loglem keine Originaldateien. Sie können es Originaldateien löschen lassen, **nachdem es sie kopiert und geprüft hat**. Das geschieht nur bei Dateien, die seit mindestens 24 Stunden niemand verändert hat, nie bei der Datei, in die ein Programm gerade schreibt, und jede Datei, die sich nicht prüfen lässt, bleibt erhalten (die Karte der Quelle zeigt eine Warnung). Das Löschen abgelaufener Archiveinträge ist ein getrennter Schritt, der in Abschnitt 3 eingestellt wird.

### Caddy

Wählen Sie unter **Aggiungi sorgente** **Il log di accesso di Caddy**, öffnen Sie den Log-Ordner (oft `/var/log/caddy`) und wählen Sie die Datei, in die Caddy schreibt, meist `access.log`. Loglem liest dann auch die Dateien, die Caddy beim Rotieren des Logs anlegt (zum Beispiel `access-2026-11-16T10-00-00.000.log`), und löscht sie nach der Prüfung. Am Ende zeigt die Seite den Block für Ihr Caddyfile:

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

- `roll_uncompressed` ist Pflicht: Loglem prüft nur unkomprimierte Dateien. Findet es `.gz`-Dateien, meldet es sie auf der Karte.
- `roll_keep_for 60d` ist ein Sicherheitsnetz: Bleibt Loglem lange aus, löscht Caddy die ältesten Dateien trotzdem.
- Loglem ändert Ihr Caddyfile nicht; den Block fügen Sie selbst ein.

Über die Kommandozeile: `loglem source add-caddy --name site --path /var/log/caddy/access.log --days 90`. Für eine Dateiquelle, die schon ein Rotationsmuster hat: `loglem source configure site --remove-rotated true`.

### Andere Programme

Lassen Sie das Programm jede fertige, geschlossene, unkomprimierte Datei in einen eigenen Ordner verschieben. Schalten Sie in den Einstellungen der Quelle **Archivia ed elimina i file di una cartella dedicata** ein und bestätigen Sie. Wählen Sie dafür niemals einen Ordner, in den ein Programm noch schreibt.

```sh
loglem source configure web --completed-dir /absolute/completed-files --handoff-confirmed
# zum Deaktivieren
loglem source configure web --completed-dir ""
```

Für eine einzelne fertige Datei, ohne 24 Stunden zu warten:

```sh
loglem archive-completed --source web --file /srv/example/logs/access-old.log --producer-closed --remove
```

Ohne `--remove` archiviert und prüft der Befehl die Datei, ohne das Original zu löschen. Loglem löscht nie die Originale eines Podman-Containers.

## 6. Eine Quelle ändern, pausieren oder entfernen

Drücken Sie auf der Webseite auf der Karte der Quelle **Impostazioni**. Über die Kommandozeile:

```sh
loglem source rename web "Öffentliche Website"
loglem source pause web          # Lesen stoppen, Historie bleibt durchsuchbar
loglem source resume web
loglem source detach web         # trennen; das Archiv bleibt durchsuchbar
loglem source configure web --container NEW_CONTAINER   # auf einen Ersatzcontainer zeigen
loglem source configure web --path /new/path            # neue Datei oder neuer Ordner
```

- Umbenennen ändert nur die Bezeichnung; die Historie bleibt erhalten.
- Um das Archiv einer Quelle in einem anderen Ordner zu halten: `loglem source configure web --archive-dir /absoluter/ordner`. Loglem kopiert es, prüft die Kopie und entfernt dann die alten Dateien; eine vorhandene Datenbank wird nie überschrieben. Mit `--archive-dir ""` kehren Sie zum Standardordner zurück. Schließen Sie vorher andere Programme, die das Archiv geöffnet haben.

## 7. Wo Ihre Daten liegen und wie Sie sie umziehen

Standardmäßig liegt alles im Loglem-Ordner:

```text
Loglem/
├── loglem          das Programm
└── data/
    ├── config.json   Ihre Quellen und Einstellungen
    ├── archives/     die gespeicherten Logs (eine Datenbank pro Quelle)
    ├── diagnostics/  das Fehlerprotokoll von Loglem
    └── updates/
```

Für einen anderen Speicherort geben Sie vor dem Befehl `--data-dir /absoluter/pfad` an oder setzen `LOGLEM_DATA_DIR`.

Für den Umzug auf eine andere Maschine:

```sh
loglem config export ./loglem-config.json     # auf der alten Maschine
loglem config import ./loglem-config.json     # auf der neuen Maschine
```

Das überträgt Ihre **Einstellungen** (Quellen, Zeiträume), nicht die Logs. Passen Sie Pfade und Berechtigungen auf der neuen Maschine an. Um auch die Logs mitzunehmen, stoppen Sie Loglem und kopieren den ganzen Ordner `data/`. Der Import einer Konfiguration entfernt nie die Aufbewahrungsregel einer Quelle, die schon ein Archiv hat; pausieren Sie in diesem Fall die Quelle.

## 8. Wenn etwas nicht klappt

- **Eine Quelle zeigt einen Fehler:** Die anderen arbeiten weiter, und bereits gespeicherte Zeilen bleiben lesbar. Öffnen Sie die Karte der Quelle für die Meldung oder führen Sie `loglem status` aus.
- **Die Seite ist leer oder eine Quelle meldet „nicht lesbar“:** Prüfen Sie, ob der Benutzer, der Loglem ausführt, den Ordner oder die Datei lesen kann.
- **Fehler von Loglem ansehen:** **Problemi** auf der Webseite oder `loglem diagnostics`. Jeder Eintrag enthält die Uhrzeit, was Loglem gerade tat, die Ursache und technische Details. Die Datei ist `data/diagnostics/loglem.log` (etwa 5 MB, dann Rotation; nur für den Besitzer lesbar).
- **Installation prüfen:** `loglem self-check`.
- **Port bereits belegt:** `loglem config web-port 8790`, dann neu starten.

## 9. Loglem aktualisieren

Am einfachsten lassen Sie Loglem sich selbst aus einer signierten Version aktualisieren. Der Ordner `data/` bleibt erhalten. (`install.sh` überschreibt keine bestehende Installation; um das Programm von Hand zu ersetzen, stoppen Sie zuerst den Dienst.)

```sh
loglem update check
loglem update install
loglem update status
```

Das funktioniert nur für ein Loglem, das in einem für den Benutzer beschreibbaren Ordner installiert und mit `loglem service install` registriert ist. Bevor etwas ersetzt wird, prüft es Signatur, Version und Größe; startet die neue Version nicht, bleibt das vorherige Programm daneben erhalten (`loglem.previous`). Bei einem lokal kompilierten Programm geben Sie zuerst an, wo gesucht wird und welchem Schlüssel vertraut wird: `loglem update configure --repository EIGENTUEMER/REPOSITORY --public-key BASE64_SCHLUESSEL`. Aktualisierungen, die das Datenformat ändern, werden abgelehnt.
