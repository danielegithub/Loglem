# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Un unico posto per i log del tuo server: dove sono, quanto spazio occupano e per quanto tempo restano.**

Loglem legge i tuoi log, li copia in un archivio in cui si può cercare e cancella le copie quando passa il tempo che hai scelto. Usi una pagina web o un menu nel terminale. Niente file di configurazione.

> **Stato: in fase di sviluppo (beta).** Funziona ed è provato su macOS. Su Linux è stato provato con journald, Podman, logrotate, Caddy e il servizio systemd. Non c'è ancora una versione pubblicata. Vedi i [limiti](docs/it/limitations.md).
>
> - **Prima di usarlo.** Si consiglia di provarlo prima su una macchina di prova e di fare un backup.
> - **Cancellazione dei file.** Attivarla è irreversibile. Essendo ancora in fase di test, si consiglia di provarne le funzionalità su una macchina di prova, o di valutare bene prima di attivare la cancellazione.
> - **Nessuna garanzia.** È software libero con licenza MIT, fornito «così com'è», senza garanzie di alcun tipo. Chi lo usa lo fa a proprio rischio: gli autori non rispondono di perdite di dati o di danni.
> - **Non è consulenza legale.** Gli strumenti per i dati personali aiutano, ma non rendono nessuno conforme alle norme e nascono sulla base di quanto diffuso in rete. Le scelte restano comunque in carico a chi lo usa.

## Indice

- [Perché Loglem](#perché-loglem)
- [Cosa legge](#cosa-legge)
- [Pulizia sicura dei file originali](#pulizia-sicura-dei-file-originali)
- [Per iniziare](#per-iniziare)
- [Installazione](#installazione)
- [Documentazione](#documentazione)
- [Licenza](#licenza)

## Perché Loglem

I log si accumulano in tanti posti. Nessuno ricorda dove sono, quanto sono grandi o quando cancellarli.

- **Vedi tutto.** Un elenco: ogni sorgente, dove si trova, la dimensione, lo stato.
- **Scegli per quanto.** 28 giorni, 6 mesi, 2 anni, per sorgente. Le copie vecchie spariscono da sole.
- **Puoi cercare.** Testo, date, IP, percorso, codice di risposta. Anche dopo che il file è stato ruotato o il container tolto. Le righe nuove compaiono in diretta.
- **Resta piccolo.** Un programma di circa 4 MB. Circa 5 MB di RAM (misurati su Linux). L'archivio occupa circa un decimo dei log originali. Nessun server di database, nessun account, nessun cloud. I log restano sulla tua macchina. [Misure](docs/it/performance.md).
- **È sicuro di default.** I file originali vengono solo letti. Cancellarli è facoltativo e controllato (vedi sotto).
- **È semplice.** Una pagina web, un menu nel terminale che funziona via SSH e comandi per gli script.
- **Ti dice cosa è andato storto.** Una sorgente ferma, una lacuna nei log, un disco pieno: lo vedi, e `loglem check` lo dice al tuo monitor.
- **Aiuta con i dati personali.** Maschera gli IP, cancella o esporta le righe di una persona, guarda chi ha consultato i log. Vedi [privacy](docs/it/privacy.md).

## Cosa legge

| Sorgente | Note |
|---|---|
| Cartelle e file di testo `.log` | Anche i file ruotati, anche `.gz` (lo storico si importa una volta, mai duplicato). |
| Server web, database, posta, SSH, cron… | Riconosciuti dalle loro ricette in `src/recipes/`: Nginx, Apache, Caddy, Traefik, HAProxy, PostgreSQL, MySQL e altri. Aggiungerne uno è un solo file. |
| Podman, Docker | Un container, oppure per nome tramite il journal. |
| Kubernetes | Un pod, oppure tutti i pod di un'applicazione. |
| Journal di Linux | Tutto, oppure alcuni servizi, programmi o gravità. |

## Pulizia sicura dei file originali

Spenta di default. Quando la accendi per una sorgente, Loglem cancella un file finito solo dopo questo controllo:

1. **Copia** il file nel suo archivio.
2. **Confronta la copia con l'originale, byte per byte.** Se una riga manca, è scaduta o è cambiata, si ferma.
3. Aspetta che nessuno modifichi il file da **24 ore**.
4. Solo allora cancella l'originale.

Il file su cui un programma sta ancora scrivendo non viene mai toccato. Se un passo fallisce, l'originale resta dov'è. Vale per i file ruotati di qualsiasi programma e per le cartelle di file finiti.

## Per iniziare

Dopo l'[installazione](#installazione):

```sh
loglem                                  # menu nel terminale
loglem discover                         # cerca i log nella configurazione dei programmi conosciuti
loglem source add-folder --name web --path /srv/example/logs --days 28
loglem collect
loglem search --source web --text '/login' --from 2026-11-01
loglem web                              # legge e apre la pagina web (stampa l'indirizzo)
```

Per continuare dopo aver chiuso il terminale (Linux: systemd, macOS: launchd):

```sh
loglem service install --start
```

Il menu e la pagina web sono in italiano. Altro in [Usare Loglem](docs/it/usage.md).

## Installazione

Quando sarà pubblicata una versione, scarica il pacchetto per la tua macchina dalla scheda **Releases**:

| Macchina | Fine del nome del pacchetto |
|---|---|
| Linux Intel/AMD 64 bit | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

I pacchetti richiedono glibc 2.39 o successiva (per esempio Ubuntu 24.04). Aggiornamenti, marche temporali e `loglem tail --server` usano il comando `curl` del sistema.

Controlla la firma con la chiave pubblica dell'editore (`release-public.pem`), ottenuta da un canale indipendente:

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

Tutto sta in `~/Loglem`: il programma e una cartella `data/`. L'installatore non avvia nessun servizio. Per compilarlo da solo vedi [CONTRIBUTING.md](CONTRIBUTING.md).

**In un container.** Costruiscilo con `podman build -t loglem -f Containerfile .` e avvialo montando una cartella di log. Vedi [Usare Loglem](docs/it/usage.md#11-in-un-container).

## Documentazione

[English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Pagina | Cosa c'è |
|---|---|
| [Usare Loglem](docs/it/usage.md) | Pochi passi per tutto |
| [Comandi](docs/it/cli.md) | Tutti i comandi |
| [Dati personali e modalità prova](docs/it/privacy.md) | Strumenti GDPR, cancellazione, esportazione, registro, giorni sigillati |
| [Se Loglem si ferma](docs/it/stopping.md) | Cosa perdi e cosa fa Loglem |
| [Limiti](docs/it/limitations.md) | Cosa non fa o non promette |

## Licenza

[MIT](LICENSE). Il testo completo dell'esclusione di garanzia è nel file LICENSE.
