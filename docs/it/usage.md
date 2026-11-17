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
