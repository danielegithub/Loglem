# Si Loglem se detiene

Loglem puede estar apagado: un fallo, un reinicio, una actualización, un error. Esto es lo que pierdes.

| Fuente | Mientras Loglem está apagado | Qué se pierde |
|---|---|---|
| Archivos, Caddy, Nginx | Los programas siguen escribiendo y rotando. | Nada, hasta que el programa borre por sí mismo los archivos viejos (`roll_keep`, `roll_keep_for`, logrotate). |
| Podman, Docker | El programa de contenedores guarda los registros. | Las líneas que borró, y todas si el contenedor se quitó o se recreó. |
| Kubernetes | Los pods guardan los registros. | Las líneas de un pod sustituido. Sigue `app=nombre`, no un solo pod. |
| Journal de Linux | El journal guarda las entradas. | Las entradas que el journal borró por espacio o antigüedad. |

Mientras Loglem está apagado, las líneas caducadas no se borran y los archivos originales no se limpian.

## Qué hace Loglem

- **Al arrancar de nuevo** lee desde el punto guardado. La ficha dice «Reanudado tras 3 horas de parada: N líneas recuperadas».
- **Los huecos se ven.** Si pueden faltar líneas, lo dicen la ficha, el aviso y `loglem check`, con un inicio y un fin estimados.
- **Parada normal.** En una parada normal (`systemctl stop`, una actualización) lee cada fuente una última vez, 30 segundos como máximo.
- **Fallo.** En el siguiente arranque dice que la última parada no fue limpia. Reanuda desde el último punto guardado. Una línea se guarda junto con su posición de lectura, así que nada se lee dos veces.
- **Reinicio automático.** El servicio se reinicia tras un fallo, 10 veces como máximo en 5 minutos.
- **Alerta.** `loglem check` falla si Loglem está apagado o una fuente lleva 15 minutos sin leerse. Lánzalo desde cron o un monitor.
- **Margen de Caddy.** Loglem avisa si `roll_keep_for` está por debajo de 7 días, o si los archivos rotados sin leer se acercan a `roll_keep`.

## Para estar más seguro

- Registros de contenedores: sigue el contenedor por nombre mediante el journal, si escribe allí (la página lo ofrece). Sobrevive a un nuevo despliegue.
- Kubernetes: usa `--selector app=nombre`.
- Mantén `roll_keep_for` de Caddy en 30 días o más.
- Lanza `loglem check` cada pocos minutos.
