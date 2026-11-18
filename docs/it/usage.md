# Usare Loglem

Pochi passi. Tutti i comandi sono in [cli.md](cli.md).

## 1. Crea un sito e collega i log

Nella pagina iniziale scegli **Nuovo sito** e scrivi il nome del sito o progetto. Premi **Aggiungi un log** e scegli il file, la cartella o il servizio da leggere. La pagina del sito mostra origine, archivio, durata e ultime righe. **Cerca nei log del sito** cerca solo nei log collegati a quel sito.

| Sorgente | Comando |
|---|---|
| Cartella di file `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Un file (e le sue copie ruotate) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Qualsiasi log, guidato passo per passo (consigliato) | `loglem add` |
| Container Podman | `loglem source add-podman --name db --container db` |
| Container Docker | `loglem source add-docker --name db --container db` |
| Pod Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal di Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` cerca i log che i programmi conosciuti indicano nella loro configurazione (vedi `src/recipes/`). La pagina **Aggiungi** lo fa per te e spiega perché propone ogni log.

Anche i file compressi `.gz` vengono letti. Loglem importa lo storico una sola volta e non duplica mai una riga.

Puoi scegliere il sito o progetto quando aggiungi una sorgente. Da terminale aggiungi `--site example.com` a un comando `source add-*`. Per spostare una sorgente esistente usa `loglem source assign-site myapp example.com`. La pagina iniziale raggruppa i log per sito. Il journal parte in «Questo server» se lasci vuoto il sito. Durante la migrazione Loglem salva la configurazione precedente in `config.json.bak-AAAAMMGG`.

Apri un sito ed espandi **Copie** per vedere percorsi, spazio e responsabile della pulizia. Un file condiviso pesa una volta sola nel totale del sito. Loglem confronta fino a 200 righe recenti per sorgente e segnala le coppie con oltre la metà di righe uguali nel campione più grande. Puoi scegliere quale raccolta mettere in pausa. La ricerca esterna controlla su richiesta le cartelle dei log, `/var/log`, `/srv`, il journal locale, Podman e Docker, per un massimo di otto secondi. I byte identici comprendono solo file distinti verificati per intero.

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

Il servizio fa anche il controllo automatico (ogni 5 minuti, senza cron). La pagina Siti e **Sistema → Servizio** mostrano se è acceso, da quando e quante volte è ripartito. Per toglierlo: `loglem service uninstall`. I dati restano.

## 5. Lascia che Loglem cancelli i file originali

Spento di default. Loglem non tocca mai il file su cui un programma sta scrivendo. Cancella solo file finiti, dopo questo controllo:

1. Loglem copia il file nel suo archivio.
2. Confronta la copia con l'originale, byte per byte. Se manca una riga, o è scaduta, il controllo si ferma.
3. Aspetta che nessuno modifichi il file da **24 ore**.
4. Solo allora cancella l'originale.

Se un passo fallisce, l'originale resta.

**File ruotati.** Le nuove sorgenti conservano gli originali. Per eliminare i file ruotati verificati, attiva la pulizia nelle impostazioni del log.

**Altri programmi.** Fai mettere al programma i file *finiti* in una cartella a parte, poi:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Non indicare mai una cartella in cui un programma scrive ancora.

**Chi cancella gli originali?** Un solo posto: il riquadro «Pulizia dei file originali» nelle impostazioni della sorgente. Loglem legge le regole di logrotate e dice se esiste già una rotazione. Se logrotate (o il programma) ruota e cancella già i file, lascia **spenta** la pulizia di Loglem: due sistemi che cancellano gli stessi file non servono e nascondono gli errori. Loglem avvisa se sono accesi entrambi, se gli originali durano più della sua copia e se i file ruotati hanno nomi che non legge.

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

### Stato e avvisi per mail

Loglem si controlla da solo ogni 5 minuti (cambia in **Sistema → Controllo automatico**, oppure `loglem alerts monitor --every 10`). La pagina mostra «Tutto a posto, controllato alle 14:35» o cosa non va.

Gli avvisi per mail sono facoltativi e usano [Resend](https://resend.com):

```sh
loglem alerts email-set --from noreply@example.com --to admin@example.com   # chiede la chiave
loglem alerts email-test
loglem alerts email-off            # cancella anche la chiave
```

Crea una chiave di **sola spedizione** limitata al tuo dominio e verifica il dominio in Resend. Il mittente può essere `noreply@` su un sottodominio. Le risposte non vengono lette. Loglem manda una mail quando un problema si apre, una quando si chiude e un promemoria se resta aperto più di 24 ore. Le mail non contengono righe di log. Al massimo 6 all'ora. La chiave sta in un file riservato (`secrets/resend.key`): mai nelle impostazioni, nelle esportazioni, nei backup o nei log.

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
