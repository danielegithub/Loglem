# Loglem: ottimizzazioni di RAM e disco (misurate)

Obiettivo del progetto: restare tra 10 e 15 MB di RAM su un VPS piccolo, con poco disco.
Ogni intervento qui sotto parte da una misura fatta sulla release attuale (`opt-level = "s"`,
`lto = "thin"`, `strip = true`). Gli interventi sono in ordine di guadagno diviso fatica.

## Misure di partenza

Banco di prova: copia di `data/` + un access log di Caddy in JSON da 200.000 righe (106 MB),
server web avviato con `loglem web`, chiamate API con curl. RAM letta da `/proc/PID/status` e
`/proc/PID/smaps_rollup`.

| Situazione | RSS | Anon (heap proprio) | Note |
|---|---|---|---|
| A riposo | 7,6 MB | 1,26 MB | PSS 5,4 MB. Il binario occupa 4,1 MB di pagine di file, libc 1,6 MB |
| Dopo **un qualsiasi errore** registrato (anche un 400) | **42 MB** | **31 MB** | Non scende più. Vedi intervento 1 |
| Ricerca (500 righe) | 8 MB | 1,8 MB | < 0,2 s anche senza risultati: va bene |
| Statistiche 30 giorni | picco **30 MB** | poi resta a 17–23 MB | Vedi interventi 2 e 3 |
| Stesso, con `MALLOC_ARENA_MAX=1` e trim di glibc | picco 30 MB | poi **0,9 MB** (RSS 7,5) | Conferma che il problema è l'allocatore |
| `loglem collect` delle 200.000 righe | picco 23 MB | — | 1,9 s |
| Ogni `journalctl` lanciato dalla raccolta | ~21 MB per processo | — | 1 processo per sorgente journal ogni 5 s |
| Disco: 106 MB di log | archivio **168 MB** (1,6×) | — | deflate a blocchi da 1000 righe: **8,5 MB** |

Ripartizione del disco dell'archivio (dbstat): tabella `entries` 137 MB per 106 MB di testo, indice
`record_key` UNIQUE 12,7 MB, `entries_file` 10,9 MB, `entries_time` 3,9 MB, `entries_collected` 3,9 MB.

Ripartizione del codice nel binario (simboli, approssimata): SQLite ~1,1 MB, serde_json ~0,45 MB,
codice di Loglem ~0,45 MB, clap ~0,4 MB, stack TLS (rustls + ring + webpki + ureq + url) ~0,7 MB,
gimli/addr2line/demangle ~0,1 MB.

## Interventi

### 1. Togliere la simbolizzazione del backtrace a ogni errore (banale, −30 MB)

`src/diagnostics.rs:53` chiama `Backtrace::force_capture().to_string()` per ogni errore registrato
(una richiesta API che risponde 400, una sorgente che non si legge, un panic…). La libreria standard carica e tiene in
cache per sempre le informazioni di debug, quelle di libc comprese se è installato `libc6-dbg`.
Nella release il risultato è inutile: tutte le righe di Loglem compaiono come `<unknown>`, perché il
binario è senza simboli.

- Usare `Backtrace::capture()`, che rispetta `RUST_BACKTRACE` / `RUST_LIB_BACKTRACE`, oppure non
  catturare affatto nella release (`cfg!(debug_assertions)`).
- Facoltativo: salvare solo gli indirizzi grezzi meno l'indirizzo di caricamento e il build-id, da
  tradurre offline con il binario non strippato pubblicato accanto alla release.
- Controllare anche il panic hook (`diagnostics::init`), che passa da `error()`.

**Fatto quando:** dopo 10 richieste che rispondono 400, `RssAnon` resta sotto i 2 MB.

### 2. Far restituire la memoria all'allocatore (piccolo, −10/−15 MB dopo i picchi)

glibc tiene un'arena per ogni thread (ogni connessione HTTP è un thread, fino a 16) e non
restituisce la memoria liberata. Misurato: dopo le statistiche, 22,9 MB senza interventi e 7,5 MB
con `MALLOC_ARENA_MAX=1` + `glibc.malloc.trim_threshold=131072`.

- All'avvio, solo con `target_env = "gnu"`: `libc::mallopt(M_ARENA_MAX, 2)` e
  `libc::mallopt(M_TRIM_THRESHOLD, 128 * 1024)`.
- Dopo le operazioni pesanti (statistiche, export, ricerca, fine del ciclo di raccolta):
  `libc::malloc_trim(0)`.
