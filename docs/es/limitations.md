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
- Las estadísticas guardan en memoria cada IP y cada página distintas del periodo. Con registros de millones de IP distintas hace falta más RAM (unos 56 bytes por IP o página distinta).

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

No probado: el servicio systemd en un Linux real, un clúster Kubernetes real (se usó un `kubectl` falso), el agente launchd de macOS, Windows y una actualización desde una versión publicada.
