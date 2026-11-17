# Si Loglem s'arrête

Loglem peut être éteint : un plantage, un redémarrage, une mise à jour, une erreur. Voici ce que vous perdez.

| Source | Pendant que Loglem est éteint | Ce qui est perdu |
|---|---|---|
| Fichiers, Caddy, Nginx | Les programmes continuent d'écrire et de faire tourner les fichiers. | Rien, tant que le programme ne supprime pas lui-même les vieux fichiers (`roll_keep`, `roll_keep_for`, logrotate). |
| Podman, Docker | Le programme des conteneurs garde les journaux. | Les lignes qu'il a supprimées, et toutes si le conteneur a été retiré ou recréé. |
| Kubernetes | Les pods gardent les journaux. | Les lignes d'un pod remplacé. Suivez `app=nom`, pas un seul pod. |
| Journal Linux | Le journal garde les entrées. | Les entrées supprimées par le journal pour manque de place ou d'âge. |

Pendant que Loglem est éteint, les lignes expirées ne sont pas supprimées et les fichiers d'origine ne sont pas nettoyés.

## Ce que fait Loglem

- **Au redémarrage**, il lit depuis le point sauvegardé. La fiche dit « Reprise après 3 heures d'arrêt : N lignes récupérées ».
- **Les trous se voient.** Si des lignes peuvent manquer, la fiche, le bandeau d'avertissement et `loglem check` le disent, avec un début et une fin estimés.
- **Arrêt normal.** À un arrêt normal (`systemctl stop`, une mise à jour), il lit chaque source une dernière fois, 30 secondes au plus.
- **Plantage.** Au démarrage suivant, il dit que le dernier arrêt n'était pas propre. Il reprend au dernier point sauvegardé. Une ligne est sauvegardée avec sa position de lecture : rien n'est lu deux fois.
- **Redémarrage automatique.** Le service redémarre après un plantage, 10 fois au plus en 5 minutes.
- **Alerte.** `loglem check` échoue si Loglem est éteint ou si une source n'est pas lue depuis 15 minutes. Lancez-le depuis cron ou une supervision.
- **Marge de Caddy.** Loglem prévient si `roll_keep_for` est sous 7 jours, ou si les fichiers tournés non lus approchent de `roll_keep`.

## Pour plus de sécurité

- Journaux de conteneurs : suivez le conteneur par son nom via le journal, s'il y écrit (la page le propose). Il survit à un nouveau déploiement.
- Kubernetes : utilisez `--selector app=nom`.
- Gardez `roll_keep_for` de Caddy à 30 jours ou plus.
- Lancez `loglem check` toutes les quelques minutes.