- Misurare anche la feature esistente `allocator-mimalloc` (con `MIMALLOC_PURGE_DELAY=0`) e
  tenerla solo se è migliore. Non cambiarla alla cieca.

**Fatto quando:** dopo le statistiche a 30 giorni, l'RSS torna sotto i 9 MB entro 1 secondo.

### 3. Statistiche in streaming, con memoria limitata (medio, picco da 30 a meno di 12 MB)

`src/stats.rs`: ogni riga viene analizzata in un `serde_json::Value` completo (`parse_caddy`) e
IP e percorsi diventano `String` in `HashMap` senza limite. La memoria cresce con il numero di IP
diversi, cioè con i bot.

- Analisi con una struct presa in prestito: `#[derive(Deserialize)] struct Line<'a>` con
  `#[serde(borrow)] uri: &'a str`, `client_ip: Option<&'a str>`, `status: u16`,
  `duration: Option<f64>`. I campi sconosciuti vanno ignorati, senza costruire l'albero `Value`.
- Chiavi compatte: `IpAddr` (o `u32` per IPv4) invece di `String`. Percorsi troncati a 256 byte.
- Contatori limitati: al massimo N chiavi distinte (per esempio 50.000), poi si passa a
  Space-Saving / Misra-Gries e la pagina indica «classifica approssimata».
- Passo successivo, che è quello che conta davvero: calcolare gli aggregati giornalieri **durante
  la raccolta** (tabella `daily_stats`). Così la pagina legge 30 righe invece di 30 giorni di log.
  Attenzione al GDPR: gli aggregati con gli IP vanno ricalcolati quando si cancella o si maschera
  un IP (`erase.rs`, `privacy.rs`) e devono scadere con la conservazione.

**Fatto quando:** statistiche a 30 giorni su 200.000 righe con picco sotto i 12 MB e meno di 0,3 s.

### 4. `/api/status` non deve contare tutto ogni 2 secondi (piccolo/medio, CPU e I/O)

La pagina chiama `/api/status` ogni 2 s (`web/app.js:6`, `REFRESH_MS`), per ogni scheda del
browser aperta. Ogni chiamata apre tutti gli archivi, fa `SELECT count(*), min(event_ns)`
(`src/archive.rs:270`) e `alerts::evaluate`, che per ogni regola fa `count_since` con `instr` sul
testo (`src/archive.rs:308`). Con milioni di righe diventa un lavoro continuo.

- Tenere conteggio e prima data nella tabella `state`, aggiornati da inserimento, scadenza e
  cancellazione: lettura O(1).
- Valutare gli allarmi nel collector, solo sulle righe nuove (contatori incrementali), e salvarne
  l'esito. `/api/status` legge soltanto.
- Lato pagina, ridisegnare solo quando cambia `Progress.generation`, che esiste già, oppure
  usare il long-poll già presente per la diretta.

**Fatto quando:** `/api/status` con 5 milioni di righe risponde in meno di 10 ms.

### 5. Le sonde sulla durata degli originali vanno in cache sul server (piccolo)

