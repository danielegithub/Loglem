# Befehlsreferenz

Mit `loglem --help` oder `loglem BEFEHL --help` sehen Sie die Optionen der installierten Version. Ohne Befehl öffnet Loglem das per Tastatur bedienbare Terminalmenü; `loglem tui` tut dasselbe ausdrücklich.

| Befehl | Wofür |
|---|---|
| `init` | Legt eine leere Konfiguration an. |
| `source list` | Listet die Quellen auf. Zum Hinzufügen: `source add-folder`, `add-file`, `add-podman` oder `add-caddy` (Caddy-Log mit Aufräumen rotierter Dateien). |
| `podman-list` | Findet Container und Pods; `--connection` wählt eine benannte Verbindung. |
| `status [--json]` | Zeigt die Speicherorte und Größen der Archive, Datumsangaben und den Status der Erfassung. |
| `collect` / `serve` | Erfasst einmal, oder lässt den Dienst (Erfassung plus Webseite unter fester Adresse) bis zum Beenden laufen. `web` tut dasselbe oder gibt die Adresse aus, wenn der Dienst schon läuft. |
| `search [...]` | Durchsucht die Archive nach Text und Datumsbereichen; `--json` liefert maschinenlesbare Ausgabe. |
| `prune [--apply]` | Zeigt abgelaufene Einträge, oder löscht sie mit `--apply` aus den Archiven. |
| `config export FILE` / `config import FILE` | Exportiert oder importiert die Konfiguration der Quellen. |
| `config poll N` | Stellt das Intervall der automatischen Erfassung ein (in Sekunden). |
| `config web-port N` | Wählt den festen lokalen Port der Webseite (ab dem nächsten Start). |
| `service install [--start]` | Registriert einen systemd-Benutzerdienst; `start`, `stop` und `status` verwalten ihn. |
| `diagnostics` | Gibt gespeicherte Fehler von Loglem samt Stack-Traces aus. |
| `self-check` | Prüft die ausführbare Datei und das eingebaute SQLite-Schema, ohne Daten anzufassen. |

Geben Sie vor dem Befehl `--data-dir /absoluter/pfad` an, um den Datenordner zu wählen; `LOGLEM_DATA_DIR` leistet in der Automatisierung dasselbe. Befehle zum Verwalten von Quellen und Updates sind in [usage.md](usage.md) beschrieben.
