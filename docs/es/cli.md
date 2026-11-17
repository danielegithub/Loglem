# Comandos

`loglem COMANDO --help` muestra todas las opciones. Sin comando, Loglem abre el menú del terminal. Pon `--data-dir /ruta/absoluta` antes del comando para usar otra carpeta de datos (o define `LOGLEM_DATA_DIR`).

## Fuentes y lectura

| Comando | Qué hace |
|---|---|
| `source add-folder / add-file / add-caddy / add-podman / add-docker / add-kubernetes / add-journal` | Añade una fuente. |
| `source list`, `source set NOMBRE --days N` | Lista las fuentes, cambia la duración. |
| `source configure NOMBRE …` | Nombre, carpeta de archivos terminados, limpieza de rotados, ruta. |
| `discover` | Busca los registros de Caddy, Nginx y Apache. |
| `podman-list`, `docker-list`, `kubernetes-list` | Lista contenedores o pods. |
| `collect` | Lee una vez. |
| `serve` / `web` | Lee siempre y sirve la página web. |
| `status [--json]` | Ubicaciones, tamaños, fechas, estado, huecos. |
| `check [--minutes N]` | Código de salida 1 si Loglem está apagado, una fuente no se lee o salta una alerta. |

## Busca y sigue

| Comando | Qué hace |
|---|---|
| `search` | `--text --ignore-case --level error --ip --path --status --from --to --source --limit --json` |
| `tail` | Mismos filtros, en directo. `--server URL --user NOMBRE` sigue un Loglem remoto. |
| `stats FUENTE [--days N] [--json]` | Peticiones por día, páginas, códigos, IP, páginas lentas. |

## Cuida los datos

| Comando | Qué hace |
|---|---|
| `alerts list / add-pattern / add-size / add-free-space / remove / command / seen` | Reglas de alerta y notificación. |
| `backup DIR`, `backup-verify DIR`, `restore DIR [--replace]` | Copia, comprueba y restaura los datos. |
| `proof enable / disable / verify` | Modo prueba: sella cada día. |
| `privacy set FUENTE …`, `privacy report` | Enmascara IP, conserva campos JSON, lista lo que se guarda. |
| `erase … [--originals]`, `export …`, `audit [--days N]` | RGPD: borra (también de los archivos originales) o exporta las líneas de una persona, ve quién miró. También en la página **Datos personales**. |
| `prune [--apply]` | Muestra o borra las líneas caducadas. |
| `archive-completed` | Archiva y comprueba a mano un archivo terminado. |

## Ejecuta y mantén

| Comando | Qué hace |
|---|---|
| `service install [--start] / start / stop / status` | Servicio: systemd en Linux, launchd en macOS. |
| `access user-add / user-remove / user-list / address / proxy` | Otros ordenadores: usuarios, dirección, proxy inverso. |
| `config export / import / poll / web-port` | Ajustes. |
| `update check / install / status / configure` | Actualizaciones firmadas (Linux). |
| `diagnostics`, `self-check`, `init` | Errores de Loglem, una prueba rápida, una configuración vacía. |
