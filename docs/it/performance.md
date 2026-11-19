# Memoria, disco e tempi di risposta

[English](../performance.md) | [Italiano](performance.md)

Loglem conserva ogni sorgente in un archivio SQLite locale. La raccolta prepara le statistiche e lo stato degli allarmi una volta per ciclo, così più schede del browser possono consultare gli stessi risultati.

## Misure della release

Prova su Linux ARM64 con glibc, due CPU virtuali e una sola sorgente. Il generatore produce 200.000 accessi HTTP sintetici distribuiti su circa 28 giorni, con oltre 65.000 indirizzi IP e 512 percorsi. Il file di ingresso pesa 106,27 MB. I tempi includono la risposta HTTP. Lo spazio occupato comprende il database e i file ausiliari SQLite presenti durante la misura.

| Misura | Risultato |
|---|---:|
| Archivio su disco | 9,32 MB |
| Ricerca su tutto lo storico, senza corrispondenze | 239 ms |
| Statistiche degli ultimi 30 giorni | 1,47 ms |
| RSS un secondo dopo le statistiche | 4,57 MiB |
| PSS nello stesso istante | 3,60 MiB |
| Picco RSS del processo web durante la prova | 5,35 MiB |
| Memoria anonima dopo 10 risposte HTTP 400 | 440 KiB |
| Stato completo su 5 milioni di righe, 50 richieste | 0,83 ms di mediana, 1,23 ms al 95º percentile |

L’RSS conta tutte le pagine residenti del processo. Il PSS ripartisce il costo delle pagine condivise tra i processi che le usano. Entrambe le misure riguardano `loglem web`, avviato dopo l’importazione iniziale. Il rapporto di compressione dipende da quanto si ripete il testo dei log.

## Interventi completati

- [x] **Diagnostica.** Errori HTTP, errori di raccolta e panic continuano a essere registrati. La cattura del backtrace segue `RUST_BACKTRACE` e `RUST_LIB_BACKTRACE`. Il collector può riprendere dopo un panic.
- [x] **Memoria del processo.** Su Linux con glibc, Loglem limita a due le arene e imposta a 128 KiB la soglia di rilascio. Dopo una richiesta HTTP e alla fine del ciclo di raccolta, chiede all’allocatore di restituire le pagine libere.
- [x] **Statistiche.** Il collector aggiorna gli aggregati giornalieri durante la raccolta. Le finestre che iniziano a metà giornata includono solo le righe successive al limite. Conteggi di richieste, codici HTTP e totali giornalieri sono esatti. Le classifiche usano fino a 256 indirizzi e 256 percorsi per riepilogo, con percorsi limitati a 256 byte. Quando i contatori saturano, la pagina e il terminale indicano «Classifica approssimata». Cancellazione, mascheramento e scadenza invalidano i riepiloghi interessati. Il lettore JSON estrae solo i campi richiesti e la risposta HTTP serializza direttamente le statistiche.
- [x] **Stato e allarmi.** Il conteggio delle righe e la data più vecchia sono mantenuti insieme alle modifiche dell’archivio. Il collector aggiorna le regole sulle nuove righe e fa scadere le corrispondenze fuori finestra. La pagina legge lo stato salvato e durante il polling invia l’ultima generazione ricevuta. Se la generazione coincide, riceve soltanto lo stato della raccolta.
- [x] **Durata degli originali.** Il collector conserva per dieci minuti la descrizione della conservazione della sorgente. Una modifica alla configurazione invalida il dato. Aprire la scheda di una sorgente legge il risultato salvato.
- [x] **Journal condiviso.** Le sorgenti journal attive condividono una lettura dei messaggi per ciclo. Si parte dal punto più vecchio e ogni sorgente applica i propri filtri, la propria scadenza e il proprio cursore. Le verifiche di disponibilità del cursore e le sonde sulla durata degli originali sono operazioni distinte.
- [x] **Archivio a blocchi.** Lo schema 4 aggiorna i formati precedenti, compreso lo schema 3 che comprimeva le singole righe. La coda viene raccolta in blocchi gzip per file e generazione al raggiungimento di 1.000 righe o dopo cinque minuti. Per le righe lunghe Loglem chiude i blocchi prima. La ricerca li legge in streaming, dal più recente, e la deduplicazione usa gli intervalli di offset per i file e le chiavi di origine per gli altri ingressi. La scadenza elimina interi blocchi quando possibile. Per modifiche parziali Loglem estrae le righe interessate e ricomprime lo storico. I sigilli usano il testo originale decompresso in ordine di arrivo. I test coprono un sigillo creato nel formato precedente, la rilettura della stessa sorgente, la scadenza parziale, la cancellazione, il mascheramento, il backup e il ripristino.
- [x] **Chiavi di deduplicazione compatte.** Per journal, Podman e Kubernetes la chiave di ogni riga si salva come i primi 16 byte della sua impronta SHA-256, senza l’indice che la copiava una seconda volta. Gli archivi dello schema 4 si portano al nuovo formato (schema 5) alla prima apertura.
- [x] **Eseguibile.** La release usa un parser CLI interno e il client di sistema `curl` per aggiornamenti, marche temporali e `tail --server`. Queste tre funzioni richiedono `curl` nel percorso dei comandi. L’immagine container lo include. Gli aggiornamenti continuano a essere verificati con la firma Ed25519. La compilazione di SQLite esclude FTS3, FTS5, R-Tree, caricamento di estensioni e API deprecate, e mantiene `dbstat`.
- [x] **Controlli automatici.** La CI Linux su x86-64 e ARM64 esegue i due benchmark qui sotto. Verifica anche la correttezza delle risposte, i limiti di memoria dopo gli errori e le soglie per archivio, ricerca, statistiche e stato.

