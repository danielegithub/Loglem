# Usare Loglem

Pochi passi. Tutti i comandi sono in [cli.md](cli.md).

## 1. Aggiungi una sorgente

Una **sorgente** è il posto da cui arrivano i log. Aggiungila dalla pagina web (**Aggiungi sorgente**), dal menu del terminale o dalla riga di comando.

| Sorgente | Comando |
|---|---|
| Cartella di file `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Un file (e le sue copie ruotate) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Log di accesso di Caddy | `loglem source add-caddy --name site --path /var/log/caddy/access.log` |
| Container Podman | `loglem source add-podman --name db --container db` |
| Container Docker | `loglem source add-docker --name db --container db` |
| Pod Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal di Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` cerca i log di Caddy, Nginx e Apache su questo server. La pagina web li propone con un clic.

Anche i file compressi `.gz` vengono letti. Loglem importa lo storico una sola volta e non duplica mai una riga.

## 2. Cerca e segui in diretta

Apri **Esplora i log**. Scrivi un testo, scegli le date, oppure apri **Filtri avanzati**: gravità, indirizzo IP, percorso, codice di risposta.

Le righe nuove compaiono in cima da sole. Nel terminale:

```sh
loglem search --text login --ignore-case --status 404 --source site
loglem tail --source site --ip 203.0.113.7
```

Per le statistiche di un log di accesso (richieste per giorno, pagine più viste, codici, IP più frequenti): `loglem stats site`, oppure il pulsante **Statistiche** sulla scheda.

## 3. Scegli per quanto tenere i log

Ogni sorgente ha una durata in giorni, mesi o anni. Le copie vecchie si cancellano da sole. I file originali non vengono toccati.

```sh
loglem source set site --days 28
```

## 4. Tieni Loglem acceso

`loglem serve` legge i log e serve la pagina web. La pagina ha sempre lo stesso indirizzo, anche dopo un riavvio.

```sh
loglem service install --start    # Linux: systemd. macOS: launchd.
loglem service status
loglem check                      # codice di uscita 1 se qualcosa non va
```

Il servizio riparte da solo dopo un arresto anomalo. Quando lo fermi normalmente, legge ogni sorgente un'ultima volta. Cosa succede mentre Loglem è spento: [stopping.md](stopping.md).

## 5. Lascia che Loglem cancelli i file originali

Spento di default. Loglem non tocca mai il file su cui un programma sta scrivendo. Cancella solo file finiti, dopo questo controllo:

1. Loglem copia il file nel suo archivio.
2. Confronta la copia con l'originale, byte per byte. Se manca una riga, o è scaduta, il controllo si ferma.
3. Aspetta che nessuno modifichi il file da **24 ore**.
4. Solo allora cancella l'originale.

Se un passo fallisce, l'originale resta.

**Caddy.** Aggiungi la sorgente con `add-caddy`. Loglem legge i file ruotati (anche `.gz`) e li cancella dopo il controllo. Usa il blocco `log` mostrato dalla pagina. `roll_keep_for 60d` è una rete di sicurezza.

**Altri programmi.** Fai mettere al programma i file *finiti* in una cartella a parte, poi:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Non indicare mai una cartella in cui un programma scrive ancora.

## 6. Apri la pagina da un altro computer

Spento di default: la pagina risponde solo su questo computer. Per aprirla altrove:

```sh
loglem access user-add anna --role admin     # reader = può solo guardare
loglem access address 10.0.0.1               # un indirizzo di questo server, per esempio quello della VPN
```

Riavvia Loglem. Cinque password sbagliate bloccano quel computer per 15 minuti. Il collegamento **non è cifrato**: usa una VPN, oppure un reverse proxy con HTTPS:

```sh
loglem access proxy --trust 127.0.0.1 --base-path /loglem
```

Stampa un blocco per il Caddyfile. Per seguire in diretta da un altro computer: `loglem tail --server http://10.0.0.1:8790 --user anna`.

## 7. Allarmi e backup

```sh
loglem alerts add-pattern accessi-falliti --text "Failed password" --count 5 --minutes 10
loglem alerts command 'echo "$LOGLEM_ALERTS" | mail -s Loglem me@example.com'
loglem backup /backups/loglem-2026-11-17
loglem backup-verify /backups/loglem-2026-11-17
```

Gli allarmi compaiono nella pagina, in `loglem check` e in `data/health.json`. **Segna come visto** ne nasconde uno finché non ricompare. Un backup conserva anche le righe nel frattempo scadute.

## 8. Dove sono i tuoi dati

```text
data/
├── config.json     sorgenti e impostazioni
├── archives/       i log salvati, un database per sorgente
├── audit/          chi ha cercato, esportato o cancellato
├── proof/          registri dei sigilli (modalità prova)
└── diagnostics/    gli errori di Loglem
```

