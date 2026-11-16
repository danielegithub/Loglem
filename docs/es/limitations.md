# Límites

Lo que Loglem no hace, o no garantiza, en esta versión.

**Qué está soportado**

- Solo Linux. Windows no está soportado; en macOS no hay servicio automático (usa `loglem web`).
- Fuentes: archivos de texto `.log` (UTF-8), contenedores Podman y registros de acceso de Caddy. Los archivos comprimidos (`.gz`) no se leen. Docker, Kubernetes, Nginx y el journal de systemd aún no están soportados.
- Líneas de hasta 1 MiB.
- La búsqueda permite filtrar por texto exacto, con distinción entre mayúsculas y minúsculas, y por intervalo de fechas. Todavía no hay filtros por gravedad, dirección IP, ruta ni código de respuesta.
- La página web solo se abre desde la máquina donde se ejecuta Loglem (usa un túnel SSH para un servidor remoto).

**Seguridad de los datos**

- Los registros de Podman se leen por instantáneas. Si un contenedor se elimina antes de que Loglem lo lea, o si Loglem está parado mucho tiempo, pueden perderse líneas.
- Las entradas solo caducan mientras Loglem está en marcha (o cuando ejecutas `loglem prune --apply`).
- Borrar una entrada del archivo de registros de Loglem no la borra de instantáneas ni copias de seguridad del sistema de archivos que ya la copiaron.
- Los archivos de registros son bases de datos SQLite y deben estar en un disco local, no en una carpeta de red. No los copies mientras Loglem está en marcha; detenlo antes.
- Si el disco se llena, se conservan los originales aún no archivados; Loglem no puede liberar espacio ni impedir que otros programas escriban.
- Loglem no puede saber por sí solo que un programa ha terminado con un archivo. Su limpieza espera 24 horas y nunca toca el archivo que se está escribiendo, pero actívala solo para carpetas y registros de Caddy configurados como indica la guía.
- Los archivos de registros no están protegidos contra modificaciones: Loglem no garantiza el valor probatorio de los registros.

**Probado y aún sin probar**

- Probado con Podman real y un navegador real: añadir fuentes desde la página, búsqueda, reinicio sin líneas perdidas ni duplicadas, caducidad y rotación simulada al estilo de Caddy.
- Aún sin probar: el servicio systemd en un Linux real, Caddy real y la vuelta a la versión anterior tras una actualización fallida.
- Todavía no hay una versión publicada.