### Scelta della configurazione di release

Sul medesimo carico da 106,27 MB, passando dal profilo Rust `s` a `z` con glibc il binario ARM64 è passato da 3,95 a 3,62 MB e la ricerca completa da 188 a 222 ms. La release usa `z`. Nel confronto con il profilo `s`, mimalloc con `MIMALLOC_PURGE_DELAY=0` ha raggiunto 13,80 MiB di picco RSS e 6,96 MiB di memoria anonima dopo gli errori. La configurazione distribuita usa l’allocatore di sistema.

## Riprodurre le misure

Su Linux con Python 3 e una toolchain Rust compatibile con il progetto, dalla radice del repository eseguire i seguenti comandi.

```sh
cargo build --release --locked
python3 .github/scripts/memory.py target/release/loglem
python3 .github/scripts/status_latency.py target/release/loglem
```

Gli script creano directory temporanee e interrogano il servizio su loopback. Il secondo genera un archivio storico da cinque milioni di righe, ne attende la migrazione e misura 50 risposte complete di stato. La misura esclude la creazione e la migrazione iniziali.

## Prove su un sistema Linux reale

Prove eseguite il 19 novembre 2026 sul commit `a528000`, su un computer Linux vero e non in un container. Coprono le parti che finora erano state provate solo su macOS o in un container: journald, Podman, logrotate, Caddy e il servizio systemd.

| Componente | Versione |
|---|---|
| Sistema | Linux Mint 22.3 (base Ubuntu 24.04), kernel 7.0, x86_64, 16 CPU |
| Librerie | glibc 2.39, systemd 255 |
| Programmi | Podman 4.9.3 rootless (driver dei log: journald), logrotate 3.21.0, Caddy 2.11.9 in container |
| Journal della macchina | 586 MB in 100 file, circa 566.000 voci, 85 giorni |
| Loglem | release compilata con `cargo build --release --locked`, eseguibile da 4,76 MB |

Tutte le prove hanno usato cartelle di dati separate e container creati apposta. Il conteggio delle righe è stato confrontato con la fonte: `journalctl` per il journal, il numero progressivo scritto in ogni riga per file e container.

### Test automatici e benchmark della CI

| Prova | Risultato |
|---|---|
| `cargo test --release` | 93 test superati |
| `memory.py` (106,27 MB di accessi sintetici) | archivio 8,92 MB, ricerca senza risultati 105 ms, statistiche 2,0 ms, RSS finale 6,0 MiB, PSS 4,2 MiB, picco 6,8 MiB, memoria anonima dopo 10 errori 652 KiB |
| `status_latency.py` (5 milioni di righe) | 1,21 ms di mediana, 1,43 ms al 95º percentile su 50 risposte |

### Funzionamento

