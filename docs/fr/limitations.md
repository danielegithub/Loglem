# Limites

Ce que Loglem ne fait pas, ou ne garantit pas, dans cette version.

**Ce qui est pris en charge**

- Linux uniquement. Windows n'est pas pris en charge ; sur macOS il n'y a pas de service automatique (utilisez `loglem web`).
- Sources : fichiers texte `.log` (UTF-8), conteneurs Podman et journaux d'accès de Caddy. Les fichiers compressés (`.gz`) ne sont pas lus. Docker, Kubernetes, Nginx et le journal systemd ne sont pas encore pris en charge.
- Lignes jusqu'à 1 Mio.
- La recherche porte sur du texte exact, avec distinction majuscules/minuscules, et sur une plage de dates. Il n'y a pas encore de filtres par gravité, adresse IP, chemin ou code de réponse.
- La page web ne s'ouvre que depuis la machine où tourne Loglem (utilisez un tunnel SSH pour un serveur distant).

**Sûreté des données**

- Les journaux de Podman sont lus par instantanés. Si un conteneur est supprimé avant que Loglem le lise, ou si Loglem reste arrêté longtemps, des lignes peuvent être perdues.
- Les entrées n'expirent que pendant que Loglem tourne (ou quand vous lancez `loglem prune --apply`).
- Supprimer une entrée de l'archive de Loglem ne l'efface pas des instantanés ou sauvegardes du système de fichiers qui l'ont déjà copiée.
- Les archives sont des fichiers SQLite et doivent être sur un disque local, pas sur un partage réseau. Ne les copiez pas pendant que Loglem tourne ; arrêtez-le d'abord.
- Si le disque est plein, les originaux pas encore archivés sont conservés ; Loglem ne peut ni libérer de l'espace ni empêcher d'autres programmes d'écrire.
- Loglem ne peut pas savoir seul qu'un programme a fini avec un fichier. Son nettoyage attend 24 heures et ne touche jamais le fichier en cours d'écriture, mais ne l'activez que pour des dossiers et journaux Caddy configurés comme décrit dans le guide.
- Les archives ne sont pas protégées contre les modifications : Loglem ne garantit pas la valeur probante des journaux.

**Testé et pas encore testé**

- Testé avec un vrai Podman et un vrai navigateur : ajout de sources depuis la page, recherche, redémarrage sans lignes perdues ni en double, expiration et rotation simulée à la manière de Caddy.
- Pas encore testés : le service systemd sur une machine Linux réelle, Caddy en conditions réelles, et le retour à la version précédente après une mise à jour échouée.
- Il n'y a pas encore de version publiée.
