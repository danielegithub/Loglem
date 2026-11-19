# Utiliser Loglem

Quelques étapes. Toutes les commandes sont dans [cli.md](cli.md).

## 1. Créer un site et connecter ses journaux

Sur la page d’accueil, choisissez **Nuovo sito** et saisissez le nom du site ou du projet. Avec **Aggiungi un log**, sélectionnez le fichier, le dossier ou le service à lire. La page du site affiche l’origine, l’archive, la durée de conservation et les dernières lignes. **Cerca nei log del sito** limite la recherche à ce site.

| Source | Commande |
|---|---|
| Dossier de fichiers `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Un fichier (et ses copies tournées) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| N'importe quel journal, guidé pas à pas (conseillé) | `loglem add` |
| Conteneur Podman | `loglem source add-podman --name db --container db` |
| Conteneur Docker | `loglem source add-docker --name db --container db` |
| Pods Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` cherche les journaux que les programmes connus indiquent dans leur configuration (voir `src/recipes/`). La page **Ajouter** le fait pour vous et explique pourquoi elle propose chaque journal.

Les fichiers compressés `.gz` sont lus aussi. Loglem importe l'ancien historique une seule fois et ne duplique jamais une ligne.

Choisis le site ou le projet lors de l’ajout d’une source. En ligne de commande, ajoute `--site example.com` à une commande `source add-*`. Pour déplacer une source existante, utilise `loglem source assign-site myapp example.com`. La page d’accueil regroupe les journaux par site. Les sources du journal système sans site rejoignent le groupe système. Lors de la migration, Loglem sauvegarde la configuration précédente dans `config.json.bak-AAAAMMJJ`.

Ouvrez un site et dépliez **Copie** pour voir les chemins, les tailles et le responsable du nettoyage. Un fichier partagé compte une seule fois dans le total du site. Loglem compare jusqu’à 200 lignes récentes par source. Si plus de la moitié du plus grand échantillon correspond, vous pouvez suspendre une des deux collectes. La recherche externe examine sur demande les répertoires de journaux, `/var/log`, `/srv`, le journal local, Podman et Docker pendant huit secondes au maximum. Le total des octets identiques comprend uniquement les fichiers distincts vérifiés en entier.

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

Le service est une unité utilisateur de systemd. Sur un serveur, exécutez une fois `loginctl enable-linger UTILISATEUR` pour qu'il démarre avec la machine et continue après la fermeture de SSH. `loglem service install` vous le dit quand c'est nécessaire.

Le service redémarre seul après un plantage. À un arrêt normal, il lit chaque source une dernière fois. Ce qui se passe quand Loglem est éteint : [stopping.md](stopping.md).

Le service fait aussi le contrôle automatique (toutes les 5 minutes, sans cron). La page Sites et **Système → Service** montrent s'il tourne, depuis quand et combien de fois il a redémarré. Pour le retirer : `loglem service uninstall`. Les données restent.

## 5. Laisser Loglem supprimer les fichiers d'origine

Désactivé par défaut. Loglem ne touche jamais le fichier dans lequel un programme écrit. Il supprime seulement les fichiers terminés, après ce contrôle :

1. Loglem copie le fichier dans son archive.
2. Il compare la copie à l'original, octet par octet. Une ligne manquante ou expirée arrête le contrôle.
3. Il attend que personne n'ait modifié le fichier depuis **24 heures**.
4. Alors seulement, il supprime l'original.

Si une étape échoue, l'original reste.

**Fichiers après rotation.** Les nouvelles sources conservent les originaux. Pour supprimer les fichiers après vérification, activez le nettoyage dans les paramètres de la source.

**Autres programmes.** Faites déposer par le programme les fichiers *terminés* dans un dossier à part, puis :

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

N'indiquez jamais un dossier où un programme écrit encore.

**Qui supprime les originaux ?** Un seul endroit : le cadre de nettoyage dans les réglages de la source. Loglem lit les règles de logrotate et dit si une rotation existe déjà. Si logrotate (ou le programme) fait déjà tourner et supprime les fichiers, laissez le nettoyage de Loglem **désactivé** : deux systèmes qui suppriment les mêmes fichiers ne servent à rien et cachent les erreurs. Loglem prévient quand les deux sont actifs, quand les originaux durent plus longtemps que sa copie et quand les fichiers tournés ont des noms qu'il ne lit pas.

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

### État et alertes par e-mail

Loglem se contrôle seul toutes les 5 minutes (à changer dans **Système → Contrôle automatique**, ou `loglem alerts monitor --every 10`). La page affiche « Tout va bien, contrôlé à 14:35 » ou ce qui ne va pas.

Les alertes par e-mail sont facultatives et utilisent [Resend](https://resend.com) :

```sh
loglem alerts email-set --from noreply@example.com --to admin@example.com   # demande la clé
loglem alerts email-test
loglem alerts email-off            # supprime aussi la clé
```

Créez une clé **d'envoi seulement**, limitée à votre domaine, et vérifiez le domaine dans Resend. L'expéditeur peut être `noreply@` sur un sous-domaine. Les réponses ne sont pas lues. Loglem envoie un e-mail quand un problème s'ouvre, un quand il se ferme et un rappel s'il reste ouvert plus de 24 heures. Les e-mails ne contiennent aucune ligne de journal. Six par heure au maximum. La clé est dans un fichier privé (`secrets/resend.key`) : jamais dans les réglages, les exports, les sauvegardes ou les journaux.

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