| Prova | Risultato |
|---|---|
| **journald**: tre sorgenti con filtri per programma e gravità, lettura condivisa | ✅ 1.400, 1.100 e 100 righe, uguali a `journalctl` |
| **journald**: `kill -9` durante l’arrivo di 6.000 righe, poi riavvio | ✅ 9.100 righe su 9.100, nessuna doppia. All’avvio Loglem segnala l’arresto irregolare |
| **journald**: importazione dell’intero journal (365 giorni) | ✅ 566.525 voci su 566.527 (le 2 mancanti sono arrivate dopo la lettura), in 91 s |
| **journald**: sorgente `--unit` con un servizio utente di systemd (anche nella lettura condivisa) | ✅ 201 righe su 200 scritte più l’avvio del servizio, 50 su 50 nella lettura condivisa |
| **Podman**: container con driver journald, letto per ID e per nome tramite il journal | ✅ 500 righe su 500 in entrambi i casi |
| **Podman**: container con driver `k8s-file` e `max-size` | ✅ 500 righe su 500 |
| **Podman**: container ricreato con lo stesso nome | ✅ La sorgente per nome continua (800 su 800). Quella per ID segnala una lacuna con un messaggio chiaro, e il ricollegamento recupera le 300 righe |
| **Podman**: container rimosso mentre Loglem è fermo | ✅ Come documentato, 200 righe perse e lacuna segnalata sulla sorgente |
| **logrotate**: `create`, con e senza `compress`, con `delaycompress`, con `dateext` (4 configurazioni, rotazioni ripetute anche senza letture in mezzo) | ✅ 2.150 righe su 2.150, nessuna doppia e nessuna mancante |
| **logrotate**: `copytruncate`, con e senza compressione, con `delaycompress` e `dateext` (4 configurazioni), anche con la copia letta prima del file attivo | ✅ 2.150 righe su 2.150, nessuna doppia e nessuna mancante |
| **Caddy** vero (`roll_size 1MiB`, `roll_keep 3`), 8.000 richieste in 33 s con `loglem serve` attivo | ✅ 8.000 richieste su 8.000, nessuna doppia, anche se Caddy ha ruotato e cancellato da solo i file più vecchi |
| **Pulizia dei ruotati** (`--remove-rotated true`) | ✅ I file con meno di 24 ore restano. Quelli più vecchi vengono cancellati dopo la verifica (3 `.gz`). Il file attivo resta |
| **Pulizia con un file ruotato illeggibile** | ✅ Non cancella niente, e la sorgente continua a leggere il file attivo (150 righe su 150). L’errore resta visibile sulla scheda |
| **Servizio systemd**: `service install --start` | ✅ Unità utente scritta, abilitata e avviata |
| **Servizio systemd**: `kill -9` del processo | ✅ Riavviato da systemd dopo 5,2 s, 3.000 righe su 3.000 |
| **Servizio systemd**: `systemctl --user restart` | ✅ Arresto pulito con l’ultima lettura, nuovo avvio in 0,2 s |
| **Servizio systemd**: `check` | ✅ Codice 0 a servizio attivo. Codice 1 con «Loglem non è in esecuzione» un minuto dopo l’arresto |
| **Servizio systemd**: pagina web servita | ✅ HTTP 200 |
| **Servizio systemd**: `service uninstall` | ✅ Unità rimossa, dati intatti |
| **Backup**: `backup`, `backup-verify`, `restore` | ✅ Copia in 0,07 s. Verifica: codice 0 se integro, codice 1 su un backup alterato. Ripristino con 8.021 righe su 8.021 |
| `self-check` | ✅ Eseguibile, schema SQLite e 17 ricette |

### Memoria e tempi

Il processo di Loglem è rimasto **tra 6 e 9 MB di RSS in tutte le prove**. I numeri più alti appartengono ai programmi esterni che Loglem lancia.

| Situazione | Loglem | Programmi esterni |
|---|---|---|
| Raccolta da tre sorgenti journal | 7,4 MB | `journalctl` 22–48 MB di RSS, di cui circa 1 MB di memoria anonima. Il resto sono i file del journal letti dalla cache del disco, condivisa e recuperabile dal kernel |
| Raccolta da sorgenti Podman | 6,7 MB | `podman` 68 MB durante la lettura |
| Importazione dell’intero journal (566.000 voci) | 8,7 MB di picco, 91 s | `journalctl` 147 MB di picco |
| `serve` durante il carico su Caddy | 7,4 MB di picco, 2,1 MB anonimi | — |
| `serve` per 60 s sull’archivio del journal, con lo stato chiesto ogni 2 s | 6,4 MB di RSS, 0,8 MB anonimi, PSS 4,5 MB | — |
| Servizio systemd (contabilità del cgroup) | `MemoryCurrent` 9,1 MiB, `MemoryPeak` 11,7 MiB | compresi i processi figli e la cache dei file scritti |

Ricerca sull’archivio dell’intero journal (566.000 voci, 100 risultati):

| Ricerca | Tempo |
|---|---:|
| Stato (`/api/status`), mediana di 30 richieste | 1,5 ms |
| Testo, su tutto lo storico (presente o assente) | 0,32 s |
| Testo con maiuscole indifferenti | 0,61 s |
| Solo errori | 1,21 s |
| Testo assente, ultimi 7 giorni | 0,04 s |

Spazio: l’archivio dell’intero journal occupa 49 MB, contro i 586 MB del journal. Gli archivi creati con la versione precedente (137,9 MB per le stesse 566.000 voci) si portano al nuovo formato alla prima apertura: 3,6 s, nessuna riga persa, 49 MB. Dopo la migrazione la deduplicazione resta attiva (nessuna riga doppia in una nuova raccolta) e la scadenza elimina chiavi e righe insieme (75.929 righe e 75.929 chiavi dopo una scadenza a 30 giorni, integrità SQLite `ok`).

