# Dati personali e modalità prova

I log contengono dati personali. Un indirizzo IP è un dato personale. Loglem ti dà strumenti per rispettare le regole. **Non** rende conforme un'installazione da solo: perché conservi i log, per quanto tempo e cosa dici alle persone restano decisioni tue.

## Tieni meno dati

- **Conservazione breve.** Imposta i giorni per sorgente. Le righe scadute si cancellano.
- **Maschera gli IP.** `203.0.113.77` diventa `203.0.113.0`. Subito, oppure dopo N giorni:

```sh
loglem privacy set site --mask-ips-after-days 0
loglem privacy set site --keep-fields request.uri,status
```

- **Tieni solo alcuni campi JSON.** Tutti gli altri vengono eliminati.
- **Vedi cosa conservi:** `loglem privacy report`. Mettilo nel tuo registro dei trattamenti.

Il mascheramento cambia solo la copia salvata. Il controllo prima di cancellare un file originale funziona lo stesso. In modalità prova il mascheramento è permesso solo «subito» (dopo cambierebbe righe già sigillate).

## Cancella i dati di una persona

Qualcuno ti chiede di cancellare i suoi dati. Usa la pagina **Dati personali**, oppure:

```sh
loglem erase --ip 203.0.113.7                                    # solo anteprima
loglem erase --ip 203.0.113.7 --apply --reason "richiesta art. 17 del 17/11/2026, pratica 3"
loglem erase --ip 203.0.113.7 --originals --apply --reason "…"   # anche dai file originali
```

Loglem toglie le righe da **tutte** le sorgenti. Il motivo finisce nel registro degli accessi. L'IP non viene mai scritto lì.

**File originali** (`--originals`, o la casella nella pagina). Loglem riscrive i file di log finiti senza quelle righe. Lo fa solo se:

- il file non è quello su cui un programma sta scrivendo,
- nessuno lo ha modificato da 24 ore,
- l'intero file è già nell'archivio.

Scrive un file nuovo accanto al vecchio, lo controlla, poi lo sostituisce. Se qualcosa non torna, l'originale resta. I file saltati sono elencati con il motivo. Ripeti il comando dopo la prossima rotazione. Anche i `.gz` vengono riscritti.

**Non** tocca:

- i log di Podman, Docker, Kubernetes e del journal. Non sono file di Loglem, e il journal non permette di togliere singole voci. Accorcia la loro durata, o ricrea il container.
- i backup già fatti (scadono con la loro rotazione),
- il file su cui un programma sta scrivendo, finché non viene ruotato.

In `--reason` scrivi il numero di una pratica, non un'email: anche l'email è un dato personale.

## Esporta i dati di una persona

Qualcuno ti chiede cosa hai su di lui:

```sh
loglem export --ip 203.0.113.7 --output /posto/sicuro/richiesta-3.txt   # --json per il JSON
```

Il file lo legge solo il proprietario. Cancellalo dopo averlo inviato.

## Chi ha guardato

`loglem audit` (o i file in `data/audit/`) elenca ogni accesso, ricerca, esportazione, cancellazione e modifica delle impostazioni, con utente e ora. I valori cercati **non** vengono scritti, solo il tipo di filtro («testo, ip»). Le voci si conservano 13 mesi.

## Disco e backup

- Loglem non cifra i suoi file. Usa un disco cifrato.
- `loglem backup` scrive file in chiaro. Cifrali prima che escano dal server (per esempio con restic).
- Non mettere copie di log con dati personali in posti da cui non puoi cancellare (come un bucket bloccato).

## Modalità prova

Per una sorgente che deve dimostrare che le sue righe non sono state cambiate:

```sh
loglem proof enable site --tsa https://freetsa.org/tsr     # --tsa è facoltativo
loglem proof verify site
```

Ogni giorno finito riceve un'impronta (SHA-256) delle sue righe. Dentro c'è anche l'impronta del giorno prima: cambiarne uno rompe tutti i successivi. Con `--tsa`, un'autorità esterna firma ogni impronta con l'ora; vede solo l'impronta.

`loglem proof verify` ricalcola ogni giorno e dice **integro**, **ALTERATO** o **scaduto**. Puoi controllare anche una copia: `loglem proof verify --archive copia.db --chain copia.chain`.

Cosa prova: un giorno sigillato non è cambiato dopo il sigillo (o dopo l'ora della marca). Non prova che le righe fossero vere quando sono state scritte.

Le righe scadono solo a giorni interi, e ogni scadenza è scritta nel registro.

**Cancellazione in modalità prova.** Vince la cancellazione. Loglem toglie le righe, **ricalcola** l'impronta di quel giorno e di tutti i successivi, e aggiunge una nota al registro: quando, perché, quante righe, impronta vecchia → nuova. `proof verify` mostra la nota. Le vecchie marche temporali restano valide per la versione vecchia. Per le impronte nuove se ne chiedono di nuove.
