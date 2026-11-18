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