Per un altro posto usa `--data-dir /percorso/assoluto` o `LOGLEM_DATA_DIR`. Sposta le impostazioni con `loglem config export` e `config import`. Sposta i log fermando Loglem e copiando `data/`.

Gli archivi delle versioni vecchie vengono convertiti all'avvio. Il testo salvato resta lo stesso, il file diventa più piccolo.

## 9. Se qualcosa non va

- Una sorgente mostra un errore: le altre continuano. Apri la sua scheda, oppure `loglem status`.
- Gli errori di Loglem: **Problemi** nella pagina, oppure `loglem diagnostics`.
- Porta occupata: `loglem config web-port 8790`, poi riavvia.
- Controlla il programma: `loglem self-check`.

## 10. Aggiorna

Premi **Controlla** nella pagina **Sistema**, oppure:

```sh
loglem update check
loglem update install
```

Si installano solo versioni firmate (Linux). `data/` resta com'è. Il vecchio programma resta come `loglem.previous`.

## 11. In un container

```sh
podman build -t loglem -f Containerfile .
podman run -d --name loglem -p 127.0.0.1:8790:8790 \
  -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='una password lunga' \
  -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro loglem
```

Apri `http://127.0.0.1:8790` e aggiungi `/logs/myapp` come sorgente cartella. I dati restano nel volume `loglem-data`. Podman, Docker e il journal dell'host non si raggiungono da dentro: per quelli usa Loglem sull'host, oppure monta i file di log in `/logs`. Per pulire i file originali, monta la cartella senza `:ro`.

Dati personali, strumenti GDPR e modalità prova: [privacy.md](privacy.md).

## Regole, siti e spazio

In **Regole e siti**, scegli un modello o crea una classe. Scrivi il motivo e assegna le sorgenti. Salvare una modifica alla classe aggiorna tutte le sorgenti assegnate. Una sorgente può restare individuale. Il campo sito raggruppa anche sorgenti di tipi diversi. In **Esplora i log** i filtri per sito, data e testo valgono anche per le quattro domande preimpostate. Gli errori di cron sono quelli registrati nei messaggi del servizio.

La quota in MiB riguarda i database SQLite delle sorgenti della classe. La quota totale comprende anche le sorgenti individuali. A fine raccolta Loglem elimina prima le righe più vecchie per data dell’evento. Gli archivi con la modalità prova attiva o con una catena di sigilli sono esclusi dalla cancellazione per spazio. Se da soli superano la quota, la raccolta segnala il problema. I file originali, i backup e i file temporanei richiedono spazio aggiuntivo. Nella tabella, lo storico attuale parte dalla prima riga ancora presente.

La compressione si applica in lotti ai messaggi raccolti nei giorni UTC già conclusi. I messaggi restano ricercabili e i sigilli verificabili. Un archivio che contiene messaggi compressi richiede questa versione di Loglem o una successiva.

In **Mappa del server**, avvia la scansione dei log leggibili in `/var/log` e `/srv`, dei file indicati nelle configurazioni web, dei container e dei servizi del journal. Per i log su file trovi la dimensione del file. **Misura messaggi** conta i byte leggibili dove il runtime usa un archivio condiviso. Per Traefik puoi anche scegliere il file di accesso direttamente; per cron la scelta guidata prepara i filtri del journal.

## Backup cifrato via SSH

In **Sistema** puoi creare e verificare un backup locale, configurare gli allarmi e verificare i sigilli di una sorgente. Per le marche temporali serve l’indirizzo HTTPS della tua autorità RFC 3161.

Per il backup remoto prepara una coppia OpenPGP su un altro computer e custodisci lì la chiave privata. Sul server importa la sola chiave pubblica nel portachiavi GPG dell’utente che esegue Loglem. Lo stesso utente deve poter scrivere e leggere la cartella remota tramite SSH con autenticazione non interattiva e chiave host già verificata. Servono GPG, tar, rsync e il client SSH sul server e rsync a destinazione.

In **Sistema → Backup cifrato fuori dal server** indica l’impronta completa della chiave, la destinazione `utente@host:/cartella` e l’intervallo in ore. Puoi includere tutte le sorgenti o soltanto quelle di classi contrassegnate per il backup. Lascia Loglem in esecuzione con `web` o `serve`. Il servizio verifica lo snapshot locale, lo cifra, lo trasferisce e confronta l’impronta del file remoto riletto. Ogni esecuzione riuscita crea un nuovo file. La conservazione delle copie remote si gestisce sul server di destinazione.

Per recuperare i dati, porta il file cifrato sul computer con la chiave privata. Estrarlo in una cartella privata e verificare il manifesto prima del ripristino:

```sh
mkdir -m 700 recupero
gpg --output recupero/snapshot.tar.gz --decrypt loglem-BACKUP.tar.gz.gpg
tar -xzf recupero/snapshot.tar.gz -C recupero
loglem backup-verify recupero
loglem --data-dir /percorso/nuovo restore recupero
```
