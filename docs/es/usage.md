# Usar Loglem

Una guía paso a paso. Para la lista de comandos, consulta [cli.md](cli.md); para lo que Loglem no hace, consulta [limitations.md](limitations.md).

## Contenido

- [1. Añade tu primera fuente](#1-añade-tu-primera-fuente)
- [2. Busca en los registros](#2-busca-en-los-registros)
- [3. Elige cuánto tiempo se guardan los registros](#3-elige-cuánto-tiempo-se-guardan-los-registros)
- [4. Mantén Loglem siempre en marcha](#4-mantén-loglem-siempre-en-marcha)
- [5. Deja que Loglem limpie los archivos originales](#5-deja-que-loglem-limpie-los-archivos-originales)
- [6. Cambia, pausa o quita una fuente](#6-cambia-pausa-o-quita-una-fuente)
- [7. Dónde están tus datos y cómo moverlos](#7-dónde-están-tus-datos-y-cómo-moverlos)
- [8. Cuando algo no va bien](#8-cuando-algo-no-va-bien)
- [9. Actualiza Loglem](#9-actualiza-loglem)

## 1. Añade tu primera fuente

Una **fuente** es el lugar de donde vienen los registros: una carpeta de archivos `.log`, un archivo suelto, un contenedor Podman o el registro de acceso de Caddy. Puedes añadirla desde la página web, desde el menú del terminal o desde la línea de comandos.

### Desde la página web

```sh
loglem web
```

Loglem muestra la dirección de la página (por ejemplo `http://127.0.0.1:53142`) y la carpeta donde guarda sus datos. Abre esa dirección en un navegador. La dirección sigue siendo la misma tras reiniciar. Si el puerto lo usa otro programa, Loglem se detiene y lo dice; elige otro con `loglem config web-port 8790`. Loglem nunca usa el puerto 80 ni el 443.

La página solo funciona en la máquina donde se ejecuta Loglem. En un servidor remoto, ábrela con un túnel SSH (`ssh -L 53142:127.0.0.1:53142 usuario@servidor`, y luego ve a `http://127.0.0.1:53142`).

El menú de la izquierda tiene cuatro páginas:

- **Sorgenti:** tus fuentes, con su estado, el número de líneas y enlaces a sus registros y ajustes.
- **Esplora i log:** busca y lee los registros de una fuente. La lista se actualiza sola; una casilla la pone en pausa.
- **Aggiungi sorgente:** un formulario guiado. Elige el tipo, la carpeta o el contenedor, un nombre, qué clase de registro es y cuánto tiempo guardarlo, y pulsa **Salva e apri i log**. Loglem lee los registros enseguida.
- **Problemi:** los errores del propio Loglem, con su causa (consulta la [sección 8](#8-cuando-algo-no-va-bien)).

### Desde la línea de comandos

```sh
# una carpeta de archivos .log (los archivos nuevos se detectan solos)
loglem source add-folder --name web-logs --path /srv/example/logs --days 28

# un archivo suelto; también lee sus copias renombradas, como api.log-1
loglem source add-file --name api --path /srv/example/logs/api.log \
  --rotated-pattern 'api.log-*' --days 14

# un contenedor Podman
loglem podman-list
loglem source add-podman --name app --container my-container --days 14

# leer ahora y comprobar
loglem collect
loglem status
```

Notas:

- Una fuente de carpeta lee los archivos `.log` que están directamente en la carpeta; se ignoran subcarpetas, archivos comprimidos y otras extensiones.
- Un archivo debe tener un nombre que termine en `.log`. Los archivos comprimidos (`.gz`) no se leen.
- Con Podman, Loglem recuerda el ID completo del contenedor. Si vuelves a crear un contenedor con el mismo nombre, añádelo como fuente nueva (o consulta la [sección 6](#6-cambia-pausa-o-quita-una-fuente)). El historial ya recogido sigue disponible después de eliminar el contenedor. Con `--connection NOMBRE` eliges una conexión Podman con nombre.
- Loglem solo lee tus archivos originales. No los modifica, salvo que actives la limpieza (sección 5).
- Los nombres y botones de la página y del menú están por ahora en italiano.

### Desde el menú del terminal

Ejecuta `loglem`. Usa las flechas (o `j`/`k`), Intro para elegir y Esc para volver.

```text
> Le mie sorgenti — posizione, spazio e log salvati
  Aggiungi cartella, file o container
  Cerca nei log
  Conservazione — per quanti giorni tenere i log
  Leggi i log adesso
  Raccolta automatica (servizio Linux)
  Impostazioni
  Aggiornamenti
  Esci
```

Elige **Aggiungi cartella, file o container**, luego el tipo y navega hasta la carpeta (o elige **Scrivi / incolla un percorso…** para escribirla). Revisa el resumen y guarda con **Salva e leggi i log adesso**. Esc cancela antes de guardar.

## 2. Busca en los registros

En la página web abre **Esplora i log**, elige la fuente y escribe tu texto. Puedes limitar por fecha (UTC). Las líneas más recientes salen primero. Desde la línea de comandos:

```sh
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16
```

- La búsqueda es de texto exacto y distingue mayúsculas de minúsculas; no es una expresión regular.
- `--to 2026-11-16` incluye todo ese día (UTC).
- Añade `--json` para una salida legible por otros programas, y `--limit` para obtener hasta 1000 resultados.
- Si una línea no tiene una fecha que Loglem reconozca, sigue siendo buscable y se muestra con la hora en que se leyó. Loglem reconoce los registros JSON habituales (`ts`, `timestamp`, `time`, `@timestamp`).

## 3. Elige cuánto tiempo se guardan los registros

Cada fuente tiene su propio periodo de conservación: un número de días, meses o años. Cuando una entrada supera ese periodo, Loglem la borra del archivo de registros.

```sh
loglem source set web-logs --days 7
loglem source configure web-logs --amount 6 --unit months --log-kind access
loglem source configure web-logs --amount 2 --unit years
```

- En la página web y en el menú del terminal (**Conservazione**), el mismo ajuste es un formulario que también muestra cuándo caducarán las entradas.
- Las entradas caducadas se eliminan mientras Loglem está en marcha (consulta la sección 4). `loglem prune` muestra lo que se borraría; `loglem prune --apply` lo borra ya.
- La conservación cuenta desde la fecha de la línea o desde que Loglem la leyó por primera vez, la que sea anterior: una línea con fecha futura no se queda para siempre.
- Pausar una fuente detiene las lecturas nuevas, no la conservación.
- La clase de registro (acceso, general, errores) solo describe la fuente; no fija por ti un plazo legal.
- La duración se aplica a la copia de Loglem. Tus archivos originales no se borran salvo que actives la limpieza (sección 5).

## 4. Mantén Loglem siempre en marcha

`loglem web` solo recoge registros mientras se ejecuta. Para recogerlos siempre, aunque cierres el terminal o reinicies, instálalo como servicio:

```sh
loglem service install --start
loglem service status
```

Esto crea un **servicio de usuario de systemd** para el usuario actual. Lee las líneas nuevas cada 10 segundos (se cambia con `loglem config poll 5`), elimina las entradas caducadas y sirve la página web en la misma dirección.

- En un servidor, pide a un administrador que ejecute una vez `sudo loginctl enable-linger USUARIO`, para que el servicio funcione aunque el usuario no haya iniciado sesión.
- El servicio solo tiene los permisos de ese usuario. Comprueba que pueda leer las carpetas de registros; Loglem no se ejecuta como root.
- Solo se ejecuta un Loglem por carpeta de datos. Si el servicio ya está en marcha, `loglem web` no inicia otro: muestra la dirección del servicio y termina.
- Iniciar Loglem con otro `--data-dir` muestra fuentes distintas. La carpeta de datos en uso se indica siempre en la página web y en el menú del terminal.

## 5. Deja que Loglem limpie los archivos originales

Por defecto, Loglem no borra tus archivos originales. Puedes dejar que borre archivos originales **después de copiarlos y verificarlos**. Solo lo hace con archivos que nadie ha modificado en al menos 24 horas, nunca con el archivo en el que un programa está escribiendo, y conserva cualquier archivo que no pueda verificar (la ficha de la fuente muestra un aviso). La caducidad de las entradas archivadas es un paso aparte, que se ajusta en la sección 3.

### Caddy

En **Aggiungi sorgente** elige **Il log di accesso di Caddy**, abre la carpeta de registros (a menudo `/var/log/caddy`) y elige el archivo en el que escribe Caddy, normalmente `access.log`. Entonces Loglem lee también los archivos que Caddy crea al rotar el registro (por ejemplo `access-2026-11-16T10-00-00.000.log`) y los borra tras verificarlos. Al final, la página muestra el bloque que debes poner en tu Caddyfile:

```caddyfile
log {
	output file /var/log/caddy/access.log {
		roll_size 50MiB
		roll_uncompressed
		roll_keep 100
		roll_keep_for 60d
	}
}
```

- `roll_uncompressed` es obligatorio: Loglem solo verifica archivos sin comprimir. Si encuentra archivos `.gz`, los señala en la ficha.
- `roll_keep_for 60d` es una red de seguridad: si Loglem queda apagado mucho tiempo, Caddy borra igualmente los archivos más antiguos.
- Loglem no modifica tu Caddyfile; el bloque lo pegas tú.

Desde la línea de comandos: `loglem source add-caddy --name site --path /var/log/caddy/access.log --days 90`. Para una fuente de archivo que ya tiene un patrón de rotación: `loglem source configure site --remove-rotated true`.

### Otros programas

Pide al programa que mueva cada archivo terminado, cerrado y sin comprimir, a una carpeta dedicada. En los ajustes de la fuente activa **Archivia ed elimina i file di una cartella dedicata** y confirma. Nunca selecciones una carpeta en la que un programa siga escribiendo.

```sh
loglem source configure web --completed-dir /absolute/completed-files --handoff-confirmed
# para desactivarlo de nuevo
loglem source configure web --completed-dir ""
```

Para un solo archivo terminado, sin esperar 24 horas:

```sh
loglem archive-completed --source web --file /srv/example/logs/access-old.log --producer-closed --remove
```

Sin `--remove`, el comando archiva y verifica el archivo sin borrar el original. Loglem nunca borra los originales de un contenedor Podman.

## 6. Cambia, pausa o quita una fuente

En la página web, pulsa **Impostazioni** en la ficha de la fuente. Desde la línea de comandos:

```sh
loglem source rename web "Sitio público"
loglem source pause web          # detiene la lectura, el historial sigue siendo buscable
loglem source resume web
loglem source detach web         # desconecta; el archivo de registros sigue siendo consultable
loglem source configure web --container NEW_CONTAINER   # apunta a un contenedor que lo sustituye
loglem source configure web --path /new/path            # nuevo archivo o carpeta
```

- Cambiar el nombre solo cambia la etiqueta; el historial se conserva.
- Para guardar el archivo de registros de una fuente en otra carpeta: `loglem source configure web --archive-dir /carpeta/absoluta`. Loglem lo copia, comprueba la copia y luego elimina los archivos antiguos; nunca sobrescribe una base de datos existente. Con `--archive-dir ""` vuelves a la carpeta predeterminada. Cierra antes otros programas que tengan abierta la base de datos.

## 7. Dónde están tus datos y cómo moverlos

Por defecto, todo está en la carpeta de Loglem:

```text
Loglem/
├── loglem          el programa
└── data/
    ├── config.json   tus fuentes y ajustes
    ├── archives/     los registros guardados (una base de datos por fuente)
    ├── diagnostics/  el registro de errores de Loglem
    └── updates/
```

Para usar otra ubicación, pasa `--data-dir /ruta/absoluta` antes del comando, o define `LOGLEM_DATA_DIR`.

Para pasar a otra máquina:

```sh
loglem config export ./loglem-config.json     # en la máquina antigua
loglem config import ./loglem-config.json     # en la máquina nueva
```

Esto mueve tus **ajustes** (fuentes, duraciones), no los registros. Ajusta rutas y permisos en la máquina nueva. Para mover también los registros, detén Loglem y copia toda la carpeta `data/`. Importar una configuración nunca quita la regla de conservación de una fuente que ya tiene registros archivados; en ese caso, pausa esa fuente.

## 8. Cuando algo no va bien

- **Una fuente muestra un error:** las demás siguen funcionando y las líneas ya guardadas siguen siendo legibles. Abre la ficha de la fuente para ver el mensaje, o ejecuta `loglem status`.
- **La página está vacía o una fuente dice «no se puede leer»:** comprueba que el usuario que ejecuta Loglem pueda leer la carpeta o el archivo.
- **Ver los errores de Loglem:** **Problemi** en la página web, o `loglem diagnostics`. Cada entrada indica la hora, qué estaba haciendo Loglem, la causa y los detalles técnicos. El archivo es `data/diagnostics/loglem.log` (unos 5 MB, luego rota; solo lo puede leer su propietario).
- **Comprobar la instalación:** `loglem self-check`.
- **Puerto ya en uso:** `loglem config web-port 8790`, y reinicia.

## 9. Actualiza Loglem

Lo más sencillo es dejar que Loglem se actualice solo desde una versión firmada. La carpeta `data/` se conserva. (`install.sh` se niega a ejecutarse sobre una instalación existente; para sustituir el programa a mano, detén antes el servicio.)

```sh
loglem update check
loglem update install
loglem update status
```

Solo funciona con un Loglem instalado en una carpeta donde el usuario puede escribir y registrado con `loglem service install`. Antes de sustituir nada, comprueba la firma, la versión y el tamaño; si la versión nueva no arranca, el programa anterior se conserva al lado (`loglem.previous`). Para un programa compilado en local, indica primero dónde buscar y en qué clave confiar: `loglem update configure --repository PROPIETARIO/REPOSITORIO --public-key CLAVE_BASE64`. Las actualizaciones que cambian el formato de los datos se rechazan.
