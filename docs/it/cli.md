# Comandi

`loglem COMANDO --help` mostra tutte le opzioni. Senza comando, Loglem apre il menu nel terminale. Metti `--data-dir /percorso/assoluto` prima del comando per usare un'altra cartella dati (o imposta `LOGLEM_DATA_DIR`).

## Sorgenti e lettura

| Comando | Cosa fa |
|---|---|
| `site add example.com`, `site list`, `site show example.com` | Crea un sito, elenca i siti o mostra un sito con i suoi log. |
| `site copies example.com`, `site duplicates example.com [--external]` | Conta le posizioni degli originali e degli archivi. Confronta le righe recenti, anche nei file, nel journal e nei container locali con `--external`. |
| `source add-folder / add-file / add-podman / add-docker / add-kubernetes / add-journal` | Aggiunge una sorgente. |
| `source list`, `source set NOME --days N` | Elenca le sorgenti, cambia la durata. |
| `source configure NOME …` | Nome, cartella dei file finiti, pulizia dei ruotati, percorso. |
| `discover` | Cerca i log nella configurazione dei programmi conosciuti (`src/recipes/`). |
| `add`, `site rename / remove` | Aggiunta guidata (cosa, chi, dove, riepilogo); rinomina o elimina un sito. |
| `podman-list`, `docker-list`, `kubernetes-list` | Elenca container o pod. |
| `collect` | Legge una volta. |
| `serve` / `web` | Legge sempre e serve la pagina web. |
| `status [--json]` | Posizioni, dimensioni, date, stato, lacune. |
| `check [--minutes N]` | Codice di uscita 1 se Loglem è spento, una sorgente non viene letta o scatta un allarme. |

## Cerca e segui

| Comando | Cosa fa |
|---|---|
| `search` | `--text --ignore-case --level error --ip --path --status --from --to --source --limit --json` |
| `tail` | Stessi filtri, in diretta. `--server URL --user NOME` segue un Loglem remoto. |
| `stats SORGENTE [--days N] [--json]` | Richieste per giorno, pagine, codici, IP, pagine lente. |

## Cura dei dati

| Comando | Cosa fa |
|---|---|
| `alerts list / add-pattern / add-size / add-free-space / remove / command / seen / monitor / email-set / email-test / email-off / email-show` | Regole di allarme e notifica. |
| `backup DIR`, `backup-verify DIR`, `restore DIR [--replace]` | Copia, controlla e ripristina i dati. |
| `proof enable / disable / verify` | Modalità prova: sigilla ogni giorno. |
| `privacy set SORGENTE …`, `privacy report` | Maschera gli IP, tieni campi JSON, elenca cosa conservi. |
| `erase … [--originals]`, `export …`, `audit [--days N]` | GDPR: cancella (anche dai file originali) o esporta le righe di una persona, vedi chi ha guardato. Anche nella pagina **Dati personali**. |
| `prune [--apply]` | Mostra o cancella le righe scadute. |
| `archive-completed` | Archivia e controlla a mano un file finito. |

## Esegui e mantieni

| Comando | Cosa fa |
|---|---|
| `service install [--start] / uninstall / start / stop / status` | Servizio: systemd su Linux, launchd su macOS. |
| `access user-add / user-remove / user-list / address / proxy` | Altri computer: utenti, indirizzo, reverse proxy. |
| `config export / import / poll / web-port` | Impostazioni. |
| `update check / install / status / configure` | Aggiornamenti firmati (Linux). |
| `diagnostics`, `self-check`, `init` | Errori di Loglem, un test veloce, una configurazione vuota. |
