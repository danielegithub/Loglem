# Usar Loglem

Pocos pasos. Todos los comandos están en [cli.md](cli.md).

## 1. Añade una fuente

Una **fuente** es el lugar de donde vienen los registros. Añádela desde la página web (**Añadir fuente**), el menú del terminal o la línea de comandos.

| Fuente | Comando |
|---|---|
| Carpeta de archivos `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Un archivo (y sus copias rotadas) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Registro de acceso de Caddy | `loglem source add-caddy --name site --path /var/log/caddy/access.log` |
| Contenedor Podman | `loglem source add-podman --name db --container db` |
| Contenedor Docker | `loglem source add-docker --name db --container db` |
| Pods de Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal de Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` busca los registros de Caddy, Nginx y Apache en este servidor. La página web los ofrece con un clic.

Los archivos comprimidos `.gz` también se leen. Loglem importa el historial una sola vez y nunca duplica una línea.

## 2. Busca y sigue en directo

Abre **Explorar registros**. Escribe un texto, elige fechas, o abre **Filtros avanzados**: gravedad, dirección IP, ruta, código de respuesta.

Las líneas nuevas aparecen arriba solas. En un terminal:

```sh
loglem search --text login --ignore-case --status 404 --source site
loglem tail --source site --ip 203.0.113.7
```

Para las estadísticas de un registro de acceso (peticiones por día, páginas más vistas, códigos, IP más frecuentes): `loglem stats site`, o el botón **Estadísticas** de la ficha.

## 3. Elige cuánto tiempo se guardan

Cada fuente tiene una duración en días, meses o años. Las copias viejas se borran solas. Los archivos originales no se tocan.

```sh
loglem source set site --days 28
```

## 4. Mantén Loglem en marcha

`loglem serve` lee los registros y sirve la página web. La página mantiene la misma dirección, también tras un reinicio.

```sh
loglem service install --start    # Linux: systemd. macOS: launchd.
loglem service status
loglem check                      # código de salida 1 si algo va mal
```

El servicio se reinicia solo tras un fallo. En una parada normal lee cada fuente una última vez. Qué pasa mientras Loglem está apagado: [stopping.md](stopping.md).

## 5. Deja que Loglem borre los archivos originales

Desactivado por defecto. Loglem nunca toca el archivo en el que un programa está escribiendo. Borra solo archivos terminados, tras esta comprobación:

1. Loglem copia el archivo en su archivo.
2. Compara la copia con el original, byte a byte. Una línea que falta o caducada detiene la comprobación.
3. Espera a que nadie haya modificado el archivo durante **24 horas**.
4. Solo entonces borra el original.

Si un paso falla, el original se queda.

**Caddy.** Añade la fuente con `add-caddy`. Loglem lee los archivos rotados (también `.gz`) y los borra tras la comprobación. Usa el bloque `log` que muestra la página. `roll_keep_for 60d` es una red de seguridad.

**Otros programas.** Haz que el programa deje los archivos *terminados* en una carpeta aparte y luego:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Nunca indiques una carpeta en la que un programa todavía escribe.

## 6. Abre la página desde otro ordenador

Desactivado por defecto: la página responde solo en este ordenador. Para abrirla en otro sitio:

```sh
loglem access user-add anna --role admin     # reader = solo puede mirar
loglem access address 10.0.0.1               # una dirección de este servidor, por ejemplo la de la VPN
```

Reinicia Loglem. Cinco contraseñas incorrectas bloquean ese ordenador durante 15 minutos. La conexión **no está cifrada**: usa una VPN, o un proxy inverso con HTTPS:

```sh
loglem access proxy --trust 127.0.0.1 --base-path /loglem
```

Muestra un bloque para el Caddyfile. Para seguir en directo desde otro ordenador: `loglem tail --server http://10.0.0.1:8790 --user anna`.

## 7. Alertas y copia de seguridad

```sh
loglem alerts add-pattern accesos-fallidos --text "Failed password" --count 5 --minutes 10
loglem alerts command 'echo "$LOGLEM_ALERTS" | mail -s Loglem me@example.com'
loglem backup /backups/loglem-2026-11-17
loglem backup-verify /backups/loglem-2026-11-17
```

Las alertas aparecen en la página, en `loglem check` y en `data/health.json`. **Marcar como visto** oculta una hasta que vuelva. Una copia de seguridad también guarda las líneas que han caducado desde entonces.

## 8. Dónde están tus datos

```text
data/
├── config.json     fuentes y ajustes
├── archives/       registros guardados, una base por fuente
├── audit/          quién buscó, exportó o borró
├── proof/          registros de sellos (modo prueba)
└── diagnostics/    los errores de Loglem
```

Para otro lugar usa `--data-dir /ruta/absoluta` o `LOGLEM_DATA_DIR`. Mueve los ajustes con `loglem config export` y `config import`. Mueve los registros deteniendo Loglem y copiando `data/`.

Los archivos de versiones antiguas se convierten al arrancar. El texto guardado sigue igual, el archivo pesa menos.

## 9. Si algo va mal

- Una fuente muestra un error: las demás siguen. Abre su ficha, o `loglem status`.
- Los errores de Loglem: **Problemas** en la página, o `loglem diagnostics`.
- Puerto ocupado: `loglem config web-port 8790`, y reinicia.
- Comprueba el programa: `loglem self-check`.

## 10. Actualiza

Pulsa **Comprobar** en la página **Sistema**, o:

```sh
loglem update check
loglem update install
```

Solo se instalan versiones firmadas (Linux). `data/` se conserva. El programa viejo queda como `loglem.previous`.

## 11. En un contenedor

```sh
podman build -t loglem -f Containerfile .
podman run -d --name loglem -p 127.0.0.1:8790:8790 \
  -e LOGLEM_ADMIN_USER=anna -e LOGLEM_ADMIN_PASSWORD='una contraseña larga' \
  -v loglem-data:/data -v /var/log/myapp:/logs/myapp:ro loglem
```

Abre `http://127.0.0.1:8790` y añade `/logs/myapp` como fuente carpeta. Los datos quedan en el volumen `loglem-data`. Podman, Docker y el journal del anfitrión no se alcanzan desde dentro: para ellos, usa Loglem en el anfitrión, o monta los archivos de registro en `/logs`. Para limpiar los archivos originales, monta la carpeta sin `:ro`.

Datos personales, herramientas RGPD y modo prueba: [privacy.md](privacy.md).
