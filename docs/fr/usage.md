# Utiliser Loglem

Un guide pas à pas. Pour la liste des commandes, voir [cli.md](cli.md) ; pour ce que Loglem ne fait pas, voir [limitations.md](limitations.md).

## Sommaire

- [1. Ajoutez votre première source](#1-ajoutez-votre-première-source)
- [2. Cherchez dans les journaux](#2-cherchez-dans-les-journaux)
- [3. Choisissez combien de temps garder les journaux](#3-choisissez-combien-de-temps-garder-les-journaux)
- [4. Gardez Loglem toujours actif](#4-gardez-loglem-toujours-actif)
- [5. Laissez Loglem nettoyer les fichiers d'origine](#5-laissez-loglem-nettoyer-les-fichiers-dorigine)
- [6. Modifiez, suspendez ou retirez une source](#6-modifiez-suspendez-ou-retirez-une-source)
- [7. Où sont vos données et comment les déplacer](#7-où-sont-vos-données-et-comment-les-déplacer)
- [8. Quand quelque chose ne va pas](#8-quand-quelque-chose-ne-va-pas)
- [9. Mettez Loglem à jour](#9-mettez-loglem-à-jour)

## 1. Ajoutez votre première source

Une **source** est l'endroit d'où viennent les journaux : un dossier de fichiers `.log`, un fichier unique, un conteneur Podman ou le journal d'accès de Caddy. Vous pouvez l'ajouter depuis la page web, depuis le menu du terminal ou en ligne de commande.

### Depuis la page web

```sh
loglem web
```

Loglem affiche l'adresse de la page (par exemple `http://127.0.0.1:53142`) et le dossier où il garde ses données. Ouvrez cette adresse dans un navigateur. L'adresse reste la même après un redémarrage. Si le port est pris par un autre programme, Loglem s'arrête et le dit ; choisissez-en un autre avec `loglem config web-port 8790`. Loglem n'utilise jamais le port 80 ni le 443.

La page ne fonctionne que sur la machine où tourne Loglem. Sur un serveur distant, ouvrez-la par un tunnel SSH (`ssh -L 53142:127.0.0.1:53142 utilisateur@serveur`, puis allez sur `http://127.0.0.1:53142`).

Le menu de gauche a quatre pages :

- **Sorgenti:** vos sources, avec leur état, le nombre de lignes et des liens vers leurs journaux et leurs réglages.
- **Esplora i log:** cherchez et lisez les journaux d'une source. La liste se rafraîchit toute seule ; une case à cocher la met en pause.
- **Aggiungi sorgente:** un formulaire guidé. Choisissez le type, le dossier ou le conteneur, un nom, le genre de journal et la durée de conservation, puis appuyez sur **Salva e apri i log**. Loglem lit les journaux tout de suite.
- **Problemi:** les erreurs de Loglem lui-même, avec leur cause (voir la [section 8](#8-quand-quelque-chose-ne-va-pas)).

### Depuis la ligne de commande

```sh
# un dossier de fichiers .log (les nouveaux fichiers sont pris en compte automatiquement)
loglem source add-folder --name web-logs --path /srv/example/logs --days 28

# un fichier unique ; lit aussi ses copies renommées, comme api.log-1
loglem source add-file --name api --path /srv/example/logs/api.log \
  --rotated-pattern 'api.log-*' --days 14

# un conteneur Podman
loglem podman-list
loglem source add-podman --name app --container my-container --days 14

# lire maintenant et vérifier
loglem collect
loglem status
```

Remarques :

- Une source dossier lit les fichiers `.log` qui se trouvent directement dans le dossier ; les sous-dossiers, les fichiers compressés et les autres extensions sont ignorés.
- Un fichier doit avoir un nom qui se termine par `.log`. Les fichiers compressés (`.gz`) ne sont pas lus.
- Pour Podman, Loglem retient l'ID complet du conteneur. Si vous recréez un conteneur du même nom, ajoutez-le comme nouvelle source (ou voyez la [section 6](#6-modifiez-suspendez-ou-retirez-une-source)). L'historique déjà collecté reste disponible après la suppression du conteneur. Avec `--connection NOM`, choisissez une connexion Podman nommée.
- Loglem lit seulement vos fichiers d'origine. Il ne les modifie pas, sauf si vous activez le nettoyage (section 5).
- Les noms et boutons de la page et du menu sont pour l'instant en italien.

### Depuis le menu du terminal

Lancez `loglem`. Utilisez les flèches (ou `j`/`k`), Entrée pour choisir et Échap pour revenir en arrière.

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

Choisissez **Aggiungi cartella, file o container**, puis le type, puis parcourez jusqu'au dossier (ou choisissez **Scrivi / incolla un percorso…** pour le saisir). Vérifiez le récapitulatif et enregistrez avec **Salva e leggi i log adesso**. Échap annule avant l'enregistrement.

## 2. Cherchez dans les journaux

Dans la page web, ouvrez **Esplora i log**, choisissez la source et saisissez votre texte. Vous pouvez limiter par date (UTC). Les lignes les plus récentes viennent en premier. En ligne de commande :

```sh
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16
```

- La recherche porte sur le texte exact et distingue majuscules et minuscules ; ce n'est pas une expression régulière.
- `--to 2026-11-16` inclut toute cette journée (UTC).
- Ajoutez `--json` pour une sortie lisible par d'autres programmes, et `--limit` pour obtenir jusqu'à 1000 résultats.
- Si une ligne n'a pas de date que Loglem reconnaît, elle reste consultable par recherche et s'affiche avec l'heure de sa lecture. Loglem reconnaît les journaux JSON courants (`ts`, `timestamp`, `time`, `@timestamp`).

## 3. Choisissez combien de temps garder les journaux

Chaque source a sa propre durée de conservation : un nombre de jours, de mois ou d'années. Quand une entrée dépasse cette durée, Loglem la supprime de son archive.

```sh
loglem source set web-logs --days 7
loglem source configure web-logs --amount 6 --unit months --log-kind access
loglem source configure web-logs --amount 2 --unit years
```

- Dans la page web et dans le menu du terminal (**Conservazione**), le même réglage est un formulaire qui montre aussi quand les entrées expireront.
- Les entrées expirées sont retirées pendant que Loglem tourne (voir section 4). `loglem prune` montre ce qui serait supprimé ; `loglem prune --apply` le supprime tout de suite.
- La conservation part de la date de la ligne ou de la première lecture par Loglem, la plus ancienne des deux : une ligne datée du futur ne reste pas pour toujours.
- Suspendre une source arrête les nouvelles lectures, pas la conservation.
- Le genre de journal (accès, général, erreurs) décrit seulement la source ; il ne fixe pas pour vous une durée légale.
- La durée s'applique à la copie de Loglem. Vos fichiers d'origine ne sont pas supprimés, sauf si vous activez le nettoyage (section 5).

## 4. Gardez Loglem toujours actif

`loglem web` ne collecte les journaux que tant qu'il tourne. Pour collecter en permanence, même après avoir fermé le terminal ou redémarré, installez-le comme service :

```sh
loglem service install --start
loglem service status
```

Cela crée un **service utilisateur systemd** pour l'utilisateur courant. Il lit les nouvelles lignes toutes les 10 secondes (modifiable avec `loglem config poll 5`), retire les entrées expirées et sert la page web à la même adresse.

- Sur un serveur, demandez à un administrateur d'exécuter une fois `sudo loginctl enable-linger UTILISATEUR`, pour que le service tourne même quand l'utilisateur n'est pas connecté.
- Le service n'a que les droits de cet utilisateur. Vérifiez que cet utilisateur peut lire les dossiers de journaux ; Loglem ne tourne pas en root.
- Un seul Loglem tourne par dossier de données. Si le service est déjà actif, `loglem web` n'en lance pas un second : il affiche l'adresse du service et se termine.
- Lancer Loglem avec un autre `--data-dir` montre d'autres sources. Le dossier de données utilisé est toujours indiqué dans la page web et dans le menu du terminal.

## 5. Laissez Loglem nettoyer les fichiers d'origine

Par défaut, Loglem ne supprime pas vos fichiers d'origine. Vous pouvez le laisser supprimer des fichiers d'origine **après les avoir copiés et vérifiés**. Il ne le fait que pour les fichiers que personne n'a modifiés depuis au moins 24 heures, jamais pour le fichier dans lequel un programme écrit, et il garde tout fichier qu'il ne peut pas vérifier (la fiche de la source affiche un avertissement). L'expiration de l'archive est une étape distincte, réglée à la section 3.

### Caddy

Dans **Aggiungi sorgente**, choisissez **Il log di accesso di Caddy**, ouvrez le dossier des journaux (souvent `/var/log/caddy`) et choisissez le fichier dans lequel Caddy écrit, en général `access.log`. Loglem lit alors aussi les fichiers que Caddy crée lors de la rotation du journal (par exemple `access-2026-11-16T10-00-00.000.log`) et les supprime après vérification. À la fin, la page affiche le bloc à mettre dans votre Caddyfile :

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

- `roll_uncompressed` est obligatoire : Loglem ne vérifie que des fichiers non compressés. S'il trouve des fichiers `.gz`, il les signale sur la fiche.
- `roll_keep_for 60d` est un filet de sécurité : si Loglem reste éteint longtemps, Caddy supprime quand même les fichiers les plus anciens.
- Loglem ne modifie pas votre Caddyfile ; vous collez le bloc vous-même.

En ligne de commande : `loglem source add-caddy --name site --path /var/log/caddy/access.log --days 90`. Pour une source fichier qui a déjà un motif de rotation : `loglem source configure site --remove-rotated true`.

### Autres programmes

Demandez au programme de déplacer chaque fichier terminé, fermé et non compressé, dans un dossier dédié. Dans les réglages de la source, activez **Archivia ed elimina i file di una cartella dedicata** et confirmez. Ne sélectionnez jamais un dossier dans lequel un programme écrit encore.

```sh
loglem source configure web --completed-dir /absolute/completed-files --handoff-confirmed
# pour le désactiver à nouveau
loglem source configure web --completed-dir ""
```

Pour un seul fichier terminé, sans attendre 24 heures :

```sh
loglem archive-completed --source web --file /srv/example/logs/access-old.log --producer-closed --remove
```

Sans `--remove`, la commande archive et vérifie le fichier sans supprimer l'original. Loglem ne supprime jamais les originaux d'un conteneur Podman.

## 6. Modifiez, suspendez ou retirez une source

Dans la page web, appuyez sur **Impostazioni** sur la fiche de la source. En ligne de commande :

```sh
loglem source rename web "Site public"
loglem source pause web          # arrête la lecture, l'historique reste consultable par recherche
loglem source resume web
loglem source detach web         # déconnecte ; l'archive reste consultable par recherche
loglem source configure web --container NEW_CONTAINER   # pointe vers un conteneur de remplacement
loglem source configure web --path /new/path            # nouveau fichier ou dossier
```

- Renommer ne change que l'étiquette ; l'historique est conservé.
- Pour garder l'archive d'une source dans un autre dossier : `loglem source configure web --archive-dir /dossier/absolu`. Loglem la copie, vérifie la copie puis supprime les anciens fichiers ; il n'écrase jamais une base existante. Avec `--archive-dir ""`, vous revenez au dossier par défaut. Fermez d'abord les autres programmes qui ont l'archive ouverte.

## 7. Où sont vos données et comment les déplacer

Par défaut, tout se trouve dans le dossier de Loglem :

```text
Loglem/
├── loglem          le programme
└── data/
    ├── config.json   vos sources et réglages
    ├── archives/     les journaux enregistrés (une base par source)
    ├── diagnostics/  le journal des erreurs de Loglem
    └── updates/
```

Pour utiliser un autre emplacement, passez `--data-dir /chemin/absolu` avant la commande, ou définissez `LOGLEM_DATA_DIR`.

Pour passer à une autre machine :

```sh
loglem config export ./loglem-config.json     # sur l'ancienne machine
loglem config import ./loglem-config.json     # sur la nouvelle machine
```

Cela déplace vos **réglages** (sources, durées), pas les journaux. Adaptez chemins et permissions sur la nouvelle machine. Pour déplacer aussi les journaux, arrêtez Loglem et copiez tout le dossier `data/`. Importer une configuration ne retire jamais la règle de conservation d'une source qui a déjà une archive ; suspendez plutôt cette source.

## 8. Quand quelque chose ne va pas

- **Une source affiche une erreur :** les autres continuent de fonctionner et les lignes déjà enregistrées restent lisibles. Ouvrez la fiche de la source pour voir le message, ou lancez `loglem status`.
- **La page est vide ou une source dit « impossible de lire » :** vérifiez que l'utilisateur qui lance Loglem peut lire le dossier ou le fichier.
- **Voir les erreurs de Loglem :** **Problemi** dans la page web, ou `loglem diagnostics`. Chaque entrée indique l'heure, ce que Loglem faisait, la cause et les détails techniques. Le fichier est `data/diagnostics/loglem.log` (environ 5 Mo, puis rotation ; lisible seulement par son propriétaire).
- **Vérifier l'installation :** `loglem self-check`.
- **Port déjà utilisé :** `loglem config web-port 8790`, puis redémarrez.

## 9. Mettez Loglem à jour

Le plus simple est de laisser Loglem se mettre à jour depuis une version signée. Le dossier `data/` est conservé. (`install.sh` refuse de s'exécuter sur une installation existante ; pour remplacer le programme à la main, arrêtez d'abord le service.)

```sh
loglem update check
loglem update install
loglem update status
```

Cela ne fonctionne que pour un Loglem installé dans un dossier où l'utilisateur peut écrire et enregistré avec `loglem service install`. Avant de remplacer quoi que ce soit, il vérifie la signature, la version et la taille ; si la nouvelle version ne démarre pas, le programme précédent est conservé à côté (`loglem.previous`). Pour un programme compilé localement, indiquez d'abord où chercher et à quelle clé faire confiance : `loglem update configure --repository PROPRIETAIRE/DEPOT --public-key CLE_BASE64`. Les mises à jour qui changent le format des données sont refusées.
