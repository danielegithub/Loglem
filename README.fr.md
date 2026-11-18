# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Un seul endroit pour les journaux de votre serveur : où ils sont, quelle place ils prennent et combien de temps ils restent.**

Loglem lit vos journaux, les copie dans une archive où l'on peut chercher, et supprime les copies quand le délai choisi est passé. Vous utilisez une page web ou un menu dans le terminal. Aucun fichier de configuration.

> **État : en cours de développement (bêta).** Ça fonctionne et c'est testé sur macOS. Il n'y a pas encore de version publiée, et certaines parties ne sont pas testées sur un vrai Linux. Voir les [limites](docs/fr/limitations.md).
>
> - **Avant de l'utiliser.** Il est conseillé de l'essayer d'abord sur une machine de test et de faire une sauvegarde.
> - **Suppression des fichiers.** L'activer est irréversible. Loglem étant encore en phase de test, il est conseillé d'en essayer les fonctions sur une machine de test, ou de bien réfléchir avant d'activer la suppression.
> - **Aucune garantie.** Logiciel libre sous licence MIT, fourni « tel quel », sans garantie d'aucune sorte. Vous l'utilisez à vos risques : les auteurs ne sont pas responsables des pertes de données ou des dommages.
> - **Pas un conseil juridique.** Les outils pour les données personnelles aident, mais ne rendent personne conforme aux règles, et ils reposent sur ce qui est diffusé en ligne. Les choix restent à la charge de celui qui l'utilise.

## Sommaire

- [Pourquoi Loglem](#pourquoi-loglem)
- [Ce qu'il lit](#ce-quil-lit)
- [Nettoyage sûr des fichiers d'origine](#nettoyage-sûr-des-fichiers-dorigine)
- [Démarrage rapide](#démarrage-rapide)
- [Installation](#installation)
- [Documentation](#documentation)
- [Licence](#licence)

## Pourquoi Loglem

Les journaux s'accumulent partout. Personne ne sait où ils sont, quelle taille ils ont, ni quand les supprimer.

- **Vous voyez tout.** Une liste : chaque source, où elle est, sa taille, son état.
- **Vous choisissez la durée.** 28 jours, 6 mois, 2 ans, par source. Les vieilles copies disparaissent seules.
- **Vous pouvez chercher.** Texte, dates, IP, chemin, code de réponse. Même après la rotation du fichier ou la suppression du conteneur. Les nouvelles lignes arrivent en direct.
- **Il reste petit.** Un programme d'environ 4 Mo. Environ 5 Mo de RAM (mesurés sous Linux). L'archive occupe environ un dixième de la place des journaux d'origine. Pas de serveur de base de données, pas de compte, pas de cloud. Vos journaux restent sur votre machine. [Mesures (en anglais)](docs/performance.md).
- **Il est sûr par défaut.** Les fichiers d'origine sont seulement lus. Les supprimer est facultatif et contrôlé (voir plus bas).
- **Il est simple.** Une page web, un menu terminal qui marche en SSH, et des commandes pour les scripts.
- **Il montre ce qui va mal.** Une source arrêtée, un trou dans les journaux, un disque plein : vous le voyez, et `loglem check` prévient votre supervision.
- **Il aide pour les données personnelles.** Masquer les IP, effacer ou exporter les lignes d'une personne, voir qui a consulté les journaux. Voir [vie privée](docs/fr/privacy.md).

## Ce qu'il lit

| Source | Remarques |
|---|---|
| Dossiers et fichiers texte `.log` | Aussi les fichiers tournés, aussi `.gz` (l'historique est importé une fois, jamais dupliqué). |
| Serveurs web, bases de données, mail, SSH, cron… | Reconnus grâce à leurs recettes dans `src/recipes/` : Nginx, Apache, Caddy, Traefik, HAProxy, PostgreSQL, MySQL et d'autres. En ajouter un tient en un fichier. |
| Podman, Docker | Un conteneur, ou par son nom via le journal. |
| Kubernetes | Un pod, ou tous les pods d'une application. |
| Journal Linux | Tout, ou certains services, programmes ou niveaux de gravité. |

## Nettoyage sûr des fichiers d'origine

Désactivé par défaut. Si vous l'activez pour une source, Loglem supprime un fichier terminé seulement après ce contrôle :

1. Il **copie** le fichier dans son archive.
2. Il **compare la copie à l'original, octet par octet**. Si une ligne manque, a expiré ou a changé, il s'arrête.
3. Il attend que personne n'ait modifié le fichier depuis **24 heures**.
4. Alors seulement, il supprime l'original.

Le fichier dans lequel un programme écrit encore n'est jamais touché. Si une étape échoue, l'original reste où il est. Valable pour les fichiers tournés de n'importe quel programme et pour les dossiers de fichiers terminés.

## Démarrage rapide

Après l'[installation](#installation) :

```sh
loglem                                  # menu dans le terminal
loglem discover                         # cherche les journaux dans la configuration des programmes connus
loglem source add-folder --name web --path /srv/example/logs --days 28
loglem collect
loglem search --source web --text '/login' --from 2026-11-01
loglem web                              # collecte et ouvre la page web (affiche l'adresse)
```

Pour continuer après la fermeture du terminal (Linux : systemd, macOS : launchd) :

```sh
loglem service install --start
```

Le menu et la page web sont en italien. Plus de détails dans [Utiliser Loglem](docs/fr/usage.md).

## Installation

Quand une version sera publiée, téléchargez le paquet pour votre machine dans l'onglet **Releases** :

| Machine | Fin du nom du paquet |
|---|---|
| Linux Intel/AMD 64 bits | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Les paquets demandent glibc 2.39 ou plus (par exemple Ubuntu 24.04). Les mises à jour, les horodatages et `loglem tail --server` utilisent la commande `curl` du système.

Vérifiez la signature avec la clé publique de l'éditeur (`release-public.pem`), obtenue par un canal indépendant :

```sh
openssl pkeyutl -verify -pubin -inkey release-public.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
sha256sum --check --ignore-missing SHA256SUMS
```

Puis installez :

```sh
tar xzf loglem-*.tar.gz
cd loglem-*
sh install.sh
~/Loglem/loglem
```

Tout est dans `~/Loglem` : le programme et un dossier `data/`. L'installateur ne démarre aucun service. Pour le compiler vous-même, voir [CONTRIBUTING.md](CONTRIBUTING.md).

**Dans un conteneur.** Construisez-le avec `podman build -t loglem -f Containerfile .` et lancez-le en montant un dossier de journaux. Voir [Utiliser Loglem](docs/fr/usage.md#11-dans-un-conteneur).

## Documentation

[English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Page | Contenu |
|---|---|
| [Utiliser Loglem](docs/fr/usage.md) | Quelques étapes pour tout faire |
| [Commandes](docs/fr/cli.md) | Toutes les commandes |
| [Données personnelles et mode preuve](docs/fr/privacy.md) | Outils RGPD, effacement, export, journal d'accès, jours scellés |
| [Si Loglem s'arrête](docs/fr/stopping.md) | Ce que vous perdez et ce que fait Loglem |
| [Limites](docs/fr/limitations.md) | Ce qu'il ne fait pas ou ne promet pas |

## Licence

[MIT](LICENSE). Le texte complet de l'exclusion de garantie se trouve dans le fichier LICENSE.
