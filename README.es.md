# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Un solo lugar para los registros de tu servidor: dónde están, cuánto espacio ocupan y cuánto tiempo se conservan.**

Loglem lee los registros de tu servidor Linux (archivos `.log` y contenedores Podman), los copia en un archivo de registros consultable y borra automáticamente las entradas archivadas cuando vence el periodo de conservación que elegiste. Lo gestionas todo desde una página web accesible en el propio servidor o desde un menú del terminal que se maneja con el teclado, sin editar archivos de configuración.

> **Estado: experimental.** Funciona y está probado, pero todavía no hay una versión publicada y algunas partes aún necesitan una verificación completa en un servidor real. Consulta los [límites](docs/es/limitations.md).

## Contenido

- [Por qué Loglem](#por-qué-loglem)
- [Qué hace](#qué-hace)
- [Inicio rápido](#inicio-rápido)
- [Instalación](#instalación)
- [Documentación](#documentación)
- [Licencia](#licencia)

## Por qué Loglem

Los registros se acumulan en muchos sitios: una carpeta para el servidor web, otra para la aplicación, la salida de cada contenedor. Nadie recuerda dónde están, cuánto han crecido ni cuándo habría que borrarlos.

Con Loglem:

- **Siempre sabes dónde están los registros.** Una lista muestra cada fuente, su ubicación original, la ubicación del archivo de registros, el tamaño y el estado de la última lectura.
- **Tú decides cuánto tiempo se guardan los registros de cada fuente.** 28 días, 6 meses o 2 años, fuente por fuente. Las entradas caducadas se eliminan por ti: el disco no se llena y los datos viejos no se quedan por descuido.
- **Puedes buscar.** Texto e intervalos de fechas en todo lo recopilado, incluso tras la rotación del archivo original o la eliminación del contenedor.
- **No molesta.** Un único ejecutable de unos 4 MB, unos 10 MB de RAM en reposo, sin servidor de base de datos, sin cuenta y sin servicio en la nube. Tus registros nunca salen de la máquina.
- **Es seguro por defecto.** Los archivos originales solo se leen, nunca se modifican. Borrar un original es opcional y solo se aplica a los archivos que entregas explícitamente una vez finalizada su escritura.
- **Es sencillo de usar.** Todo se hace desde una página que solo escucha en el propio servidor, o desde un menú de terminal que funciona también por SSH. También hay comandos para scripts y automatización.
- **Sustituye un montón de scripts.** Recoger, limpiar, conservar y buscar registros suele exigir una mezcla de reglas de rotación, tareas programadas y comandos improvisados. Loglem lo hace todo en un solo lugar, con los mismos ajustes en la página web, el terminal y la línea de comandos.
- **Va contigo.** Rutas, contenedores y duraciones son ajustes, no código: exporta la configuración e impórtala en otra máquina.

## Qué hace

1. **Añades una fuente:** una carpeta de archivos `.log`, un archivo suelto, un contenedor Podman o el registro de acceso de Caddy.
2. **Loglem recoge las líneas nuevas** cada 10 segundos por defecto y las guarda en una pequeña base de datos SQLite para esa fuente. Recuerda dónde se quedó, así que nada se lee dos veces.
3. **Buscas y compruebas** desde la página web o el terminal, y ves el estado de cada fuente.
4. **Las entradas antiguas se eliminan** cuando superan la duración que fijaste para esa fuente.
5. **Si algo falla,** los errores de Loglem se guardan con su causa y se ven en una página de Diagnóstico. Una fuente que falla no detiene a las demás.

Funciona como servicio en segundo plano (un servicio de usuario de systemd) o bajo demanda. Se ejecuta en Linux, sin escritorio gráfico.

## Inicio rápido

Después de la [instalación](#instalación):

```sh
# Abre el menú guiado del terminal (flechas, Intro, Esc)
loglem

# O añade una carpeta de registros desde la línea de comandos y léela una vez
loglem source add-folder --name web-logs --path /srv/example/logs --days 28
loglem collect
loglem status

# Buscar
loglem search --source web-logs --text '/login' --from 2026-11-01 --to 2026-11-16

# Abre la página web (imprime su propia dirección, por ejemplo http://127.0.0.1:53142)
loglem web
```

Para seguir recogiendo registros después de cerrar el terminal:

```sh
loglem service install --start
```

El menú del terminal y la página web usan por ahora etiquetas en italiano. Detalles en [Usar Loglem](docs/es/usage.md).

## Instalación

Cuando se publique una versión, descarga el paquete para tu máquina desde la pestaña **Releases** de este repositorio:

| Máquina | Sufijo del paquete |
|---|---|
| Linux Intel/AMD de 64 bits | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Los paquetes necesitan glibc 2.39 o posterior (por ejemplo Ubuntu 24.04).

Antes de instalar, comprueba la firma con la clave pública del editor (`release-public.pem`), obtenida por un canal independiente:

```sh
openssl pkeyutl -verify -pubin -inkey release-public.pem -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
sha256sum --check --ignore-missing SHA256SUMS
```

Después instala:

```sh
tar xzf loglem-*.tar.gz
cd loglem-*
sh install.sh
~/Loglem/loglem
```

Todo se guarda en `~/Loglem`: el programa y una carpeta `data/` con ajustes, registros archivados y diagnósticos. El instalador no activa ningún servicio por sí solo. Para las fuentes Podman, Podman debe estar instalado y ser accesible para el mismo usuario que ejecuta Loglem.

Para compilarlo tú mismo, consulta [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentación

La documentación está disponible en: [English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Página | Contenido |
|---|---|
| [Usar Loglem](docs/es/usage.md) | Guía paso a paso: añadir fuentes, búsqueda, conservación, ejecución como servicio, limpieza, traslado a otra máquina, solución de problemas y actualizaciones |
| [Referencia de comandos](docs/es/cli.md) | Todos los comandos |
| [Límites](docs/es/limitations.md) | Lo que no hace o no garantiza |

## Licencia

[MIT](LICENSE).
