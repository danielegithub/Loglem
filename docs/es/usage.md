# Usar Loglem

Pocos pasos. Todos los comandos están en [cli.md](cli.md).

## 1. Crea un sitio y conecta sus registros

En la página inicial elige **Nuovo sito** y escribe el nombre del sitio o proyecto. Pulsa **Aggiungi un log** y elige el archivo, carpeta o servicio que quieres leer. La página del sitio muestra el origen, el archivo de Loglem, el plazo de conservación y las últimas líneas. **Cerca nei log del sito** limita la búsqueda a ese sitio.

| Fuente | Comando |
|---|---|
| Carpeta de archivos `.log` | `loglem source add-folder --name web --path /var/log/myapp` |
| Un archivo (y sus copias rotadas) | `loglem source add-file --name api --path /var/log/api.log --rotated-pattern 'api.log.*'` |
| Cualquier registro, guiado paso a paso (recomendado) | `loglem add` |
| Contenedor Podman | `loglem source add-podman --name db --container db` |
| Contenedor Docker | `loglem source add-docker --name db --container db` |
| Pods de Kubernetes | `loglem source add-kubernetes --name shop --namespace shop --selector app=web` |
| Journal de Linux | `loglem source add-journal --name ssh --identifier sshd` |

`loglem discover` busca los registros que los programas conocidos indican en su configuración (ver `src/recipes/`). La página **Añadir** lo hace por ti y explica por qué propone cada registro.

Los archivos comprimidos `.gz` también se leen. Loglem importa el historial una sola vez y nunca duplica una línea.

Puedes elegir el sitio o proyecto al añadir una fuente. Añade `--site example.com` a cualquier comando `source add-*`. Para trasladar una fuente existente usa `loglem source assign-site myapp example.com`. La página inicial agrupa los registros por sitio. Las fuentes del journal sin sitio asignado empiezan en el grupo del sistema. Durante la migración, Loglem guarda la configuración anterior en `config.json.bak-AAAAMMDD`.

Abre un sitio y despliega **Copie** para ver rutas, tamaños y quién se encarga de la limpieza. Un fichero compartido cuenta una sola vez en el total del sitio. Loglem compara hasta 200 líneas recientes por fuente. Si coincide más de la mitad de la muestra mayor, puedes pausar una de las dos fuentes. La búsqueda externa revisa a petición las carpetas de registros, `/var/log`, `/srv`, el journal local, Podman y Docker durante un máximo de ocho segundos. El total de bytes idénticos solo incluye ficheros distintos verificados por completo.

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

El servicio también hace la comprobación automática (cada 5 minutos, sin cron). La página Sitios y **Sistema → Servicio** muestran si está en marcha, desde cuándo y cuántas veces se ha reiniciado. Para quitarlo: `loglem service uninstall`. Los datos se quedan.

## 5. Deja que Loglem borre los archivos originales

Desactivado por defecto. Loglem nunca toca el archivo en el que un programa está escribiendo. Borra solo archivos terminados, tras esta comprobación:

1. Loglem copia el archivo en su archivo.
2. Compara la copia con el original, byte a byte. Una línea que falta o caducada detiene la comprobación.
3. Espera a que nadie haya modificado el archivo durante **24 horas**.
4. Solo entonces borra el original.

Si un paso falla, el original se queda.

**Archivos rotados.** Las fuentes nuevas conservan los originales. Para borrar archivos rotados después de verificarlos, activa la limpieza en los ajustes de la fuente.

**Otros programas.** Haz que el programa deje los archivos *terminados* en una carpeta aparte y luego:

```sh
loglem source configure web --completed-dir /var/log/done --handoff-confirmed
```

Nunca indiques una carpeta en la que un programa todavía escribe.

**¿Quién borra los originales?** Un solo sitio: el cuadro de limpieza en los ajustes de la fuente. Loglem lee las reglas de logrotate y dice si ya existe una rotación. Si logrotate (o el programa) ya rota y borra los archivos, deja la limpieza de Loglem **apagada**: dos sistemas que borran los mismos archivos no sirven y esconden los errores. Loglem avisa si están los dos activos, si los originales duran más que su copia y si los archivos rotados tienen nombres que no lee.

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

### Estado y avisos por correo

Loglem se comprueba solo cada 5 minutos (cámbialo en **Sistema → Comprobación automática**, o con `loglem alerts monitor --every 10`). La página muestra «Todo en orden, comprobado a las 14:35» o lo que falla.

Los avisos por correo son opcionales y usan [Resend](https://resend.com):

```sh
loglem alerts email-set --from noreply@example.com --to admin@example.com   # pide la clave
loglem alerts email-test
loglem alerts email-off            # borra también la clave
```

Crea una clave **solo de envío** limitada a tu dominio y verifica el dominio en Resend. El remitente puede ser `noreply@` en un subdominio. Las respuestas no se leen. Loglem envía un correo cuando se abre un problema, otro cuando se cierra y un recordatorio si sigue abierto más de 24 horas. Los correos no contienen líneas de registro. Como máximo 6 por hora. La clave se guarda en un archivo privado (`secrets/resend.key`): nunca en los ajustes, las exportaciones, las copias de seguridad ni los registros.

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
