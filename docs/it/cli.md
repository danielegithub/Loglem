# Riferimento dei comandi

Esegui `loglem --help` o `loglem COMANDO --help` per vedere le opzioni della versione installata. Senza comando Loglem apre il menu da tastiera nel terminale; `loglem tui` fa lo stesso in modo esplicito. I gruppi principali sono:

| Comando | A cosa serve |
|---|---|
| `init` | Crea una configurazione vuota. |
| `source list` | Elenca le sorgenti. Per aggiungerne una usa `source add-folder`, `add-file`, `add-podman` o `add-caddy` (log di Caddy con pulizia dei file ruotati). |
| `podman-list` | Trova container e pod; con `--connection` si sceglie una connessione con nome. |
| `status [--json]` | Mostra posizione degli archivi, dimensioni, date e stato della raccolta. |
| `collect` / `serve` | Legge una volta, oppure fa girare il servizio (raccolta più pagina web a indirizzo fisso) fino all'arresto. `web` fa lo stesso, o stampa l'indirizzo se il servizio è già attivo. |
| `search [opzioni]` | Cerca testo e intervalli di date negli archivi; `--json` dà un output leggibile da programmi. |
| `prune [--apply]` | Mostra le voci scadute, oppure con `--apply` le cancella dagli archivi. |
| `config export FILE` / `config import FILE` | Esporta o importa la configurazione delle sorgenti. |
| `config poll SECONDI` | Imposta ogni quanto leggere in automatico. |
| `config web-port PORTA` | Sceglie la porta locale fissa della pagina web (dal prossimo avvio). |
| `service install [--start]` | Registra un servizio utente systemd; `start`, `stop` e `status` lo gestiscono. |
| `diagnostics` | Stampa gli errori di Loglem salvati, con le tracce dello stack. |
| `self-check` | Controlla l'eseguibile e lo schema SQLite incorporato senza toccare i dati. |

Passa `--data-dir /percorso/assoluto` prima del comando per scegliere la cartella dati; `LOGLEM_DATA_DIR` fa lo stesso nelle automazioni. I comandi per gestire le sorgenti e gli aggiornamenti sono descritti in [usage.md](usage.md).
