# Límites

Lo que Loglem no hace, o no promete.

## Plataformas

- Linux y macOS. El servicio es systemd (Linux) o launchd (macOS). Actualizaciones firmadas: solo Linux.
- Windows: el código está escrito para ser portable, pero no se ha compilado ni probado.
- Todavía no hay una versión publicada.
- En un contenedor, Loglem lee solo los archivos de registro que montas. Podman, Docker, Kubernetes y el journal del anfitrión necesitan Loglem en el anfitrión.

## Qué lee

- Líneas de texto UTF-8 de hasta 1 MiB. Otras codificaciones no se leen.
- Los filtros de gravedad, código y ruta leen el texto de la línea. Conocen Caddy (JSON), Nginx y Apache (combined) y el journal. Otros formatos solo se encuentran buscando el texto.
- Las estadísticas funcionan con registros de Caddy en JSON y de Nginx/Apache en formato combined.
- Todavía no hay expresiones regulares.
- Los rankings de las estadísticas usan hasta 256 direcciones y 256 rutas por resumen. Cuando los contadores se llenan, la página muestra «Clasificación aproximada». Los recuentos de peticiones y códigos siguen siendo exactos.

## Qué se puede perder

- Podman, Docker y Kubernetes guardan los registros solo mientras existe el contenedor o el pod. Si se quita mientras Loglem está apagado, esas líneas se pierden. Loglem muestra un **hueco** en la ficha.
- Para Kubernetes, sigue la aplicación por etiqueta (`app=web`). Un pod suelto desaparece en cada despliegue.
- Si Loglem está apagado, las líneas no caducan y no se limpia nada. Consulta [stopping.md](stopping.md).
- Disco lleno: los archivos aún no archivados se conservan. Loglem no puede liberar espacio.

## Seguridad de los datos

- Los archivos son ficheros SQLite en un disco local, no en una carpeta de red. Detén Loglem antes de copiarlos a mano, o usa `loglem backup`.
- Borrar una línea en Loglem no la borra de las copias de seguridad ni de los archivos originales.
- Loglem no cifra los datos en el disco. Usa un disco cifrado y copias cifradas.
- La conexión a la página web no está cifrada. Usa una VPN o un proxy inverso con HTTPS.
- Loglem no puede saber si un programa ha terminado con un archivo. La limpieza espera 24 horas y nunca toca el archivo en uso.
- El modo prueba muestra que un día sellado no ha cambiado. No prueba que las líneas fueran verdaderas. Consulta [privacy.md](privacy.md).

## Qué se ha probado

Probado en macOS con Podman, journald (en un contenedor Linux), Caddy, Chrome y una autoridad de sello de tiempo real: añadir fuentes desde la página, búsqueda, directo, acceso con contraseña, reinicio sin líneas perdidas ni duplicadas, caducidad, rotación de Caddy, proxy inverso, copia y restauración, modo prueba, borrado.

Probado en un Linux real (Linux Mint 22.3, systemd 255, glibc 2.39, x86_64): journald, Podman 4.9 (drivers journald y de archivo, contenedores recreados y eliminados), logrotate (`create`, `delaycompress`, `dateext`, `copytruncate`), Caddy 2.11 con su propia rotación, el servicio systemd (instalación, reinicio tras `kill -9`, reinicio limpio, `check`, desinstalación), copia, verificación y restauración. Detalles y cifras: [performance.md](performance.md).

Nunca probado en: un clúster Kubernetes real, Docker, Nginx y Apache, ARM64, un reinicio real de la máquina y una actualización desde una versión publicada.