Il nuovo `/api/original-retention` (`src/sources/retention.rs`) lancia `systemd-analyze`, e per i
container `podman inspect` e `podman container exists`, a ogni richiesta. La pagina la ripete per
ogni scheda, ogni 60 s, per ogni scheda del browser aperta. `podman` è un programma Go pesante (da
misurare sul VPS: qui non c'erano container attivi).

- Calcolare la descrizione nel collector (o al massimo ogni 10 minuti) e tenerla in `state`. La
  rotta HTTP legge soltanto.
- Invalidare la cache quando cambia la configurazione della sorgente.

### 6. Una sola lettura del journal per ciclo (medio, −21 MB per ogni sorgente oltre la prima)

`src/sources/journal.rs:212`: ogni sorgente journal lancia il suo `journalctl` ogni 5 s. Le 3
sorgenti container dell'utente sono tutte journal filtrate per `CONTAINER_NAME`.

- Un solo `journalctl --output=json` per ciclo, con le condizioni unite con `+` (OR),
  smistando ogni voce alla sorgente giusta in base ai campi. Ogni sorgente mantiene il suo punto di
  ripartenza: si parte dal `SINCE` più vecchio e si scartano le voci già lette (la chiave unica lo
  fa già).
- Da valutare dopo, e solo misurando: leggere tramite `libsystemd` (`sd_journal_*` con `dlopen`)
  invece di lanciare un processo. Leggere direttamente i file del journal in Rust **non** è
  consigliato: richiederebbe xz/lz4/zstd, e il formato cambia tra versioni di systemd.

### 7. Archivio compresso a blocchi (grande, disco da 168 MB a ~10–15 MB)

È il guadagno più grande per un VPS con pochi GB, e riduce anche il lavoro del page cache.

- Nuovo schema (`user_version = 3`, con migrazione): una tabella di coda non compressa per le
  righe recenti e una tabella `blocks(id, file_ref, first_ns, last_ns, first_offset, last_offset,
  lines, data BLOB)`. Il blocco si chiude ogni 1000 righe o ogni 5 minuti. Compressione deflate con
  `flate2`, che è già tra le dipendenze: nessun crate nuovo.
- Ricerca: si scorrono i blocchi per `last_ns` decrescente e si decomprime in streaming. Il filtro
  si applica in Rust con `Query::matches`, che esiste già ed è allineato a `Query::sql()`.
- Scadenza: si cancellano i blocchi interi. Se un blocco è a cavallo della data limite, si riscrive.
- Cancellazione e mascheramento GDPR: si riscrivono solo i blocchi coinvolti. `secure_delete=ON`
  resta.
- Modalità prova: l'impronta del giorno va calcolata sul testo decompresso, nello stesso ordine
  di oggi, così i sigilli esistenti restano verificabili. Va testato esplicitamente.
- Deduplicazione: oggi `record_key` (testo, indice UNIQUE da 12,7 MB) e `file_id` (testo ripetuto
  su ogni riga) occupano più di 25 MB. Basta un intervallo di offset per blocco più una tabella
  `files` con un id intero.
- Passo intermedio, più economico, se il punto 7 completo è troppo: tabella `files` con id intero
  al posto di `file_id` testuale, e unicità su `(file_ref, generation, byte_offset)` invece che su
  `record_key`. Prima va verificato come sono costruite le chiavi per journal, Podman e Kubernetes.
  Risparmio stimato: 20–25% del disco.

**Fatto quando:** 106 MB di log Caddy occupano meno di 20 MB, la ricerca senza risultati su tutto
l'archivio resta sotto 1 s, e i test di scadenza, cancellazione, prova, backup e ripristino passano.

### 8. Binario più piccolo (medio/grande, −1,5/−2 MB di RSS da file)

Le pagine di codice sono pulite e il kernel le può liberare, però compaiono in `top` e `ps`.

- **Stack TLS (~0,7 MB):** sostituire `ureq` + `rustls` + `ring` + `webpki-roots` lanciando `curl`
  per il controllo degli aggiornamenti, lo scaricamento, la marca temporale (TSA) e
  `tail --server`. L'integrità degli aggiornamenti la garantisce già la firma ed25519, non TLS. Se
  `curl` manca serve un messaggio chiaro.
- **clap (~0,4 MB):** sostituirlo con un parser scritto a mano o con `lexopt`. È una riscrittura
  ampia di `src/cli.rs` (1.483 righe) e dei testi di aiuto: va fatta per ultima.
- **serde_json (~0,45 MB):** meno istanze generiche. Serializzare direttamente con `to_vec`
  invece di passare da `to_value` (per esempio `/api/stats`), e meno `json!` con tipi diversi.
- **SQLite (~1,1 MB):** compilarlo senza le funzioni che non si usano tramite `LIBSQLITE3_FLAGS`
  (per esempio FTS3/FTS5, RTREE, load_extension, le API deprecate). Prima va controllato cosa
  abilita `libsqlite3-sys` con `bundled` e cosa serve davvero (dbstat è utile per la diagnosi).
- Provare `opt-level = "z"` e misurare la velocità della ricerca.
- **Non** usare `panic = "abort"`: `src/runtime.rs` usa `catch_unwind` per sopravvivere a una
  raccolta andata in panico.

### 9. Un controllo automatico della RAM nei test (piccolo, impedisce di peggiorare)

- Script di CI: avvia `loglem web` su dati di prova (200.000 righe generate), esegue una sequenza
  fissa di chiamate (status, ricerche, una richiesta in errore, statistiche) e fallisce se `VmHWM`
  supera una soglia o se `RssAnon` a fine giro supera 3 MB.
- Nel README riportare il PSS (`smaps_rollup`) oltre all'RSS: è il numero corretto.

## Ordine consigliato

1 → 2 → 9 (subito, poche righe, il grosso del guadagno in RAM) → 4 → 5 → 3 → 6 → 7 → 8.

Dopo 1 e 2 Loglem dovrebbe restare stabilmente sotto i 10 MB anche dopo errori e statistiche. Il
punto 7 è quello che risolve il problema del disco.
