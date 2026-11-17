# Données personnelles et mode preuve

Les journaux contiennent des données personnelles. Une adresse IP est une donnée personnelle. Loglem vous donne des outils pour respecter les règles. Il ne rend **pas** une installation conforme à lui seul : pourquoi vous gardez des journaux, combien de temps, et ce que vous dites aux personnes restent vos décisions.

## Garder moins

- **Conservation courte.** Réglez les jours par source. Les lignes expirées sont supprimées.
- **Masquer les IP.** `203.0.113.77` devient `203.0.113.0`. Tout de suite, ou après N jours :

```sh
loglem privacy set site --mask-ips-after-days 0
loglem privacy set site --keep-fields request.uri,status
```

- **Ne garder que certains champs JSON.** Tous les autres sont supprimés.
- **Voir ce que vous gardez :** `loglem privacy report`. Mettez-le dans votre registre des traitements.

Le masquage ne change que la copie sauvegardée. Le contrôle avant de supprimer un fichier d'origine fonctionne toujours. En mode preuve, le masquage n'est permis que « tout de suite » (plus tard, il changerait des lignes déjà scellées).

## Effacer les données d'une personne

Quelqu'un vous demande d'effacer ses données. Utilisez la page **Données personnelles**, ou :

```sh
loglem erase --ip 203.0.113.7                                    # aperçu seulement
loglem erase --ip 203.0.113.7 --apply --reason "demande art. 17 du 17/11/2026, dossier 3"
loglem erase --ip 203.0.113.7 --originals --apply --reason "…"   # aussi dans les fichiers d'origine
```

Loglem retire les lignes de **toutes** les sources. Le motif va dans le journal d'accès. L'IP n'y est jamais écrite.

**Fichiers d'origine** (`--originals`, ou la case de la page). Loglem réécrit les fichiers de journal terminés sans ces lignes. Seulement si :

- le fichier n'est pas celui dans lequel un programme écrit,
- personne ne l'a modifié depuis 24 heures,
- le fichier entier est déjà dans l'archive.

Il écrit un nouveau fichier à côté de l'ancien, le contrôle, puis le met à sa place. Si quelque chose cloche, l'original reste. Les fichiers ignorés sont listés avec la raison. Relancez la commande après la prochaine rotation. Les `.gz` sont réécrits aussi.

Il ne touche **pas** :

- les journaux de Podman, Docker, Kubernetes et du journal système. Ce ne sont pas des fichiers de Loglem, et le journal ne permet pas de retirer des entrées isolées. Raccourcissez leur durée, ou recréez le conteneur.
- les sauvegardes déjà faites (elles expirent avec leur propre rotation),
- le fichier dans lequel un programme écrit, tant qu'il n'a pas tourné.

Dans `--reason`, écrivez un numéro de dossier, pas une adresse e-mail : un e-mail est aussi une donnée personnelle.

## Exporter les données d'une personne

Quelqu'un vous demande ce que vous avez sur lui :

```sh
loglem export --ip 203.0.113.7 --output /endroit/sur/demande-3.txt   # --json pour du JSON
```

Seul le propriétaire peut lire le fichier. Supprimez-le après l'envoi.

## Qui a regardé

`loglem audit` (ou les fichiers de `data/audit/`) liste chaque connexion, recherche, export, effacement et changement de réglage, avec l'utilisateur et l'heure. Les valeurs cherchées ne sont **pas** écrites, seulement le type de filtre (« texte, ip »). Les entrées sont gardées 13 mois.

## Disque et sauvegardes

- Loglem ne chiffre pas ses fichiers. Utilisez un disque chiffré.
- `loglem backup` écrit des fichiers en clair. Chiffrez-les avant qu'ils quittent le serveur (par exemple avec restic).
- Ne mettez pas de copies de journaux contenant des données personnelles dans un stockage où vous ne pouvez pas supprimer (comme un bucket verrouillé).

## Mode preuve

Pour une source qui doit prouver que ses lignes n'ont pas été modifiées :

```sh
loglem proof enable site --tsa https://freetsa.org/tsr     # --tsa est facultatif
loglem proof verify site
```

Chaque jour terminé reçoit une empreinte (SHA-256) de ses lignes. Elle contient aussi l'empreinte de la veille : en changer une casse toutes les suivantes. Avec `--tsa`, une autorité extérieure signe chaque empreinte avec l'heure ; elle ne voit que l'empreinte.

`loglem proof verify` recalcule chaque jour et dit **intact**, **ALTÉRÉ** ou **expiré**. Vous pouvez aussi contrôler une copie : `loglem proof verify --archive copie.db --chain copie.chain`.

Ce que cela prouve : un jour scellé n'a pas changé après le scellé (ou après l'heure du cachet). Cela ne prouve pas que les lignes étaient vraies quand elles ont été écrites.

Les lignes n'expirent que par jours entiers, et chaque expiration est écrite dans le registre.

**Effacement en mode preuve.** L'effacement l'emporte. Loglem retire les lignes, **recalcule** l'empreinte de ce jour et de tous les jours suivants, et ajoute une note au registre : quand, pourquoi, combien de lignes, ancienne empreinte → nouvelle. `proof verify` montre la note. Les anciens horodatages restent valables pour l'ancienne version. De nouveaux sont demandés pour les nouvelles empreintes.
