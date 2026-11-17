# Datos personales y modo prueba

Los registros contienen datos personales. Una dirección IP es un dato personal. Loglem te da herramientas para cumplir las reglas. **No** hace que una instalación sea conforme por sí solo: por qué guardas registros, cuánto tiempo y qué dices a las personas siguen siendo decisiones tuyas.

## Guarda menos

- **Conservación corta.** Define los días por fuente. Las líneas caducadas se borran.
- **Enmascara las IP.** `203.0.113.77` pasa a `203.0.113.0`. En el acto, o tras N días:

```sh
loglem privacy set site --mask-ips-after-days 0
loglem privacy set site --keep-fields request.uri,status
```

- **Guarda solo algunos campos JSON.** Todos los demás se descartan.
- **Mira qué guardas:** `loglem privacy report`. Ponlo en tu registro de actividades de tratamiento.

El enmascarado cambia solo la copia guardada. La comprobación antes de borrar un archivo original sigue funcionando. En modo prueba, enmascarar solo se permite «en el acto» (más tarde cambiaría líneas ya selladas).

## Borra los datos de una persona

Alguien te pide que borres sus datos. Usa la página **Datos personales**, o:

```sh
loglem erase --ip 203.0.113.7                                    # solo vista previa
loglem erase --ip 203.0.113.7 --apply --reason "solicitud art. 17 del 17/11/2026, expediente 3"
loglem erase --ip 203.0.113.7 --originals --apply --reason "…"   # también de los archivos originales
```

Loglem quita las líneas de **todas** las fuentes. El motivo va al registro de accesos. La IP nunca se escribe allí.

**Archivos originales** (`--originals`, o la casilla de la página). Loglem reescribe los archivos de registro terminados sin esas líneas. Solo si:

- el archivo no es aquel en el que un programa escribe,
- nadie lo ha modificado en 24 horas,
- el archivo entero ya está en el archivo de Loglem.

Escribe un archivo nuevo junto al viejo, lo comprueba y lo coloca en su lugar. Si algo no cuadra, el original se queda. Los archivos omitidos se listan con el motivo. Repite el comando tras la próxima rotación. Los `.gz` también se reescriben.

**No** toca:

- los registros de Podman, Docker, Kubernetes y del journal. No son archivos de Loglem, y el journal no permite quitar entradas sueltas. Acorta su duración, o recrea el contenedor.
- las copias de seguridad ya hechas (caducan con su propia rotación),
- el archivo en el que un programa escribe, hasta que se rote.

En `--reason` escribe un número de expediente, no un correo: un correo también es un dato personal.

## Exporta los datos de una persona

Alguien te pregunta qué datos tienes de él:

```sh
loglem export --ip 203.0.113.7 --output /sitio/seguro/solicitud-3.txt   # --json para JSON
```

Solo el propietario puede leer el archivo. Bórralo después de enviarlo.

## Quién ha mirado

`loglem audit` (o los archivos de `data/audit/`) lista cada acceso, búsqueda, exportación, borrado y cambio de ajustes, con usuario y hora. Los valores buscados **no** se escriben, solo el tipo de filtro («texto, ip»). Las entradas se guardan 13 meses.

## Disco y copias de seguridad

- Loglem no cifra sus archivos. Usa un disco cifrado.
- `loglem backup` escribe archivos en claro. Cífralos antes de que salgan del servidor (por ejemplo con restic).
- No pongas copias de registros con datos personales en un almacenamiento del que no puedas borrar (como un bucket bloqueado).

## Modo prueba

Para una fuente que debe demostrar que sus líneas no se han cambiado:

```sh
loglem proof enable site --tsa https://freetsa.org/tsr     # --tsa es opcional
loglem proof verify site
```

Cada día terminado recibe una huella (SHA-256) de sus líneas. Dentro va también la huella del día anterior: cambiar una rompe todas las siguientes. Con `--tsa`, una autoridad externa firma cada huella con la hora; solo ve la huella.

`loglem proof verify` recalcula cada día y dice **íntegro**, **ALTERADO** o **caducado**. También puedes comprobar una copia: `loglem proof verify --archive copia.db --chain copia.chain`.

Qué demuestra: un día sellado no cambió después del sello (o de la hora del sello de tiempo). No demuestra que las líneas fueran verdaderas cuando se escribieron.

Las líneas caducan solo por días enteros, y cada caducidad se escribe en el registro.

**Borrado en modo prueba.** Gana el borrado. Loglem quita las líneas, **recalcula** la huella de ese día y de todos los siguientes, y añade una nota al registro: cuándo, por qué, cuántas líneas, huella antigua → nueva. `proof verify` muestra la nota. Los sellos de tiempo antiguos siguen valiendo para la versión antigua. Se piden nuevos para las huellas nuevas.
