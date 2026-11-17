# Loglem

[English](README.md) | [Italiano](README.it.md) | [Français](README.fr.md) | [Español](README.es.md) | [Português](README.pt.md) | [Deutsch](README.de.md)

**Un solo lugar para los registros de tu servidor: dónde están, cuánto ocupan y cuánto tiempo se guardan.**

Loglem lee tus registros, los copia en un archivo donde se puede buscar y borra las copias cuando pasa el tiempo que elegiste. Usas una página web o un menú en el terminal. Sin archivos de configuración.

> **Estado: experimental.** Funciona y está probado en macOS. Todavía no hay una versión publicada, y algunas partes no están probadas en un Linux real. Consulta los [límites](docs/es/limitations.md).

## Contenido

- [Por qué Loglem](#por-qué-loglem)
- [Qué lee](#qué-lee)
- [Limpieza segura de los archivos originales](#limpieza-segura-de-los-archivos-originales)
- [Inicio rápido](#inicio-rápido)
- [Instalación](#instalación)
- [Documentación](#documentación)
- [Licencia](#licencia)

## Por qué Loglem

Los registros se acumulan en muchos sitios. Nadie recuerda dónde están, cuánto pesan ni cuándo borrarlos.

- **Lo ves todo.** Una lista: cada fuente, dónde está, su tamaño, su estado.
- **Eliges cuánto tiempo.** 28 días, 6 meses, 2 años, por fuente. Las copias viejas desaparecen solas.
- **Puedes buscar.** Texto, fechas, IP, ruta, código de respuesta. También después de rotar el archivo o quitar el contenedor. Las líneas nuevas aparecen en directo.
- **Es pequeño.** Un programa de unos 5 MB. Unos 8 MB de RAM en reposo (medidos en Linux). Sin servidor de base de datos, sin cuenta, sin nube. Tus registros se quedan en tu máquina.
- **Es seguro por defecto.** Los archivos originales solo se leen. Borrarlos es opcional y está controlado (mira abajo).
- **Es sencillo.** Una página web, un menú de terminal que funciona por SSH y comandos para scripts.
- **Te dice qué falla.** Una fuente parada, un hueco en los registros, un disco lleno: lo ves, y `loglem check` avisa a tu monitor.
- **Ayuda con los datos personales.** Enmascara IP, borra o exporta las líneas de una persona, mira quién consultó los registros. Consulta [privacidad](docs/es/privacy.md).

## Qué lee

| Fuente | Notas |
|---|---|
| Carpetas y archivos de texto `.log` | También archivos rotados, también `.gz` (el historial se importa una vez, nunca duplicado). |
| Caddy, Nginx, Apache | Se encuentran solos en su configuración. |
| Podman, Docker | Un contenedor, o por nombre mediante el journal. |
| Kubernetes | Un pod, o todos los pods de una aplicación. |
| Journal de Linux | Todo, o algunos servicios, programas o niveles de gravedad. |

## Limpieza segura de los archivos originales

Desactivada por defecto. Si la activas para una fuente, Loglem borra un archivo terminado solo después de esta comprobación:

1. **Copia** el archivo en su archivo.
2. **Compara la copia con el original, byte a byte.** Si falta una línea, caducó o cambió, se detiene.
3. Espera a que nadie haya modificado el archivo durante **24 horas**.
4. Solo entonces borra el original.

El archivo en el que un programa todavía escribe nunca se toca. Si falla un paso, el original se queda donde está. Vale para Caddy (archivos rotados) y para carpetas de archivos terminados.

## Inicio rápido

Después de la [instalación](#instalación):

```sh
loglem                                  # menú en el terminal
loglem discover                         # busca los registros de Caddy, Nginx y Apache
loglem source add-folder --name web --path /srv/example/logs --days 28
loglem collect
loglem search --source web --text '/login' --from 2026-11-01
loglem web                              # lee y abre la página web (muestra la dirección)
```

Para seguir tras cerrar el terminal (Linux: systemd, macOS: launchd):

```sh
loglem service install --start
```

El menú y la página web están en italiano. Más en [Usar Loglem](docs/es/usage.md).

## Instalación

Cuando haya una versión publicada, descarga el paquete para tu máquina en la pestaña **Releases**:

| Máquina | Final del nombre del paquete |
|---|---|
| Linux Intel/AMD 64 bits | `x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `aarch64-unknown-linux-gnu.tar.gz` |

Los paquetes necesitan glibc 2.39 o posterior (por ejemplo Ubuntu 24.04).

Comprueba la firma con la clave pública del editor (`release-public.pem`), obtenida por un canal independiente:

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

Todo está en `~/Loglem`: el programa y una carpeta `data/`. El instalador no inicia ningún servicio. Para compilarlo tú mismo, consulta [CONTRIBUTING.md](CONTRIBUTING.md).

**En un contenedor.** Constrúyelo con `podman build -t loglem -f Containerfile .` y ejecútalo montando una carpeta de registros. Consulta [Usar Loglem](docs/es/usage.md#11-en-un-contenedor).

## Documentación

[English](docs/usage.md) | [Italiano](docs/it/usage.md) | [Français](docs/fr/usage.md) | [Español](docs/es/usage.md) | [Português](docs/pt/usage.md) | [Deutsch](docs/de/usage.md)

| Página | Qué contiene |
|---|---|
| [Usar Loglem](docs/es/usage.md) | Pocos pasos para todo |
| [Comandos](docs/es/cli.md) | Todos los comandos |
| [Datos personales y modo prueba](docs/es/privacy.md) | Herramientas RGPD, borrado, exportación, registro de accesos, días sellados |
| [Si Loglem se detiene](docs/es/stopping.md) | Qué pierdes y qué hace Loglem |
| [Límites](docs/es/limitations.md) | Lo que no hace o no promete |

## Licencia

[MIT](LICENSE).
