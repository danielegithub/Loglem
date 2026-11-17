# If Loglem stops

Loglem can be off: a crash, a reboot, an update, a mistake. This is what you lose.

| Source | While Loglem is off | What is lost |
|---|---|---|
| Files, Caddy, Nginx | Programs keep writing and rotating. | Nothing, until the program deletes old files by itself (`roll_keep`, `roll_keep_for`, logrotate). |
| Podman, Docker | The runtime keeps the logs. | Lines that the runtime deleted, and all lines if the container was removed or recreated. |
| Kubernetes | Pods keep logs. | The lines of a pod that was replaced. Follow `app=name`, not one pod. |
| Linux journal | The journal keeps entries. | Entries the journal deleted for space or age. |

Also while Loglem is off: expired lines are not deleted, and original files are not cleaned up.

## What Loglem does

- **When it starts again** it reads from the saved point. The card says "Resumed after 3 hours: recovered N lines".
- **Gaps are shown.** If lines may be missing, the card, the warning box and `loglem check` say so, with an estimated start and end.
- **Clean stop.** On a normal stop (`systemctl stop`, an update) it reads every source one last time, for at most 30 seconds.
- **Crash.** At the next start it says the last stop was not clean. It resumes from the last saved point. A line is saved together with its read position, so nothing is read twice.
- **Restart by itself.** The service restarts after a crash, at most 10 times in 5 minutes.
- **Alert.** `loglem check` fails if Loglem is off or a source has not been read for 15 minutes. Run it from cron or a monitor.
- **Caddy margin.** Loglem warns if `roll_keep_for` is under 7 days, or if unread rotated files are close to `roll_keep`.

## Make it safer

- Container logs: follow the container by name through the journal if it writes there (the page offers it). It survives a new release.
- Kubernetes: use `--selector app=name`.
- Keep `roll_keep_for` of Caddy at 30 days or more.
- Run `loglem check` every few minutes.
