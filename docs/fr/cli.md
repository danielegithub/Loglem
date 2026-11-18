# Commandes

`loglem COMMANDE --help` montre toutes les options. Sans commande, Loglem ouvre le menu du terminal. Mettez `--data-dir /chemin/absolu` avant la commande pour utiliser un autre dossier de données (ou définissez `LOGLEM_DATA_DIR`).

## Sources et lecture

| Commande | Ce qu'elle fait |
|---|---|
| `site add example.com`, `site list`, `site show example.com` | Crée un site, liste les sites ou affiche un site et ses journaux. |
| `site copies example.com`, `site duplicates example.com [--external]` | Compte les emplacements des originaux et des archives et compare les lignes récentes. Avec `--external`, examine aussi les fichiers locaux, le journal et les conteneurs. |
| `source add-folder / add-file / add-podman / add-docker / add-kubernetes / add-journal` | Ajoute une source. |
| `source list`, `source set NOM --days N` | Liste les sources, change la durée. |
| `source configure NOM …` | Nom, dossier des fichiers terminés, nettoyage des fichiers tournés, chemin. |
| `discover` | Cherche les journaux dans la configuration des programmes connus (`src/recipes/`). |
| `add`, `site rename / remove` | Ajout guidé (quoi, qui, où, récapitulatif) ; renommer ou supprimer un site. |
| `podman-list`, `docker-list`, `kubernetes-list` | Liste les conteneurs ou les pods. |
| `collect` | Lit une fois. |
| `serve` / `web` | Lit en permanence et sert la page web. |
| `status [--json]` | Emplacements, tailles, dates, état, trous. |
| `check [--minutes N]` | Code de sortie 1 si Loglem est éteint, si une source n'est pas lue ou si une alerte se déclenche. |

## Chercher et suivre

| Commande | Ce qu'elle fait |
|---|---|
| `search` | `--text --ignore-case --level error --ip --path --status --from --to --source --limit --json` |
| `tail` | Mêmes filtres, en direct. `--server URL --user NOM` suit un Loglem distant. |
| `stats SOURCE [--days N] [--json]` | Requêtes par jour, pages, codes, IP, pages lentes. |

## Soigner les données

| Commande | Ce qu'elle fait |
|---|---|
| `alerts list / add-pattern / add-size / add-free-space / remove / command / seen / monitor / email-set / email-test / email-off / email-show` | Règles d'alerte et notification. |
| `backup DIR`, `backup-verify DIR`, `restore DIR [--replace]` | Copie, contrôle et restaure les données. |
| `proof enable / disable / verify` | Mode preuve : scelle chaque jour. |
| `privacy set SOURCE …`, `privacy report` | Masque les IP, garde des champs JSON, liste ce qui est conservé. |
| `erase … [--originals]`, `export …`, `audit [--days N]` | RGPD : efface (aussi dans les fichiers d'origine) ou exporte les lignes d'une personne, voir qui a regardé. Aussi dans la page **Données personnelles**. |
| `prune [--apply]` | Montre ou supprime les lignes expirées. |
| `archive-completed` | Archive et contrôle à la main un fichier terminé. |

## Exécuter et maintenir

| Commande | Ce qu'elle fait |
|---|---|
| `service install [--start] / uninstall / start / stop / status` | Service : systemd sous Linux, launchd sous macOS. |
| `access user-add / user-remove / user-list / address / proxy` | Autres ordinateurs : utilisateurs, adresse, reverse proxy. |
| `config export / import / poll / web-port` | Réglages. |
| `update check / install / status / configure` | Mises à jour signées (Linux). |
| `diagnostics`, `self-check`, `init` | Erreurs de Loglem, un test rapide, une configuration vide. |
