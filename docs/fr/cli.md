# Référence des commandes

Lancez `loglem --help` ou `loglem COMMANDE --help` pour voir les options de la version installée. Sans commande, Loglem ouvre le menu au clavier dans le terminal ; `loglem tui` fait la même chose explicitement.

| Commande | À quoi ça sert |
|---|---|
| `init` | Crée une configuration vide. |
| `source list` | Liste les sources. Pour en ajouter une : `source add-folder`, `add-file`, `add-podman` ou `add-caddy` (journal de Caddy avec nettoyage des fichiers issus de la rotation). |
| `podman-list` | Trouve les conteneurs et pods ; `--connection` choisit une connexion nommée. |
| `status [--json]` | Montre l'emplacement des archives, les tailles, les dates et l'état de la collecte. |
| `collect` / `serve` | Collecte une fois, ou fait tourner le service (collecte plus page web à adresse fixe) jusqu'à l'arrêt. `web` fait la même chose, ou affiche l'adresse si le service est déjà actif. |
| `search [...]` | Cherche du texte et des plages de dates dans les archives ; `--json` donne une sortie exploitable par d'autres programmes. |
| `prune [--apply]` | Montre les entrées expirées, ou avec `--apply` les supprime des archives. |
| `config export FILE` / `config import FILE` | Exporte ou importe la configuration des sources. |
| `config poll N` | Règle l'intervalle de collecte automatique (en secondes). |
| `config web-port N` | Choisit le port local fixe de la page web (au prochain démarrage). |
| `service install [--start]` | Enregistre un service utilisateur systemd ; `start`, `stop` et `status` le gèrent. |
| `diagnostics` | Affiche les erreurs de Loglem enregistrées, avec leurs traces de pile d'appels. |
| `self-check` | Vérifie l'exécutable et le schéma SQLite intégré sans toucher aux données. |

Passez `--data-dir /chemin/absolu` avant la commande pour choisir le dossier de données ; `LOGLEM_DATA_DIR` fait la même chose pour l'automatisation. Les commandes pour gérer les sources et les mises à jour sont décrites dans [usage.md](usage.md).
