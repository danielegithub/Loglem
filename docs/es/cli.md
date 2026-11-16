# Referencia de comandos

Ejecuta `loglem --help` o `loglem COMANDO --help` para ver las opciones de la versión instalada. Sin comando, Loglem abre el menú de teclado en el terminal; `loglem tui` hace lo mismo de forma explícita.

| Comando | Para qué sirve |
|---|---|
| `init` | Crea una configuración vacía. |
| `source list` | Lista las fuentes. Para añadir una: `source add-folder`, `add-file`, `add-podman` o `add-caddy` (registro de Caddy con limpieza de archivos rotados). |
| `podman-list` | Encuentra contenedores y pods; `--connection` elige una conexión con nombre. |
| `status [--json]` | Muestra la ubicación de los archivos de registros, sus tamaños, fechas y estado de la recopilación. |
| `collect` / `serve` | Recoge una vez, o ejecuta el servicio (recogida más página web en una dirección fija) hasta que se detenga. `web` hace lo mismo, o muestra la dirección si el servicio ya está en marcha. |
| `search [...]` | Busca texto e intervalos de fechas en los archivos de registros; `--json` da una salida legible por programas. |
| `prune [--apply]` | Muestra las entradas caducadas, o con `--apply` las borra de los archivos de registros. |
| `config export FILE` / `config import FILE` | Exporta o importa la configuración de las fuentes. |
| `config poll N` | Fija cada cuánto se recoge automáticamente (en segundos). |
| `config web-port N` | Elige el puerto local fijo de la página web (desde el próximo inicio). |
| `service install [--start]` | Registra un servicio de usuario de systemd; `start`, `stop` y `status` lo gestionan. |
| `diagnostics` | Muestra los errores de Loglem guardados, con sus trazas de pila. |
| `self-check` | Comprueba el ejecutable y el esquema SQLite incluido sin tocar los datos. |

Pasa `--data-dir /ruta/absoluta` antes del comando para elegir la carpeta de datos; `LOGLEM_DATA_DIR` hace lo mismo en automatizaciones. Los comandos para gestionar fuentes y actualizaciones se describen en [usage.md](usage.md).
