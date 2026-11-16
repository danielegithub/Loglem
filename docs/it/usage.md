# Usare Loglem

Una guida passo per passo. Per l'elenco dei comandi vedi [cli.md](cli.md); per quello che Loglem non fa vedi [limitations.md](limitations.md).

## Indice

- [1. Aggiungi la tua prima sorgente](#1-aggiungi-la-tua-prima-sorgente)
- [2. Cerca nei log](#2-cerca-nei-log)
- [3. Scegli per quanto tempo tenere i log](#3-scegli-per-quanto-tempo-tenere-i-log)
- [4. Tieni Loglem sempre acceso](#4-tieni-loglem-sempre-acceso)
- [5. Fai pulire a Loglem i file originali](#5-fai-pulire-a-loglem-i-file-originali)
- [6. Cambia, metti in pausa o togli una sorgente](#6-cambia-metti-in-pausa-o-togli-una-sorgente)
- [7. Dove sono i dati e come spostarli](#7-dove-sono-i-dati-e-come-spostarli)
- [8. Quando qualcosa non va](#8-quando-qualcosa-non-va)
- [9. Aggiorna Loglem](#9-aggiorna-loglem)

## 1. Aggiungi la tua prima sorgente

Una **sorgente** è il posto da cui arrivano i log: una cartella di file `.log`, un singolo file, un container Podman o il log di accesso di Caddy. Puoi aggiungerla dalla pagina web, dal menu nel terminale o dalla riga di comando.

### Dalla pagina web

```sh
loglem web
```

Loglem stampa l'indirizzo della pagina (per esempio `http://127.0.0.1:53142`) e la cartella in cui tiene i dati. Apri quell'indirizzo in un browser. L'indirizzo resta lo stesso dopo un riavvio. Se la porta è occupata da un altro programma, Loglem si ferma e lo dice; scegline un'altra con `loglem config web-port 8790`. Loglem non usa mai la porta 80 o 443.

La pagina funziona solo sulla macchina dove gira Loglem. Su un server remoto aprila con un tunnel SSH (`ssh -L 53142:127.0.0.1:53142 utente@server`, poi vai su `http://127.0.0.1:53142`).

Il menu a sinistra ha quattro pagine:

- **Sorgenti:** le tue sorgenti, con stato, numero di righe e collegamenti ai log e alle impostazioni.
- **Esplora i log:** cerca e leggi i log di una sorgente. L'elenco si aggiorna da solo; una casella lo mette in pausa.
- **Aggiungi sorgente:** un modulo guidato. Scegli il tipo, la cartella o il container, un nome, che tipo di log è e per quanto tenerlo, poi premi **Salva e apri i log**. Loglem legge subito i log.
- **Problemi:** gli errori di Loglem, con la loro causa (vedi la [sezione 8](#8-quando-qualcosa-non-va)).

### Dalla riga di comando

```sh
# una cartella di file .log (i file nuovi vengono presi in automatico)
loglem source add-folder --name web-logs --path /srv/example/logs --days 28

# un singolo file; legge anche le sue copie rinominate, come api.log-1
loglem source add-file --name api --path /srv/example/logs/api.log \
  --rotated-pattern 'api.log-*' --days 14

# un container Podman
loglem podman-list
loglem source add-podman --name app --container my-container --days 14

# leggi adesso e controlla
loglem collect
loglem status
```

Note:

- Una sorgente cartella legge i file `.log` che stanno direttamente nella cartella; sottocartelle, file compressi e altre estensioni sono ignorati.
- Un file deve avere un nome che finisce in `.log`. I file compressi (`.gz`) non vengono letti.
- Per Podman, Loglem ricorda l'ID completo del container. Se ricrei un container con lo stesso nome, aggiungilo come sorgente nuova (oppure vedi la [sezione 6](#6-cambia-metti-in-pausa-o-togli-una-sorgente)). Lo storico già raccolto resta disponibile anche dopo la rimozione del container. Con `--connection NOME` scegli una connessione Podman con nome.
- Loglem legge soltanto i tuoi file originali. Non li modifica, a meno che tu non attivi la pulizia (sezione 5).

### Dal menu nel terminale

Esegui `loglem`. Usa le frecce (o `j`/`k`), Invio per scegliere e Esc per tornare indietro.

```text
> Le mie sorgenti — posizione, spazio e log salvati
  Aggiungi cartella, file o container
  Cerca nei log
  Conservazione — per quanti giorni tenere i log
  Leggi i log adesso
  Raccolta automatica (servizio Linux)
  Impostazioni
  Aggiornamenti
  Esci
```

Scegli **Aggiungi cartella, file o container**, poi il tipo, poi sfoglia fino alla cartella (oppure scegli **Scrivi / incolla un percorso…** per digitarla). Controlla il riepilogo e salva con **Salva e leggi i log adesso**. Esc annulla prima del salvataggio.

## 2. Cerca nei log

Nella pagina web apri **Esplora i log**, scegli la sorgente e scrivi il testo. Puoi limitare per data (UTC). Le righe più recenti vengono prima.

Dalla riga di comando:

```sh
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16
```

- La ricerca è per testo esatto e distingue maiuscole e minuscole; non è un'espressione regolare.
- `--to 2026-11-16` include tutto quel giorno (UTC).
- Con `--json` l'output è leggibile da altri programmi; `--limit` arriva fino a 1000 risultati.
- Se una riga non ha una data che Loglem riconosce, resta comunque cercabile e viene mostrata con l'ora in cui è stata letta. Loglem riconosce i comuni log JSON (`ts`, `timestamp`, `time`, `@timestamp`).

## 3. Scegli per quanto tempo tenere i log

Ogni sorgente ha il proprio periodo di conservazione: un numero di giorni, mesi o anni. Quando una voce supera questo periodo, Loglem la cancella dal suo archivio.

```sh
loglem source set web-logs --days 7
loglem source configure web-logs --amount 6 --unit months --log-kind access
loglem source configure web-logs --amount 2 --unit years
```

- Nella pagina web e nel menu del terminale (**Conservazione**) la stessa impostazione è un modulo che mostra anche quando scadranno le voci.
- Le voci scadute vengono tolte mentre Loglem è in esecuzione (vedi sezione 4). `loglem prune` mostra cosa verrebbe cancellato; `loglem prune --apply` lo cancella subito.
- La conservazione parte dalla data della riga o da quando Loglem l'ha letta per la prima volta, la più antica delle due: una riga con data nel futuro non resta per sempre.
- Mettere in pausa una sorgente ferma le nuove letture, non la conservazione.
- Il tipo di log (accessi, generico, errori) descrive soltanto la sorgente; non stabilisce per te una durata di legge.
- La durata vale per la copia di Loglem. I tuoi file originali non vengono cancellati, a meno che tu non attivi la pulizia (sezione 5).

## 4. Tieni Loglem sempre acceso

`loglem web` raccoglie i log solo finché è in esecuzione. Per raccoglierli sempre, anche dopo aver chiuso il terminale o riavviato, installalo come servizio:

```sh
loglem service install --start
loglem service status
```

Crea un **servizio utente systemd** per l'utente corrente. Legge le righe nuove ogni 10 secondi (cambia con `loglem config poll 5`), toglie le voci scadute e serve la pagina web allo stesso indirizzo.

- Su un server, chiedi a un amministratore di eseguire una volta `sudo loginctl enable-linger UTENTE`, così il servizio gira anche quando l'utente non è collegato.
- Il servizio ha solo i permessi di quell'utente. Controlla che l'utente possa leggere le cartelle dei log; Loglem non gira come root.
- Per ogni cartella dati gira un solo Loglem. Se il servizio è già attivo, `loglem web` non ne avvia un secondo: stampa l'indirizzo del servizio ed esce.
- Avviare Loglem con un altro `--data-dir` mostra sorgenti diverse. La cartella dati in uso è sempre indicata nella pagina web e nel menu del terminale.

## 5. Fai pulire a Loglem i file originali

Per impostazione predefinita, Loglem non cancella i file originali. Puoi lasciargli cancellare i file originali **dopo che li ha copiati e verificati**. Lo fa solo per i file che nessuno ha modificato da almeno 24 ore, mai per il file in cui un programma sta scrivendo, e conserva ogni file che non riesce a verificare (la scheda della sorgente mostra un avviso). La scadenza dell'archivio è un passaggio separato, impostato nella sezione 3.

### Caddy

In **Aggiungi sorgente** scegli **Il log di accesso di Caddy**, apri la cartella dei log (spesso `/var/log/caddy`) e scegli il file in cui scrive Caddy, di solito `access.log`. Loglem legge allora anche i file che Caddy crea quando fa ruotare il log (per esempio `access-2026-11-16T10-00-00.000.log`) e li cancella dopo la verifica. Alla fine la pagina mostra il blocco da mettere nel Caddyfile:

```caddyfile
log {
	output file /var/log/caddy/access.log {
		roll_size 50MiB
		roll_uncompressed
		roll_keep 100
		roll_keep_for 60d
	}
}
```

- `roll_uncompressed` è obbligatorio: Loglem verifica solo file non compressi. Se trova file `.gz` li segnala sulla scheda.
- `roll_keep_for 60d` è una rete di sicurezza: se Loglem resta spento a lungo, Caddy cancella comunque i file più vecchi.
- Loglem non modifica il tuo Caddyfile: il blocco lo incolli tu.

Dalla riga di comando: `loglem source add-caddy --name site --path /var/log/caddy/access.log --days 90`. Per una sorgente file che ha già un modello di rotazione: `loglem source configure site --remove-rotated true`.

### Altri programmi

Chiedi al programma di spostare ogni file finito, chiuso e non compresso, in una cartella dedicata. Nelle impostazioni della sorgente attiva **Archivia ed elimina i file di una cartella dedicata** e conferma. Non puntarlo mai a una cartella in cui un programma sta ancora scrivendo.

```sh
loglem source configure web --completed-dir /percorso/assoluto/file-conclusi --handoff-confirmed
# per disattivare
loglem source configure web --completed-dir ""
```

Per un singolo file finito, senza aspettare 24 ore:

```sh
loglem archive-completed --source web --file /srv/example/logs/access-old.log --producer-closed --remove
```

Senza `--remove` il comando archivia e verifica il file, lasciando intatto l'originale. Loglem non cancella mai gli originali di un container Podman.

## 6. Cambia, metti in pausa o togli una sorgente

Nella pagina web premi **Impostazioni** sulla scheda della sorgente. Dalla riga di comando:

```sh
loglem source rename web "Sito pubblico"
loglem source pause web          # ferma la lettura, lo storico resta cercabile
loglem source resume web
loglem source detach web         # scollega; l'archivio resta cercabile
loglem source configure web --container NUOVO_CONTAINER   # punta a un container sostitutivo
loglem source configure web --path /nuovo/percorso        # nuovo file o cartella
```

- Rinominare cambia solo l'etichetta; lo storico resta.
- Per tenere l'archivio di una sorgente in un'altra cartella: `loglem source configure web --archive-dir /cartella/assoluta`. Loglem lo copia, controlla la copia e poi toglie i vecchi file; non sovrascrive mai un database esistente. Con `--archive-dir ""` torni alla cartella predefinita. Prima chiudi gli altri programmi che hanno aperto l'archivio.

## 7. Dove sono i dati e come spostarli

Di default tutto sta nella cartella di Loglem:

```text
Loglem/
├── loglem          il programma
└── data/
    ├── config.json   le tue sorgenti e impostazioni
    ├── archives/     i log salvati (un database per sorgente)
    ├── diagnostics/  il registro degli errori di Loglem
    └── updates/
```

Per usare un altro percorso passa `--data-dir /percorso/assoluto` prima del comando, oppure imposta `LOGLEM_DATA_DIR`.

Per passare a un'altra macchina:

```sh
loglem config export ./loglem-config.json     # sulla macchina vecchia
loglem config import ./loglem-config.json     # sulla macchina nuova
```

Così sposti le **impostazioni** (sorgenti, durate), non i log. Sistema percorsi e permessi sulla nuova macchina. Per spostare anche i log, ferma Loglem e copia l'intera cartella `data/`. Importare una configurazione non toglie mai la regola di conservazione di una sorgente che ha già un archivio: in quel caso metti la sorgente in pausa.

## 8. Quando qualcosa non va

- **Una sorgente mostra un errore:** le altre continuano a funzionare e le righe già salvate restano leggibili. Apri la scheda della sorgente per il messaggio, oppure esegui `loglem status`.
- **La pagina è vuota o una sorgente dice «non si può leggere»:** controlla che l'utente che esegue Loglem possa leggere la cartella o il file.
- **Vedere gli errori di Loglem:** **Problemi** nella pagina web, oppure `loglem diagnostics`. Ogni voce ha l'ora, cosa stava facendo Loglem, la causa e i dettagli tecnici. Il file è `data/diagnostics/loglem.log` (circa 5 MB, poi ruota; leggibile solo dal proprietario).
- **Controllare l'installazione:** `loglem self-check`.
- **Porta già in uso:** `loglem config web-port 8790`, poi riavvia.

## 9. Aggiorna Loglem

Il modo più semplice è lasciare che Loglem si aggiorni da solo da una versione firmata. La cartella `data/` resta. (`install.sh` non sovrascrive un'installazione esistente; per sostituire il programma a mano, ferma prima il servizio.)

```sh
loglem update check
loglem update install
loglem update status
```

Funziona solo per un Loglem installato in una cartella in cui l'utente può scrivere e registrato con `loglem service install`. Prima di sostituire qualcosa controlla firma, versione e dimensione; se la nuova versione non parte, il programma precedente resta accanto a quello nuovo (`loglem.previous`). Per un programma compilato in locale, indica prima dove cercare e di quale chiave fidarsi: `loglem update configure --repository PROPRIETARIO/REPOSITORY --public-key CHIAVE_BASE64`. Gli aggiornamenti che cambiano il formato dei dati vengono rifiutati.
