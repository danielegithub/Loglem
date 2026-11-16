# Limiti

Cosa Loglem non fa, o non garantisce, in questa versione.

**Cosa è supportato**

- Solo Linux. Windows non è supportato; su macOS non c'è un servizio automatico (si usa `loglem web`).
- Sorgenti: file di testo `.log` (UTF-8), container Podman e log di accesso di Caddy. I file compressi (`.gz`) non vengono letti. Docker, Kubernetes, Nginx e il journal di systemd non sono ancora supportati.
- Righe fino a 1 MiB.
- La ricerca è per testo esatto, con distinzione tra maiuscole e minuscole, e per intervallo di date. Non ci sono ancora filtri per gravità, indirizzo IP, percorso o codice di risposta.
- La pagina web si apre solo dalla macchina su cui gira Loglem (per un server remoto usa un tunnel SSH).

**Sicurezza dei dati**

- I log di Podman sono letti per istantanee. Se un container viene cancellato prima che Loglem lo legga, o se Loglem resta fermo a lungo, alcune righe possono andare perse.
- Le voci scadono solo mentre Loglem è in esecuzione (o quando esegui `loglem prune --apply`).
- Cancellare una voce dall'archivio di Loglem non la cancella dalle istantanee o dai backup del file system che l'hanno già copiata.
- Gli archivi sono file SQLite e devono stare su un disco locale, non su una cartella di rete. Non copiarli mentre Loglem è in esecuzione: fermalo prima.
- Se il disco è pieno, gli originali non ancora archiviati vengono conservati; Loglem non può liberare spazio né impedire ad altri programmi di scrivere.
- Loglem non può capire da solo che un programma ha finito con un file. La sua pulizia aspetta 24 ore e non tocca mai il file in scrittura, ma va attivata solo per cartelle e log di Caddy configurati come spiegato nella guida.
- Gli archivi non sono a prova di manomissione: Loglem non è un archivio che garantisce il valore probatorio dei log.

**Provato e non ancora provato**

- Provato con Podman vero e un vero browser: aggiunta di sorgenti dalla pagina, ricerca, riavvio senza righe perse o duplicate, scadenza e rotazione simulata in stile Caddy.
- Non ancora provati: il servizio systemd su un vero Linux, Caddy reale e il ritorno alla versione precedente dopo un aggiornamento fallito.
- Non c'è ancora una versione pubblicata.
