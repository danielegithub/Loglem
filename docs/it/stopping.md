# Se Loglem si ferma

Loglem può essere spento: un arresto anomalo, un riavvio, un aggiornamento, un errore. Ecco cosa perdi.

| Sorgente | Mentre Loglem è spento | Cosa si perde |
|---|---|---|
| File, Caddy, Nginx | I programmi continuano a scrivere e ruotare. | Niente, finché il programma non cancella da sé i file vecchi (`roll_keep`, `roll_keep_for`, logrotate). |
| Podman, Docker | Il programma dei container tiene i log. | Le righe che ha cancellato, e tutte se il container è stato tolto o ricreato. |
| Kubernetes | I pod tengono i log. | Le righe di un pod sostituito. Segui `app=nome`, non un solo pod. |
| Journal di Linux | Il journal tiene le voci. | Le voci che il journal ha cancellato per spazio o età. |

Mentre Loglem è spento le righe scadute non vengono cancellate e gli originali non vengono ripuliti.

## Cosa fa Loglem

- **Quando riparte** legge dal punto salvato. La scheda dice «Ripresa dopo 3 ore di fermo: recuperate N righe».
- **Le lacune si vedono.** Se possono mancare righe, lo dicono la scheda, il riquadro di avviso e `loglem check`, con un inizio e una fine stimati.
- **Arresto normale.** Con un arresto normale (`systemctl stop`, un aggiornamento) legge ogni sorgente un'ultima volta, al massimo per 30 secondi.
- **Arresto anomalo.** Al riavvio dice che l'ultimo arresto non è stato regolare. Riprende dall'ultimo punto salvato. Una riga si salva insieme alla sua posizione di lettura, quindi niente viene letto due volte.
- **Riparte da solo.** Il servizio riparte dopo un arresto anomalo, al massimo 10 volte in 5 minuti.
- **Allarme.** `loglem check` fallisce se Loglem è spento o una sorgente non viene letta da 15 minuti. Lancialo da cron o da un monitor.
- **Margine di Caddy.** Loglem avvisa se `roll_keep_for` è sotto i 7 giorni, o se i file ruotati non letti sono vicini a `roll_keep`.

## Per stare più sicuro

- Log dei container: segui il container per nome tramite il journal, se scrive lì (la pagina lo propone). Sopravvive a un nuovo rilascio.
- Kubernetes: usa `--selector app=nome`.
- Tieni `roll_keep_for` di Caddy a 30 giorni o più.
- Lancia `loglem check` ogni pochi minuti.
