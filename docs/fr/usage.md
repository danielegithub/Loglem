# Utiliser Loglem

Quelques étapes. Toutes les commandes sont dans [cli.md](cli.md).

## 1. Ajouter une source

Une **source** est l'endroit d'où viennent les journaux. Ajoutez-la depuis la page web (**Ajouter une source**), le menu du terminal ou la ligne de commande.

| Source | Commande |
|---|---|
| Dossier de fichiers `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Un fichier (et ses copies tournées) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Journal d'accès de Caddy | `loglem source add-caddy --name site --path /var/log/caddy/access.log` |
| Conteneur Podman | `loglem source add-podman --name db --container db` |
| Conteneur Docker | `loglem source add-docker --name db --container db` |
| Pods Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` cherche les journaux de Caddy, Nginx et Apache sur ce serveur. La page web les propose en un clic.

Les fichiers compressés `.gz` sont lus aussi. Loglem importe l'ancien historique une seule fois et ne duplique jamais une ligne.

## 2. Chercher et suivre en direct

Ouvrez **Explorer les journaux**. Saisissez un texte, choisissez des dates, ou ouvrez **Filtres avancés** : gravité, adresse IP, chemin, code de réponse.

Les nouvelles lignes apparaissent en haut toutes seules. Dans un terminal :

```sh
loglem search --text login --ignore-case --status 404 --source site
loglem tail --source site --ip 203.0.113.7
```

Pour les statistiques d'un journal d'accès (requêtes par jour, pages les plus vues, codes, IP les plus fréquentes) : `loglem stats site`, ou le bouton **Statistiques** sur la fiche de la source.

## 3. Choisir la durée de conservation

Chaque source a une durée en jours, mois ou années. Les vieilles copies sont supprimées pour vous. Les fichiers d'origine ne sont pas touchés.

```sh
loglem source set site --days 28
```

## 4. Garder Loglem en marche

`loglem serve` collecte les journaux et sert la page web. La page garde la même adresse, même après un redémarrage.

```sh
loglem service install --start    # Linux : systemd. macOS : launchd.
loglem service status
loglem check                      # code de sortie 1 si quelque chose ne va pas
```

Le service redémarre seul après un plantage. À un arrêt normal, il lit chaque source une dernière fois. Ce qui se passe quand Loglem est éteint : [stopping.md](stopping.md).

## 5. Laisser Loglem supprimer les fichiers d'origine

Désactivé par défaut. Loglem ne touche jamais le fichier dans lequel un programme écrit. Il supprime seulement les fichiers terminés, après ce contrôle :

1. Loglem copie le fichier dans son archive.
2. Il compare la copie à l'original, octet par octet. Une ligne manquante ou expirée arrête le contrôle.
3. Il attend que personne n'ait modifié le fichier depuis **24 heures**.
4. Alors seulement, il supprime l'original.

Si une étape échoue, l'original reste.

**Caddy.** Ajoutez la source avec `add-caddy`. Loglem lit les fichiers tournés (aussi `.gz`) et les supprime après le contrôle. Utilisez le bloc `log` affiché par la page. `roll_keep_for 60d` est un filet de sécurité.

**Autres programmes.** Faites déposer par le programme les fichiers *terminés* dans un dossier à part, puis :

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

N'indiquez jamais un dossier où un programme écrit encore.

## 6. Ouvrir la page depuis un autre ordinateur

Désactivé par défaut : la page répond seulement sur cet ordinateur. Pour l'ouvrir ailleurs :

```sh
loglem access user-add anna --role admin     # reader = peut seulement regarder
loglem access address 10.0.0.1               # une adresse de ce serveur, par exemple celle du VPN
```

Redémarrez Loglem. Cinq mots de passe faux bloquent cet ordinateur pendant 15 minutes. La liaison **n'est pas chiffrée** : utilisez un VPN, ou un reverse proxy en HTTPS :

```sh
loglem access proxy --trust 127.0.0.1 --base-path /loglem
```

Cela affiche un bloc de Caddyfile. Pour suivre en direct depuis un autre ordinateur : `loglem tail --server http://10.0.0.1:8790 --user anna`.

## 7. Alertes et sauvegarde

```sh
loglem alerts add-pattern echecs-connexion --text "Failed password" --count 5 --minutes 10
loglem alerts command 'echo "$LOGLEM_ALERTS" | mail -s Loglem me@example.com'
loglem backup /backups/loglem-2026-11-17
loglem backup-verify /backups/loglem-2026-11-17
```

Les alertes apparaissent dans la page, dans `loglem check` et dans `data/health.json`. **Marquer comme vu** en masque une jusqu'à son retour. Une sauvegarde garde aussi les lignes expirées depuis.

## 8. Où sont vos données

```text
data/
├── config.json     sources et réglages
├── archives/       journaux sauvegardés, une base par source
├── audit/          qui a cherché, exporté ou effacé
├── proof/          registres des scellés (mode preuve)
└── diagnostics/    les erreurs de Loglem
```

Pour un autre emplacement : `--data-dir /chemin/absolu` ou `LOGLEM_DATA_DIR`. Déplacez les réglages avec `loglem config export` et `config import`. Déplacez les journaux en arrêtant Loglem et en copiant `data/`.

Les archives des anciennes versions sont converties au démarrage. Le texte sauvegardé reste le même, le fichier devient plus petit.

## 9. En cas de problème

- Une source affiche une erreur : les autres continuent. Ouvrez sa fiche, ou `loglem status`.
- Les erreurs de Loglem : **Problèmes** dans la page, ou `loglem diagnostics`.
- Port occupé : `loglem config web-port 8790`, puis redémarrez.
- Contrôler le programme : `loglem self-check`.

## 10. Mettre à jour

Appuyez sur **Vérifier** dans la page **Système**, ou :

```sh
loglem update check
loglem update install
```

Seules les versions signées sont installées (Linux). `data/` est conservé. L'ancien programme reste sous le nom `loglem.previous`.

## 11. Dans un conteneur

```sh
podman build -t loglem -f Containerfile .
podman run -d --name loglem -p 127.0.0.1:8790:8790 \
  -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='un mot de passe long' \
  -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro loglem
```

Ouvrez `http://127.0.0.1:8790` et ajoutez `/logs/myapp` comme source dossier. Les données restent dans le volume `loglem-data`. Podman, Docker et le journal de l'hôte ne sont pas accessibles de l'intérieur : pour eux, lancez Loglem sur l'hôte, ou montez les fichiers de journal dans `/logs`. Pour nettoyer les fichiers d'origine, montez le dossier sans `:ro`.

Données personnelles, outils RGPD et mode preuve : [privacy.md](privacy.md).
