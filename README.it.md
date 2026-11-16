# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Un unico posto per i log del tuo server: dove sono, quanto spazio occupano e per quanto tempo vengono conservati.**

Loglem legge i log del tuo server Linux (file `.log` e container Podman), li copia in un archivio consultabile e cancella automaticamente le voci archiviate alla scadenza del periodo di conservazione che hai scelto. Gestisci tutto da una pagina web accessibile sul server stesso o da un menu nel terminale navigabile con la tastiera, senza modificare file di configurazione.

> **Stato: sperimentale.** Funziona ed è testato, ma non c'è ancora una versione pubblicata e alcune parti vanno ancora verificate fino in fondo su un server reale. Vedi i [limiti](docs/it/limitations.md).

## Indice

- [Perché Loglem](#perché-loglem)
- [Cosa fa](#cosa-fa)
- [Partenza rapida](#partenza-rapida)
- [Installazione](#installazione)
- [Documentazione](#documentazione)
- [Licenza](#licenza)

## Perché Loglem

I log si accumulano in tanti posti: una cartella per il server web, un'altra per l'applicazione, l'output di ogni container. Nessuno ricorda dove sono, quanto sono diventati grandi o quando andrebbero cancellati.

Con Loglem:

- **Sai sempre dove sono i log.** Un elenco mostra ogni sorgente, la posizione originale, la posizione dell'archivio, la dimensione e lo stato dell'ultima lettura.
- **Decidi per quanto tempo tenere i log di ciascuna sorgente.** 28 giorni, 6 mesi o 2 anni, sorgente per sorgente. Le voci scadute vengono tolte al posto tuo: il disco non si riempie e i dati vecchi non restano lì per caso.
- **Puoi cercare.** Testo e intervalli di date su tutto ciò che è stato raccolto, anche dopo che il file originale è stato ruotato o il container rimosso.
- **Non dà fastidio.** Un solo eseguibile di circa 4 MB, circa 10 MB di RAM a riposo, nessun server di database, nessun account, nessun servizio cloud. I log non lasciano mai la macchina.
- **È sicuro di default.** I file originali vengono solo letti, mai modificati. Cancellare un originale è facoltativo e vale solo per i file che consegni esplicitamente come conclusi.
- **È semplice da usare.** Tutto da una pagina che risponde solo sul server stesso, o da un menu nel terminale che funziona anche via SSH. Ci sono anche i comandi per script e automazioni.
- **Sostituisce un mucchio di script.** Raccogliere, pulire, conservare e cercare nei log di solito richiede un misto di regole di rotazione, attività pianificate e comandi improvvisati. Loglem fa tutto in un posto solo, con le stesse impostazioni nella pagina web, nel terminale e nella riga di comando.
- **Ti segue ovunque.** Percorsi, container e durate sono impostazioni, non codice: esporti la configurazione e la importi su un'altra macchina.

## Cosa fa

1. **Aggiungi una sorgente:** una cartella di file `.log`, un singolo file, un container Podman o il log di accesso di Caddy.
2. **Loglem legge le righe nuove** ogni 10 secondi (valore predefinito) e le salva in un piccolo archivio SQLite di quella sorgente. Ricorda dove si è fermato, quindi nulla viene letto due volte.
3. **Cerchi e controlli** dalla pagina web o dal terminale, e vedi lo stato di ogni sorgente.
4. **Le voci vecchie vengono tolte** quando superano la durata che hai impostato per quella sorgente.
5. **Se qualcosa va storto,** gli errori di Loglem vengono salvati con la loro causa e si vedono nella pagina Diagnostica. Una sorgente che fallisce non ferma le altre.

Gira come servizio in background (servizio utente systemd) o quando serve. Funziona su Linux, senza desktop grafico.

## Partenza rapida

Dopo l'[installazione](#installazione):

```sh
# Apre il menu guidato nel terminale (frecce, Invio, Esc)
loglem

# Oppure aggiungi una cartella di log da riga di comando e leggila una volta
loglem source add-folder --name web-logs --path /srv/example/logs --days 28
loglem collect
loglem status

# Cerca
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16

# Apre la pagina web (stampa il suo indirizzo, per esempio http://127.0.0.1:53142)
loglem web
```

Per continuare a leggere anche dopo aver chiuso il terminale:

```sh
loglem service install --start
```

Menu e pagina web sono già in italiano. I dettagli sono in [Usare Loglem](docs/it/usage.md).

## Installazione

Quando sarà pubblicata una versione, scarica dalla scheda **Releases** di questo repository il pacchetto per la tua macchina:

| Macchina | Suffisso del pacchetto |
|---|---|
| Linux Intel/AMD 64 bit | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

I pacchetti richiedono glibc 2.39 o successiva (per esempio Ubuntu 24.04).

Prima di installare, controlla la firma con la chiave pubblica di chi pubblica la versione (`release-public.pem`), ottenuta da un canale indipendente:

```sh
openssl pkeyutl -verify -pubin -inkey release-public.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
sha256sum --check --ignore-missing SHA256SUMS
```

Poi installa:

```sh
tar xzf loglem-*.tar.gz
cd loglem-*
sh install.sh
~/Loglem/loglem
```

Tutto sta in `~/Loglem`: il programma e una cartella `data/` con impostazioni, archivi e diagnostica. L'installazione non attiva nessun servizio da sola. Per le sorgenti Podman, Podman deve essere installato e accessibile allo stesso utente che esegue Loglem.

Per compilarlo dal codice sorgente, vedi [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentazione

La documentazione è disponibile in: [English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Pagina | Cosa contiene |
|---|---|
| [Usare Loglem](docs/it/usage.md) | Guida passo per passo: sorgenti, ricerca, conservazione, servizio, pulizia, passaggio a un'altra macchina, problemi, aggiornamenti |
| [Riferimento dei comandi](docs/it/cli.md) | Tutti i comandi |
| [Limiti](docs/it/limitations.md) | Cosa non fa o non garantisce |

## Licenza

[MIT](LICENSE).
