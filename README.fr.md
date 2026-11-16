# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Un seul endroit pour les journaux de votre serveur : où ils sont, combien de place ils prennent et combien de temps ils sont conservés.**

Loglem lit les journaux de votre serveur Linux (fichiers `.log` et conteneurs Podman), les copie dans une archive consultable et supprime automatiquement les entrées archivées une fois la durée de conservation choisie écoulée. Vous gérez tout depuis une page web accessible sur le serveur lui-même ou depuis un menu du terminal navigable au clavier, sans modifier de fichier de configuration.

> **État : expérimental.** Il fonctionne et il est testé, mais aucune version n'est encore publiée et certaines parties doivent encore être vérifiées de bout en bout sur un vrai serveur. Voir les [limites](docs/fr/limitations.md).

## Sommaire

- [Pourquoi Loglem](#pourquoi-loglem)
- [Ce qu'il fait](#ce-quil-fait)
- [Démarrage rapide](#démarrage-rapide)
- [Installation](#installation)
- [Documentation](#documentation)
- [Licence](#licence)

## Pourquoi Loglem

Les journaux s'accumulent à de nombreux endroits : un dossier pour le serveur web, un autre pour l'application, la sortie de chaque conteneur. Personne ne sait où ils sont, quelle taille ils ont atteinte ni quand il faudrait les supprimer.

Avec Loglem :

- **Vous savez toujours où sont les journaux.** Une liste montre chaque source, son emplacement d'origine, l'emplacement de l'archive, la taille et l'état de la dernière lecture.
- **Vous décidez combien de temps conserver les journaux de chaque source.** 28 jours, 6 mois ou 2 ans, source par source. Les entrées expirées sont supprimées pour vous : le disque ne se remplit pas et les anciennes données ne sont pas conservées par oubli.
- **Vous pouvez chercher.** Texte et plages de dates sur tout ce qui a été collecté, même après la rotation du fichier d'origine ou la suppression du conteneur.
- **Il reste discret.** Un seul exécutable d'environ 4 Mo, environ 10 Mo de RAM au repos, pas de serveur de base de données, pas de compte, pas de service cloud. Vos journaux ne quittent jamais la machine.
- **Il est sûr par défaut.** Les fichiers d'origine sont seulement lus, jamais modifiés. Supprimer un original est facultatif et ne concerne que les fichiers que vous confiez explicitement à Loglem une fois leur écriture terminée.
- **Il est simple à utiliser.** Tout se fait depuis une page qui n'écoute que sur le serveur lui-même, ou depuis un menu de terminal qui fonctionne aussi par SSH. Des commandes pour les scripts et l'automatisation sont aussi disponibles.
- **Il remplace un tas de scripts.** Collecter, nettoyer, conserver et chercher dans les journaux demande d'ordinaire un mélange de règles de rotation, de tâches planifiées et de commandes improvisées. Loglem fait tout cela au même endroit, avec les mêmes réglages dans la page web, le terminal et la ligne de commande.
- **Il vous suit partout.** Chemins, conteneurs et durées sont des réglages, pas du code : exportez la configuration et importez-la sur une autre machine.

## Ce qu'il fait

1. **Vous ajoutez une source :** un dossier de fichiers `.log`, un fichier unique, un conteneur Podman ou le journal d'accès de Caddy.
2. **Loglem collecte les nouvelles lignes** toutes les 10 secondes par défaut et les stocke dans une petite archive SQLite propre à cette source. Il retient où il s'est arrêté, donc rien n'est lu deux fois.
3. **Vous cherchez et vérifiez** depuis la page web ou le terminal, et vous voyez l'état de chaque source.
4. **Les anciennes entrées sont supprimées** quand elles dépassent la durée fixée pour cette source.
5. **En cas de problème,** les erreurs de Loglem sont enregistrées avec leur cause et visibles dans une page Diagnostic. Une source en échec n'arrête pas les autres.

Il fonctionne comme service en arrière-plan (un service utilisateur systemd) ou à la demande. Il tourne sous Linux, sans bureau graphique.

## Démarrage rapide

Après l'[installation](#installation) :

```sh
# Ouvre le menu guidé du terminal (flèches, Entrée, Échap)
loglem

# Ou ajoutez un dossier de journaux en ligne de commande et lisez-le une fois
loglem source add-folder --name web-logs --path /srv/example/logs --days 28
loglem collect
loglem status

# Chercher
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16

# Ouvre la page web (elle affiche sa propre adresse, par exemple http://127.0.0.1:53142)
loglem web
```

Pour continuer à collecter après la fermeture du terminal :

```sh
loglem service install --start
```

Le menu du terminal et la page web utilisent pour l'instant des libellés en italien. Détails dans [Utiliser Loglem](docs/fr/usage.md).

## Installation

Quand une version sera publiée, téléchargez le paquet adapté à votre machine depuis l'onglet **Releases** de ce dépôt :

| Machine | Suffixe du paquet |
|---|---|
| Linux Intel/AMD 64 bits | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Les paquets nécessitent glibc 2.39 ou une version ultérieure (par exemple Ubuntu 24.04).

Avant d'installer, vérifiez la signature avec la clé publique de l'éditeur (`release-public.pem`), obtenue par un canal indépendant :

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

Tout se trouve dans `~/Loglem` : le programme et un dossier `data/` contenant les réglages, les archives et les journaux de diagnostic. L'installateur n'active aucun service de lui-même. Pour les sources Podman, Podman doit être installé et utilisable par le même utilisateur.

Pour le compiler vous-même, voir [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentation

La documentation est disponible en : [English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Page | Contenu |
|---|---|
| [Utiliser Loglem](docs/fr/usage.md) | Guide pas à pas : ajout de sources, recherche, conservation, exécution comme service, nettoyage, transfert vers une autre machine, dépannage, mises à jour |
| [Référence des commandes](docs/fr/cli.md) | Toutes les commandes |
| [Limites](docs/fr/limitations.md) | Ce qu'il ne fait pas ou ne garantit pas |

## Licence

[MIT](LICENSE).
