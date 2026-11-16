#!/bin/sh
set -eu

# Run from an extracted release. Never downloads or executes remote shell code.
case "$(uname -s)" in Linux) ;; *) echo 'This release supports Linux only.' >&2; exit 1 ;; esac
loglem_package=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
loglem_prefix=${LOGLEM_INSTALL_DIR:-"$HOME/Loglem"}
loglem_destination="$loglem_prefix/loglem"
loglem_temporary="$loglem_prefix/.loglem-install-$$"
if [ -e "$loglem_destination" ]; then
    echo 'An installation already exists. Use loglem update install, or stop the service before a manual replacement.' >&2
    exit 1
fi
mkdir -p "$loglem_prefix"
trap 'rm -f "$loglem_temporary"' EXIT HUP INT TERM
install -m 755 "$loglem_package/loglem" "$loglem_temporary"
"$loglem_temporary" self-check
mv "$loglem_temporary" "$loglem_destination"
printf 'Installed: %s\n' "$loglem_destination"
printf 'Open the menu: %s\n' "$loglem_destination"
printf 'Optional user service: %s service install --start\n' "$loglem_destination"

