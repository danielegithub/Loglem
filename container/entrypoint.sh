#!/bin/sh
# Avvio di Loglem in un container: porta fissa, utente amministratore, pagina raggiungibile da fuori.
set -eu

port="${LOGLEM_PORT:-8790}"
loglem config web-port "$port"

# La password si può dare in un file (consigliato, es. un «secret» di Podman) o in una variabile.
if [ -n "${LOGLEM_ADMIN_PASSWORD_FILE:-}" ]; then
    LOGLEM_ADMIN_PASSWORD="$(cat "$LOGLEM_ADMIN_PASSWORD_FILE")"
fi
if [ -n "${LOGLEM_ADMIN_USER:-}" ] && [ -n "${LOGLEM_ADMIN_PASSWORD:-}" ]; then
    LOGLEM_PASSWORD="$LOGLEM_ADMIN_PASSWORD" loglem access user-add "$LOGLEM_ADMIN_USER" --role admin
fi

# Senza utenti la pagina risponde solo dentro il container: inutile. Meglio fermarsi e dirlo.
if loglem access user-list | grep -q "Nessun utente"; then
    echo "Manca l'utente: imposta LOGLEM_ADMIN_USER e LOGLEM_ADMIN_PASSWORD (o LOGLEM_ADMIN_PASSWORD_FILE)." >&2
    exit 1
fi

loglem access address 0.0.0.0
exec loglem serve
