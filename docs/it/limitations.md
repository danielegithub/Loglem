# Limiti

Cosa Loglem non fa, o non promette.

## Piattaforme

- Linux e macOS. Il servizio è systemd (Linux) o launchd (macOS). Aggiornamenti firmati: solo Linux.
- Windows: il codice è scritto per essere portabile, ma non è stato compilato né provato.
- Non c'è ancora una versione pubblicata.
- In un container Loglem legge solo i file di log che monti. Podman, Docker, Kubernetes e il journal dell'host richiedono Loglem sull'host.

## Cosa legge

- Righe di testo UTF-8 fino a 1 MiB. Le altre codifiche non si leggono.
- I filtri per gravità, codice e percorso leggono il testo della riga. Conoscono Caddy (JSON), Nginx e Apache (combined) e il journal. Gli altri formati si trovano solo cercando il testo.
- Le statistiche funzionano con i log di Caddy in JSON e di Nginx/Apache in formato combined.
- Non ci sono ancora le espressioni regolari.

## Cosa si può perdere

- Podman, Docker e Kubernetes tengono i log solo finché esiste il container o il pod. Se viene tolto mentre Loglem è spento, quelle righe sono perse. Loglem mostra una **lacuna** sulla scheda.
- Per Kubernetes segui l'applicazione per etichetta (`app=web`). Un singolo pod sparisce a ogni rilascio.
- Se Loglem è spento, le righe non scadono e nulla viene ripulito. Vedi [stopping.md](stopping.md).
- Disco pieno: i file non ancora archiviati restano. Loglem non può liberare spazio.

## Sicurezza dei dati

- Gli archivi sono file SQLite su un disco locale, non su una cartella di rete. Ferma Loglem prima di copiarli a mano, oppure usa `loglem backup`.
- Cancellare una riga da Loglem non la cancella dai backup né dai file originali.
- Loglem non cifra i dati sul disco. Usa un disco cifrato e backup cifrati.
- Il collegamento alla pagina web non è cifrato. Usa una VPN o un reverse proxy con HTTPS.
- Loglem non può sapere se un programma ha finito con un file. La pulizia aspetta 24 ore e non tocca mai il file in uso.
- La modalità prova mostra che un giorno sigillato non è cambiato. Non prova che le righe fossero vere. Vedi [privacy.md](privacy.md).

## Cosa è stato provato

Provato su macOS con Podman, journald (in un container Linux), Caddy, Chrome e una vera autorità di marca temporale: aggiunta di sorgenti dalla pagina, ricerca, diretta, accesso con password, riavvio senza righe perse o duplicate, scadenza, rotazione di Caddy, reverse proxy, backup e ripristino, modalità prova, cancellazione.

Non provato: il servizio systemd su un vero Linux, un vero cluster Kubernetes (è stato usato un finto `kubectl`), l'agente launchd di macOS, Windows e un aggiornamento da una versione pubblicata.
