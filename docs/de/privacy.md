# Personenbezogene Daten und Beweismodus

Logs enthalten personenbezogene Daten. Eine IP-Adresse ist ein personenbezogenes Datum. Loglem gibt Ihnen Werkzeuge, um die Regeln einzuhalten. Es macht eine Installation **nicht** von allein konform: Warum Sie Logs aufbewahren, wie lange und was Sie den Menschen sagen, bleibt Ihre Entscheidung.

## Weniger aufbewahren

- **Kurze Aufbewahrung.** Stellen Sie die Tage je Quelle ein. Abgelaufene Zeilen werden gelöscht.
- **IPs maskieren.** `203.0.113.77` wird zu `203.0.113.0`. Sofort oder nach N Tagen:

```sh
loglem privacy set site --mask-ips-after-days 0
loglem privacy set site --keep-fields request.uri,status
```

- **Nur einige JSON-Felder behalten.** Alle anderen werden verworfen.
- **Sehen, was Sie aufbewahren:** `loglem privacy report`. Nehmen Sie es in Ihr Verzeichnis der Verarbeitungstätigkeiten auf.

Das Maskieren ändert nur die gespeicherte Kopie. Die Prüfung vor dem Löschen einer Originaldatei funktioniert weiter. Im Beweismodus ist Maskieren nur „sofort“ erlaubt (später würde es bereits versiegelte Zeilen ändern).

## Die Daten einer Person löschen

Jemand bittet Sie, seine Daten zu löschen. Nutzen Sie die Seite **Personenbezogene Daten**, oder:

```sh
loglem erase --ip 203.0.113.7                                    # nur Vorschau
loglem erase --ip 203.0.113.7 --apply --reason "Antrag Art. 17 vom 17.11.2026, Vorgang 3"
loglem erase --ip 203.0.113.7 --originals --apply --reason "…"   # auch aus den Originaldateien
```

Loglem entfernt die Zeilen aus **allen** Quellen. Der Grund kommt ins Zugriffsprotokoll. Die IP wird dort nie geschrieben.

**Originaldateien** (`--originals`, oder das Kästchen auf der Seite). Loglem schreibt fertige Logdateien ohne diese Zeilen neu. Nur wenn:

- die Datei nicht die ist, in die ein Programm gerade schreibt,
- sie seit 24 Stunden niemand geändert hat,
- die ganze Datei schon im Archiv liegt.

Es schreibt eine neue Datei neben die alte, prüft sie und tauscht sie dann aus. Stimmt etwas nicht, bleibt das Original. Übersprungene Dateien werden mit Grund aufgelistet. Wiederholen Sie den Befehl nach der nächsten Rotation. Auch `.gz`-Dateien werden neu geschrieben.

Es fasst **nicht** an:

- die Logs von Podman, Docker, Kubernetes und des Journals. Das sind keine Dateien von Loglem, und das Journal kann einzelne Einträge nicht entfernen. Verkürzen Sie ihre Aufbewahrung, oder erstellen Sie den Container neu.
- bereits erstellte Sicherungen (sie laufen mit ihrer eigenen Rotation ab),
- die Datei, in die ein Programm schreibt, bis sie rotiert wurde.

Schreiben Sie in `--reason` eine Vorgangsnummer, keine E-Mail-Adresse: Auch eine E-Mail ist ein personenbezogenes Datum.

## Die Daten einer Person exportieren

Jemand fragt, welche Daten Sie über ihn haben:

```sh
loglem export --ip 203.0.113.7 --output /sicherer/ort/antrag-3.txt   # --json für JSON
```

Nur der Eigentümer kann die Datei lesen. Löschen Sie sie nach dem Versand.

## Wer hingesehen hat

`loglem audit` (oder die Dateien in `data/audit/`) listet jede Anmeldung, Suche, jeden Export, jedes Löschen und jede Änderung der Einstellungen auf, mit Benutzer und Uhrzeit. Gesuchte Werte werden **nicht** geschrieben, nur die Art des Filters („Text, ip“). Die Einträge werden 13 Monate aufbewahrt.

## Platte und Sicherungen

- Loglem verschlüsselt seine Dateien nicht. Nutzen Sie eine verschlüsselte Platte.
- `loglem backup` schreibt Dateien im Klartext. Verschlüsseln Sie sie, bevor sie den Server verlassen (zum Beispiel mit restic).
- Legen Sie keine Kopien von Logs mit personenbezogenen Daten in einem Speicher ab, aus dem Sie nicht löschen können (etwa einem gesperrten Bucket).

## Beweismodus

Für eine Quelle, die beweisen muss, dass ihre Zeilen nicht verändert wurden:

```sh
loglem proof enable site --tsa https://freetsa.org/tsr     # --tsa ist optional
loglem proof verify site
```

Jeder fertige Tag erhält einen Fingerabdruck (SHA-256) seiner Zeilen. Darin steckt auch der Fingerabdruck des Vortags: Ändert man einen, brechen alle folgenden. Mit `--tsa` signiert eine externe Stelle jeden Fingerabdruck mit der Uhrzeit; sie sieht nur den Fingerabdruck.

`loglem proof verify` berechnet jeden Tag neu und sagt **intakt**, **VERÄNDERT** oder **abgelaufen**. Sie können auch eine Kopie prüfen: `loglem proof verify --archive kopie.db --chain kopie.chain`.

Was es beweist: Ein versiegelter Tag hat sich nach dem Siegel (oder nach der Zeit des Zeitstempels) nicht geändert. Es beweist nicht, dass die Zeilen beim Schreiben wahr waren.

Zeilen laufen nur in ganzen Tagen ab, und jeder Ablauf wird im Register vermerkt.

**Löschen im Beweismodus.** Das Löschen gewinnt. Loglem entfernt die Zeilen, **berechnet** den Fingerabdruck dieses Tages und aller folgenden Tage **neu** und fügt dem Register eine Notiz hinzu: wann, warum, wie viele Zeilen, alter Fingerabdruck → neuer. `proof verify` zeigt die Notiz. Die alten Zeitstempel bleiben für die alte Version gültig. Für die neuen Fingerabdrücke werden neue angefordert.
