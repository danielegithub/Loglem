# Limites

Ce que Loglem ne fait pas, ou ne promet pas.

## Plateformes

- Linux et macOS. Le service est systemd (Linux) ou launchd (macOS). Mises à jour signées : Linux seulement.
- Windows : le code est écrit pour être portable, mais n'a été ni compilé ni testé.
- Aucune version n'est encore publiée.
- Dans un conteneur, Loglem lit seulement les fichiers de journal que vous montez. Podman, Docker, Kubernetes et le journal de l'hôte demandent Loglem sur l'hôte.

## Ce qu'il lit

- Lignes de texte UTF-8 jusqu'à 1 Mio. Les autres encodages ne sont pas lus.
- Les filtres de gravité, de code et de chemin lisent le texte de la ligne. Ils connaissent Caddy (JSON), Nginx et Apache (combined) et le journal. Les autres formats ne se trouvent qu'en cherchant le texte.
- Les statistiques marchent avec les journaux Caddy en JSON et Nginx/Apache au format combined.
- Pas encore d'expressions régulières.
- Les statistiques gardent en mémoire chaque IP et chaque page différentes de la période. Avec des journaux de millions d'IP différentes, il faut plus de RAM (environ 56 octets par IP ou page différente).

## Ce qui peut être perdu

- Podman, Docker et Kubernetes gardent les journaux seulement tant que le conteneur ou le pod existe. S'il est retiré pendant que Loglem est éteint, ces lignes sont perdues. Loglem affiche un **trou** sur la fiche.
- Pour Kubernetes, suivez l'application par étiquette (`app=web`). Un pod seul disparaît à chaque déploiement.
- Si Loglem est éteint, les lignes n'expirent pas et rien n'est nettoyé. Voir [stopping.md](stopping.md).
- Disque plein : les fichiers pas encore archivés sont gardés. Loglem ne peut pas libérer de place.

## Sécurité des données

- Les archives sont des fichiers SQLite sur un disque local, pas sur un partage réseau. Arrêtez Loglem avant de les copier à la main, ou utilisez `loglem backup`.
- Supprimer une ligne dans Loglem ne la supprime pas des sauvegardes ni des fichiers d'origine.
- Loglem ne chiffre pas les données sur le disque. Utilisez un disque chiffré et des sauvegardes chiffrées.
- La liaison vers la page web n'est pas chiffrée. Utilisez un VPN ou un reverse proxy HTTPS.
- Loglem ne peut pas savoir si un programme a fini avec un fichier. Le nettoyage attend 24 heures et ne touche jamais le fichier en cours d'usage.
- Le mode preuve montre qu'un jour scellé n'a pas changé. Il ne prouve pas que les lignes étaient vraies. Voir [privacy.md](privacy.md).

## Ce qui a été testé

Testé sur macOS avec Podman, journald (dans un conteneur Linux), Caddy, Chrome et une vraie autorité d'horodatage : ajout de sources depuis la page, recherche, direct, connexion avec mot de passe, redémarrage sans ligne perdue ni dupliquée, expiration, rotation de Caddy, reverse proxy, sauvegarde et restauration, mode preuve, effacement.

Non testé : le service systemd sur un vrai Linux, un vrai cluster Kubernetes (un faux `kubectl` a été utilisé), l'agent launchd de macOS, Windows et une mise à jour depuis une version publiée.
