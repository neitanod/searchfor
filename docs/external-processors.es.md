> 🇬🇧 Read in English: [external-processors.md](external-processors.md)

# Cómo escribir un External Processor

Un **External Processor** le da Trails a un Formato que searchfor no conoce, sin cambiar s4.
Es un programa que escribís vos, en cualquier lenguaje, y que s4 corre como **Helper
Processor**: s4 le manda la ruta de un archivo y las posiciones de los resultados, y el
programa contesta con el Trail de cada resultado. Esta guía muestra cómo escribir uno, en este
orden:

1. [El ejemplo completo](#el-ejemplo-completo-archivos-ini-primero-con---pre-después-con-un-helper):
   un Formato INI, primero con `--pre` y después como Helper Processor, para mostrar cuándo
   conviene cada uno.
2. [La Definición de Formato](#la-definición-de-formato): los campos que nombran el programa.
3. [El protocolo v1](#el-protocolo-v1-mensaje-por-mensaje), mensaje por mensaje.
4. [Los tres modos](#los-tres-modos) y cuándo elegir cada uno.
5. [Las fallas y cómo las informa s4](#las-fallas-y-cómo-las-informa-s4).
6. [La memoria y los archivos temporales](#la-memoria-y-los-archivos-temporales).
7. [Cómo probar un helper](#cómo-probar-un-helper).

El glosario de los términos en negrita está en [CONTEXT.md](../CONTEXT.md). La descripción
normativa del protocolo es el
[§4.4 de la spec](specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459), y esta
guía la sigue. Cada salida de abajo es lo que s4 imprime sobre los archivos de
[`docs/examples/`](examples), y `tests/guide.rs` corre los dos ejemplos y comprueba su salida
en Linux, macOS y Windows.

## El ejemplo completo: archivos INI, primero con `--pre`, después con un helper

El ejemplo busca en este archivo, [`docs/examples/sample.ini`](examples/sample.ini). Los
comandos del ejemplo se corren desde `docs/examples/`, donde está el archivo:

```ini
[server]
host = needle.example
```

s4 no tiene un Formato INI, así que por defecto imprime el resultado como **Plain**, con el
Trail vacío. Hay dos herramientas para cambiar eso, y resuelven problemas distintos.

### `--pre`: convertir el archivo en texto

`--pre COMMAND` es el flag de ripgrep, y s4 lo corre como rg: ejecuta `COMMAND PATH` con el
archivo en el stdin, y busca en lo que el comando escribe en lugar del archivo. `--pre` sirve
cuando el archivo no es texto en el que se pueda buscar (un formato binario, comprimido o
codificado) y un programa puede convertirlo en texto.

[`ini-pre.py`](examples/ini-pre.py) escribe sólo los valores de un archivo INI, uno por
línea:

```
$ s4 --no-docs --no-trail --pre ./ini-pre.py needle sample.ini
needle.example
```

La salida es la que rg imprime con el mismo `--pre`. La sección y la clave desaparecieron: el
texto convertido no tiene estructura, así que no tiene un Trail para dar. `--pre` elige **en
qué** se busca.

**Cómo correr un comando `--pre` en Python en cada sistema:**

- **Linux y macOS.** `--pre` corre un programa, y un script es un programa cuando empieza con
  una línea shebang y tiene el bit de ejecución. `ini-pre.py` empieza con
  `#!/usr/bin/env python3` y está commiteado como ejecutable, así que el comando de arriba
  funciona tal cual. Así escriben sus scripts de `--pre` los que ya usan rg.
- **Windows.** Windows no lee las líneas shebang, así que `--pre` tiene que nombrar un
  programa (`.exe`). [`pre-launcher.rs`](examples/pre-launcher.rs) es un programa nativo
  chico que corre `SEARCHFOR_EXAMPLE_PYTHON SEARCHFOR_EXAMPLE_CONVERTER PATH`, comparte su
  stdin, su stdout y su stderr con el conversor, y sale con el código de salida del conversor.
  Se compila con el compilador de Rust, sin un proyecto de Cargo, y `--pre` recibe su ruta
  absoluta: en Windows rg, y s4 con él, busca un programa de `--pre` relativo sólo en el
  `PATH`, nunca en el directorio actual.

  ```
  > rustc pre-launcher.rs -o ini-pre-launcher.exe
  > $env:SEARCHFOR_EXAMPLE_PYTHON = "python"
  > $env:SEARCHFOR_EXAMPLE_CONVERTER = "$PWD\ini-pre.py"
  > s4 --no-docs --no-trail --pre "$PWD\ini-pre-launcher.exe" needle sample.ini
  needle.example
  ```

  Esos comandos son de PowerShell. El launcher también funciona en Linux y macOS: desde la raíz
  del repositorio, `rustc docs/examples/pre-launcher.rs -o ini-pre-launcher` lo compila, y las
  dos variables nombran el intérprete y el script como arriba. El test de esta guía compila el
  launcher y corre esta búsqueda con él en cada sistema.

### Un Helper Processor: describir la estructura del archivo

Un Helper Processor deja la búsqueda como está: s4 busca en los bytes del propio archivo, y le
pregunta al helper sólo dónde está cada resultado. Un Helper Processor sirve cuando el archivo
ya es texto y lo que falta es su estructura, el **Trail**.

[`ini-helper.py`](examples/ini-helper.py) lee un archivo INI y contesta con sus secciones y sus
claves. Su Definición de Formato es [`ini.toml`](examples/ini.toml):

```toml
name       = "ini-guide"
extensions = ["ini"]
processor  = "external"

[params]
command    = ["python3", "/absolute/path/to/docs/examples/ini-helper.py"]
mode       = "oneshot"
timeout_ms = 5000
```

Copiá `ini.toml` al directorio de formatos y escribí en `command` la ruta absoluta de
`ini-helper.py`. El directorio de formatos es `searchfor/formats/` dentro del directorio de
configuración del usuario:

| Sistema | Directorio de formatos |
|---|---|
| Linux | `$XDG_CONFIG_HOME/searchfor/formats/` (`~/.config/searchfor/formats/` por defecto) |
| macOS | `~/Library/Application Support/searchfor/formats/` |
| Windows | `%APPDATA%\searchfor\formats\` |

`SEARCHFOR_FORMATS_DIR` reemplaza ese directorio, como hacen los tests. En Windows escribí
`"python"` (o `"py"`, el lanzador de Python) en lugar de `"python3"`. Con eso, la misma
búsqueda da el Trail:

```
$ s4 --flat needle sample.ini
2: server › host │ host = needle.example
```

`server › host` es el Trail: la sección `server`, y dentro de ella la clave `host`. En la
salida de árbol, la de por defecto en una terminal:

```
$ s4 --tree needle sample.ini
sample.ini
  server
    host
      2:host = needle.example
```

y `--json` lleva el mismo Trail como datos, con el nombre del Formato:

```json
{"type":"match","data":{"path":{"text":"sample.ini"},"lines":{"text":"host = needle.example\n"},"line_number":2,"absolute_offset":9,"submatches":[{"match":{"text":"needle"},"start":7,"end":13}],"format":"ini-guide","trail":[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}],"fragment":null}}
```

### Cuál de los dos usar

| Qué tenés | Qué usar |
|---|---|
| Un archivo que no es texto en el que se pueda buscar (binario, comprimido, codificado) | `--pre`, que lo convierte |
| Un archivo de texto cuya estructura s4 no conoce | Un Helper Processor, que da el Trail |
| Las dos cosas: un archivo binario con estructura | `--pre` para el texto, y un Helper Processor para el Formato que dice el nombre del archivo: s4 le manda al helper el texto convertido en un archivo temporal |

Los dos se pueden combinar: con `--pre`, el Helper Processor recibe el texto convertido, así
que `ini-helper.py` detrás de `--pre ini-pre.py` no encuentra secciones e imprime el resultado
con el Trail vacío (`1: needle.example`).

## La Definición de Formato

Una Definición de Formato es un archivo TOML en el directorio de formatos. Un Helper Processor
usa estos campos:

| Campo | Obligatorio | Qué es |
|---|---|---|
| `name` | sí | El nombre del Formato: `--format <name>` lo elige, y los avisos y `--json` lo muestran. Una Definición del usuario con el nombre de un Formato incorporado lo reemplaza |
| `extensions` | no | Extensiones de archivo sin el punto, como `["ini"]` |
| `names` | no | Nombres exactos de archivo, como `["Dockerfile"]` |
| `shebangs` | no | Intérpretes nombrados en una primera línea `#!`, que se lee sólo en un archivo sin extensión |
| `magic` | no | Bytes del principio, como `[{ offset = 0, hex = "7f454c46" }]`, que se leen sólo en un archivo sin extensión |
| `processor` | sí | `"external"` para un Helper Processor |
| `params.command` | sí | El programa y sus argumentos, como un array. Un nombre solo se busca en el `PATH`, y nunca en el directorio actual; una ruta se usa como está |
| `params.mode` | no | `"pool"` (el default), `"concurrent"` u `"oneshot"`; ver [los tres modos](#los-tres-modos) |
| `params.timeout_ms` | no | Lo más que s4 espera cada respuesta, en milisegundos, contado desde que s4 encola el pedido; un entero positivo, 5000 si falta |

Una Definición sin `extensions`, `names`, `shebangs` ni `magic` se aplica sólo a los archivos
de `--format <name>`. Cada clave de `[params]`, también las que s4 no usa, le llega al helper
en `hello`, así que un helper puede tomar sus propias opciones de la Definición.
`s4 --list-formats` muestra si la Definición se cargó, y una inválida da un aviso que nombra el
archivo y el campo.

## El protocolo v1, mensaje por mensaje

s4 y el helper intercambian JSON en UTF-8, un objeto por línea terminada en `\n`, en los dos
sentidos: s4 escribe en el stdin del helper, y el helper contesta por su stdout. Los dos lados
ignoran los campos que no conocen. Cada pedido tiene un `id`, y su respuesta lleva el mismo
`id`. Cada línea que el helper escribe en su stderr se muestra como un aviso,
`s4: <format>: <line>`, salvo que el usuario haya pasado `--no-messages`.

Esta es la conversación de `s4 --flat needle sample.ini` con `ini-helper.py`, grabada en Linux
(los `params` de `hello` son los de la Definición, acortados acá):

```
s4 → {"op":"hello","id":0,"protocol":1,"format":"ini-guide","params":{"command":[…],"mode":"oneshot"},"threads":1}
s4 → {"op":"trails","id":1,"path":"/home/me/sample.ini","matches":[{"line":2,"byte":16}]}
   ← {"id":0,"protocol":1,"name":"ini-helper.py","version":"1.0.0"}
   ← {"id":1,"trails":[[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]]}
```

### `hello`

La primera línea que recibe cada proceso:

```json
{"id":0,"op":"hello","protocol":1,"format":"ini-guide","params":{…},"threads":1}
```

`params` es la tabla `[params]` de la Definición y `threads` es la mayor cantidad de pedidos
que s4 va a tener en curso con este proceso (1 en `pool` y en `oneshot`). Contestá con el
protocolo que hablás y, si querés, con tu nombre y tu versión:

```json
{"id":0,"protocol":1,"name":"ini-helper.py","version":"1.0.0"}
```

Si el helper contesta otro número de protocolo o un `error`, no contesta dentro de
`timeout_ms`, o termina antes de contestar, s4 desactiva la Definición por el resto de la
corrida, con un aviso, e imprime los archivos del Formato como Plain.

### `trails`

El Trail de cada resultado de un archivo:

```json
{"id":1,"op":"trails","path":"/home/me/sample.ini","matches":[{"line":2,"byte":16}]}
```

`line` es la línea del resultado, contando desde 1, y `byte` su posición absoluta en bytes en
el archivo de `path`. Contestá con un Trail por resultado, en el orden de `matches`:

```json
{"id":1,"trails":[[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]]}
```

Un Trail es una lista de **Segmentos**, del más externo al más interno. Un Segmento es el
objeto de `--json`: `kind`, `name`, `line` y `end_line` (la primera y la última línea de la
Sección), y los opcionales `level` y `signature`. Una lista vacía es un resultado sin Trail.

**Contá bytes, no caracteres.** `byte` cuenta los bytes del archivo. Leé el archivo como bytes
y calculá cada posición sobre ellos; en Python, `open(path, "rb")`, nunca un `str`
decodificado, cuyos índices cuentan caracteres y se corren en cuanto una línea tiene una `é` o
un BOM. `ini-helper.py` decodifica un nombre sólo para mostrarlo.

### `names`

Cada nombre de Segmento de un archivo, para `--in-headings`, que busca el patrón en los nombres
en lugar de en las líneas:

```json
{"id":8,"op":"names","path":"/home/me/sample.ini"}
```

```json
{"id":8,"names":[{"text":"server","pieces":[[0,6,1,7]],"anchor":1,"trail":[{"kind":"section","name":"server","line":1,"end_line":2}]},{"text":"host","pieces":[[0,4,9,13]],"anchor":9,"trail":[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]}]}
```

- `text` es el nombre como se muestra.
- `pieces` dice de dónde viene cada pedazo de `text`, como rangos de bytes `[text_start,
  text_end, src_start, src_end]`. Los largos son iguales cuando el nombre se copia tal cual
  (`server` son los bytes 0–6 del texto y los bytes 1–7 del archivo), y distintos cuando el
  helper lo decodificó (una entidad como `&lt;` es 1 byte de texto sobre 4 del archivo). Un
  nombre repartido en dos líneas tiene un pedazo por línea.
- `anchor` es el byte donde cae un resultado vacío (`^`, `$`) cuando ningún pedazo lo
  contiene.
- `trail` es el Trail del Segmento nombrado, con el propio Segmento al final.

### Errores

```json
{"id":9,"error":"ValueError: unknown op 'unknown'"}
```

Una respuesta `error` hace fallar sólo ese pedido, y el proceso sigue con el siguiente.
`ini-helper.py` contesta un `error` a una operación que no conoce y a un archivo que no puede
leer.

## Los tres modos

`params.mode` dice cómo le pasa s4 los pedidos al helper:

| Modo | Cómo le pasa s4 los pedidos | Cuándo usarlo |
|---|---|---|
| `pool` (default) | Cada copia del helper tiene un pedido por vez. s4 arranca una copia nueva sólo cuando todas las que corren están ocupadas, hasta la cantidad de threads de la búsqueda (`-j`), y le da cada archivo a una copia libre | Un programa que lee una línea, la contesta y lee la siguiente. Elegilo salvo que alguno de los otros dos encaje mejor |
| `concurrent` | Un solo proceso recibe hasta la cantidad de threads de la búsqueda en pedidos sin esperar las respuestas (`threads` en `hello` dice cuántos), y puede contestarlos en cualquier orden: s4 empareja cada respuesta con su pedido por el `id` | Un helper caro de arrancar (una Gramática grande, una JVM) que puede trabajar en varios archivos a la vez |
| `oneshot` | Un proceso por pedido: s4 escribe `hello` y el pedido, cierra el stdin, y lee las dos respuestas | Scripts chicos: el script puede leer todo su stdin antes de escribir algo. Cada archivo paga el arranque de un proceso |

`ini-helper.py` es un helper `oneshot`: lee su stdin hasta el EOF y recién entonces contesta
cada pedido. Un helper `pool`, en cambio, tiene que contestar cada línea a medida que la lee y
vaciar su stdout (flush) después de cada respuesta; s4 manda el pedido siguiente recién después
de la respuesta.

En modo `concurrent`, las respuestas de un helper que atiende pedidos en paralelo pueden volver
en otro orden que los pedidos, como en este esquema:

```
s4 → {"id":1,"op":"trails","path":"a.ini","matches":[…]}
s4 → {"id":2,"op":"trails","path":"b.ini","matches":[…]}
   ← {"id":2,"trails":[…]}
   ← {"id":1,"trails":[…]}
```

**Vida del proceso.** s4 arranca un helper cuando el primer archivo de su Formato tiene un
resultado, le manda cada archivo siguiente de ese Formato en la misma corrida, y le cierra el
stdin cuando termina la corrida. Terminá con ese EOF: s4 espera hasta un segundo, y después
mata al helper. La corrida siguiente lo arranca de nuevo. Un helper que arranca procesos
propios tiene que terminarlos cuando sale; s4 espera como mucho un segundo más por los pipes
que un hijo así deje abiertos. En Windows, cada proceso que arranca s4, helpers y sus hijos
incluidos, pertenece a un Job Object que los termina cuando termina s4.

## Las fallas y cómo las informa s4

Un Helper Processor nunca frena la búsqueda ni cambia el código de salida de s4. Un archivo
cuyo pedido falla se imprime como Plain (sin Trail, `format` en `null` en `--json`), con un
aviso que nombra el archivo y la Definición. Los avisos de la tabla son salida real de s4, en
inglés como los imprime; cada uno sigue al caso que informa:

| Caso | Aviso | Qué pasa después |
|---|---|---|
| El pedido recibe una respuesta `error` | `s4: sample.ini: ini-guide: no parser for this file` | El archivo se imprime como Plain; el proceso sigue. No cuenta como falla |
| Ninguna respuesta dentro de `timeout_ms` | `s4: sample.ini: ini-guide: did not answer within 500 ms` | s4 mata el proceso, y eso hace fallar los otros pedidos que tenía en curso |
| El helper termina con pedidos en curso | `s4: sample.ini: ini-guide: the helper exited` | El pedido siguiente arranca un proceso nuevo |
| El helper escribe una línea que no es JSON, o que no tiene `id` | `s4: sample.ini: ini-guide: the helper wrote a line that is not JSON (expected value at line 1 column 1)` | s4 mata el proceso, y falla cada pedido que tenía en curso |
| Tres fallas en una corrida (crashes, timeouts, líneas sin JSON o sin `id`) | `s4: ini-guide: the helper failed 3 times; its files print without Trails for the rest of this run` | La Definición queda desactivada por la corrida |
| Falla `hello` (otro protocolo, `error`, sin respuesta, el proceso termina) | `s4: ini-guide: did not answer hello within 300 ms; its files print without Trails for the rest of this run` | La Definición queda desactivada por la corrida |
| El comando no puede arrancar | ``s4: ini-guide: cannot start `no-such-python3 x.py`: No such file or directory (os error 2); its files print without Trails for the rest of this run`` | La Definición queda desactivada por la corrida |
| El helper escribe en su stderr | `s4: ini-guide: <the line>` | Nada más; `--no-messages` lo silencia |

Cómo cuenta s4 las fallas:

- Cada proceso cuenta una vez, sin importar cuántos pedidos hizo fallar.
- Una respuesta `error`, y una respuesta con el `id` correcto cuyos campos no encajan en el
  protocolo (una cantidad equivocada de Trails, un Segmento sin `name`), hacen fallar sólo ese
  pedido y no cuentan.
- Un proceso que termina entre pedidos, sin nada en curso, se reemplaza sin contar una falla,
  así que un helper puede terminar después de cada respuesta si prefiere.
- Mientras una Definición está desactivada, sus archivos se imprimen como Plain, sin más
  avisos. La corrida siguiente vuelve a probar el helper.

## La memoria y los archivos temporales

**No guardes nada de un archivo para el siguiente.** Los pedidos son independientes: leé el
archivo, calculá la respuesta, escribila, y liberá el archivo, su árbol y la respuesta antes de
leer el pedido siguiente. Mantené cargado sólo lo que es del Formato: una Gramática, una query
compilada, los `params` de `hello`. Un helper `pool` o `concurrent` vive toda la corrida, que
puede ser de miles de archivos, así que una memoria que crece por archivo termina frenando la
búsqueda. Los helpers propios de searchfor tienen tests que comprueban que su memoria queda
plana a lo largo de 5 000 pedidos.

**Leé el archivo de `path`.** s4 manda la ruta y las posiciones, nunca el contenido. `path` es
absoluta, o relativa al directorio de trabajo de s4, que el helper hereda. s4 manda la ruta
resuelta del propio archivo sólo cuando es un archivo regular cuyos bytes s4 buscó sin
cambiarlos. Para todo lo demás (stdin, un FIFO, un archivo al que s4 le sacó el BOM o cuyo
UTF-16 transcodificó, la salida de `--pre` o de `-z`), s4 escribe los bytes que buscó en un
archivo temporal y manda esa ruta, así que `byte` siempre cuenta los bytes de `path`. Nunca
vuelvas a abrir un stream por tu cuenta: en el helper, el stdin es el pipe del protocolo.

**Los archivos temporales son de s4.** s4 los guarda en un directorio propio,
`searchfor-<random>`, creado en el directorio temporal del sistema (`$TMPDIR` o `/tmp` en Unix,
`%TEMP%` en Windows) cuando hace falta el primero. Cada archivo se llama `<n>-<file name>`, así
el helper sigue viendo la extensión, y s4 lo borra después de la respuesta. Leelo y dejalo como
está: no lo modifiques, ni lo muevas, ni lo guardes. En Unix el directorio toma los permisos
del umask del usuario; si otros usuarios de la máquina no deben leer lo que buscás desde stdin
o con `--pre`, apuntá `TMPDIR` a un directorio que sólo vos puedas leer. Si el helper necesita
archivos de trabajo propios, crealos con la función segura de archivos temporales de su
lenguaje (`tempfile` en Python), nunca con un nombre fijo en un directorio compartido, y
borralos antes de contestar.

## Cómo probar un helper

1. **Hablale a mano.** Escribí los pedidos en su stdin y leé las respuestas. Este es el
   intercambio `oneshot` del ejemplo, con `names` y una operación desconocida:

   ```
   $ printf '%s\n' '{"id":0,"op":"hello","protocol":1,"format":"ini-guide","params":{},"threads":1}' \
       '{"id":8,"op":"names","path":"sample.ini"}' '{"id":9,"op":"unknown"}' \
       | python3 ini-helper.py
   {"id":0,"protocol":1,"name":"ini-helper.py","version":"1.0.0"}
   {"id":8,"names":[{"text":"server","pieces":[[0,6,1,7]],"anchor":1,"trail":[{"kind":"section","name":"server","line":1,"end_line":2}]},{"text":"host","pieces":[[0,4,9,13]],"anchor":9,"trail":[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]}]}
   {"id":9,"error":"ValueError: unknown op 'unknown'"}
   ```

2. **Comprobá que s4 carga la Definición.** `s4 --list-formats` la lista con su origen:

   ```
   $ s4 --list-formats | grep ini-guide
   ini-guide    user (/home/me/.config/searchfor/formats/ini-guide.toml) ini
   ```

3. **Corrélo a través de s4.** `--format <name>` le aplica el Formato a cada archivo que
   nombrás, sea cual sea su extensión, así que podés probar el helper sobre cualquier archivo:

   ```
   $ s4 --flat --format ini-guide needle sample.ini
   2: server › host │ host = needle.example
   ```

   `--json` muestra los campos de cada Segmento como los mandó el helper, y un aviso en stderr
   nombra cualquier falla. Probá `--in-headings` para ejercitar `names`.

4. **Corré el test de la guía.** `cargo test --test guide` corre los dos ejemplos de esta guía
   (la búsqueda con `--pre` a través del launcher compilado, y el Helper Processor a través de
   s4) y el intercambio del paso 1. Donde falta Python 3 imprime que los salteó.
