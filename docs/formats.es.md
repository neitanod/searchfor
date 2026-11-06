> 🇬🇧 Read in English: [formats.md](formats.md)

# Los Formatos de searchfor

Esta es la referencia de cada Formato que describe s4: qué archivos reclama, de qué Segmentos
están hechos sus Trails, sus límites y qué hace con lo que no puede leer. El
[README](../README.es.md#formatos) tiene la tabla corta, con un Trail de ejemplo por Formato.

Todos los ejemplos de abajo son salidas reales sobre [`tests/fixtures/`](../tests/fixtures),
corridas desde adentro de ese directorio, salvo donde se dice sobre qué corren. Los archivos
objeto y los documentos de `m4/` los escriben ahí los writers que usa
[`tests/cli_m4.rs`](../tests/cli_m4.rs) (`searchfor::processors::object::fixture` y
`searchfor::docs::fixture`), y las filas de
[imágenes, audio y binarios](#imágenes-audio-y-binarios) salen de los archivos que
`searchfor::processors::object::fixture::m6` escribe para
[`tests/cli_m6_object.rs`](../tests/cli_m6_object.rs). `s4 --list-formats` lista cada Formato
con su origen y sus extensiones, y `s4 --export-formats DIR` escribe las Definiciones incluidas
en `DIR` como archivos TOML, para copiarlas y editarlas.

## Cómo elige s4 un Formato

Un archivo toma el primer Formato que le da una de estas señales, en este orden:

1. `--format NOMBRE` para todos los archivos, o `--format 'GLOB=NOMBRE'` para los que el glob
   reconoce.
2. Un nombre exacto de los `names` de una Definición: `Dockerfile`, `.env`, `nginx.conf`.
3. Una extensión compuesta de `extensions`, una entrada con un punto: `cart.blade.php` es
   Blade, mientras que `cart.php` sigue siendo PHP.
4. Un glob de `paths` cuyo último componente escribe la extensión: `**/ChatExport_*/result.json`
   hace `telegram` a un `result.json` dentro de una exportación de Telegram, mientras que un
   `result.json` en cualquier otro lado sigue siendo JSON.
5. La última extensión, comparada sin distinguir mayúsculas.
6. Todos los demás globs de `paths`, cuyo último componente deja abierta la extensión:
   `**/.ssh/config`, `Dockerfile.*`, `.env.*`. Van después de la extensión, así que
   `Makefile.toml` sigue siendo TOML y `.env.json` sigue siendo JSON.
7. Los primeros bytes (`magic`), y 8. la línea shebang (`#!/usr/bin/env python3`), las dos
   leídas sólo para un archivo cuyo nombre no tiene una extensión que decida un Formato.

Un glob sin `/` se compara con el nombre del archivo y uno con `/` con la ruta entera, con `/`
entre los componentes en todos los sistemas (en Windows el glob ve `\` como `/`). s4 nunca mira
el contenido de un archivo para elegir un Formato de texto. Un archivo que ninguna señal
reclama es **Plain**: sus resultados se imprimen con el Trail vacío, como los imprime `rg`. Una
Definición de usuario con el nombre de una incluida la reemplaza, y `--list-formats` la muestra
como `overridden`.

## Todos los Formatos

| Formato | `--format` | Archivos | Segmentos |
|---|---|---|---|
| Markdown | `markdown` | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | título del frontmatter, títulos, bloques de código, ítems de lista padres, columnas de tabla |
| YAML | `yaml` | `.yaml`, `.yml` | claves y elementos de arrays; `document N` para cada documento de un stream |
| JSON | `json` | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest`, `.babelrc`, `.eslintrc` | claves y elementos de arrays; acepta comentarios y comas finales |
| TOML | `toml` | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | claves, tablas y arrays de tablas |
| PHP | `php` | `.php`, `.phtml` | namespaces, clases, interfaces, traits, enums, métodos, funciones |
| JavaScript | `javascript` | `.js`, `.mjs`, `.cjs`, `.jsx` | clases, métodos, funciones, y funciones flecha o expresiones de función con nombre |
| TypeScript | `typescript` | `.ts`, `.mts`, `.cts` | los de JavaScript, más namespaces, interfaces y clases abstractas |
| TSX | `tsx` | `.tsx` | los de TypeScript |
| Bash | `bash` | `.sh`, `.bash`, `.bats`, `.bashrc`, `.bash_profile`, `.bash_aliases`, `.profile` | funciones |
| Go | `go` | `.go` | tipos, funciones, y métodos nombrados por su receptor (`Server.Start`) |
| Rust | `rust` | `.rs` | módulos, structs, enums, traits, bloques `impl`, funciones |
| Python | `python` | `.py`, `.pyi`, `.pyw` | clases y funciones, también las anidadas |
| HTML | `html` | `.html`, `.htm`, `.xhtml` | elementos como `tag#id.clase`, con `[n]` entre hermanos del mismo tag; `html` y `body` ocultos; desde 1.1, el código y el JSON-LD de los `<script>` ([abajo](#los-scripts-de-html-cambiados-en-11)) |
| XML | `xml` | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.rss`, `.atom`, `.plist`, `.wsdl`, `.xlf`, `.xliff`, `.kml`, `.gpx`, `.csproj`, `.vbproj`, `.fsproj`, `.nuspec`, `.resx`, `.xaml`, y los archivos que empiezan con `<?xml` | elementos por tag, con `[n]` entre hermanos del mismo tag |
| CSV | `csv` | `.csv` | `row N › Columna`; el delimitador (`,` `;` tab `\|`) se lee de la primera línea |
| TSV | `tsv` | `.tsv` | `row N › Columna` |
| SubRip | `srt` | `.srt` | cues: `cue 12 00:01:02,500` |
| WebVTT | `vtt` | `.vtt` | cues: `cue intro 00:00.000` |
| ELF | `elf` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre | imagen › sección › símbolo, en el offset hexadecimal |
| PE | `pe` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre | imagen › sección › símbolo, en el offset hexadecimal |
| Mach-O | `macho` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre; también los fat (universales) | imagen › sección › símbolo, en el offset hexadecimal |
| PDF | `pdf` | `.pdf`, y los archivos que empiezan con `%PDF-` | marcadores como capítulos, y páginas |
| Word | `docx` | `.docx`, `.docm`, `.dotx`, `.dotm` | títulos y tablas |
| Excel | `xlsx` | `.xlsx`, `.xlsm`, `.xltx`, `.xltm` | hojas y celdas |
| PowerPoint | `pptx` | `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm` | diapositivas y sus notas |
| OpenDocument texto, planilla, presentación | `odt`, `ods`, `odp` | `.odt`, `.ott`; `.ods`, `.ots`; `.odp`, `.otp`; y los archivos cuyo `mimetype` lo dice | como Word, Excel y PowerPoint |
| EPUB | `epub` | `.epub`, y los archivos cuyo `mimetype` lo dice | capítulos, nombrados por la tabla de contenidos, y títulos |
| SQLite | `sqlite` | `.sqlite`, `.sqlite3`, `.db`, `.db3`, `.s3db`, `.sl3`, y los archivos que empiezan con `SQLite format 3` | tablas, filas y columnas |
| AsciiDoc | `asciidoc` | `.adoc`, `.asciidoc` | título del documento, títulos, bloques de código y literales |
| reStructuredText | `rst` | `.rst`, `.rest` | títulos, bloques de código, directivas |
| Org-mode | `org` | `.org` | títulos (con sus palabras TODO y etiquetas), bloques de código y de ejemplo, columnas de tabla |
| LaTeX | `latex` | `.tex`, `.ltx`, `.latex` | título, de `\part` a `\subparagraph`, entornos, entornos de código |
| Jupyter | `jupyter` | `.ipynb` | títulos de las celdas Markdown, celdas, salidas, claves |
| INI | `ini` | `.ini`, `.cfg`, `.cnf`, `.inf`, `.desktop`, `.gitconfig`, `.gitmodules`, `.editorconfig`, `.npmrc`, `.pypirc`, `.flake8`, `.pylintrc`, `.coveragerc`, `.hgrc`, `**/.git/config`, `**/git/config` | secciones y claves |
| Unidades de systemd | `systemd` | `.service`, `.socket`, `.timer`, `.mount`, `.automount`, y `**/systemd/**/*.{path,target,slice,scope,swap,network,netdev,link,conf}` | secciones y claves |
| Properties de Java | `properties` | `.properties` | claves, decodificadas |
| dotenv | `env` | `.env`, `*.env`, `.env.*` | variables |
| Configuración de SSH | `ssh-config` | `ssh_config`, `sshd_config`, `**/.ssh/config`, `**/.ssh/config.d/*`, `**/ssh/ssh_config.d/*.conf`, `**/ssh/sshd_config.d/*.conf` | secciones `Host` y `Match`, palabras clave |
| nginx | `nginx` | `nginx.conf`, `**/nginx/**/*.conf`, `**/nginx/sites-available/*`, `**/nginx/sites-enabled/*` | bloques (`server` nombrado por su `server_name`), directivas |
| Apache | `apache` | `httpd.conf`, `apache2.conf`, `.htaccess`, `**/apache2/**/*.conf`, `**/httpd/**/*.conf`, `**/apache2/sites-available/*`, `**/apache2/sites-enabled/*` | secciones (`VirtualHost` nombrado por su `ServerName`), directivas |
| Terraform y HCL | `hcl` | `.tf`, `.tfvars`, `.hcl`, `.nomad` | bloques con sus etiquetas, atributos, claves de objetos, elementos de listas |
| Dockerfile | `dockerfile` | `Dockerfile`, `Containerfile`, `dockerfile`, `.dockerfile`, `.containerfile`, `Dockerfile.*`, `Containerfile.*` | etapas, instrucciones |
| Makefile | `make` | `Makefile`, `makefile`, `GNUmakefile`, `BSDmakefile`, `.mk`, `.mak`, `.make`, `Makefile.*` | reglas, variables, `define`, condicionales |
| justfile | `just` | `justfile`, `Justfile`, `.justfile`, `.just` | recetas, variables, alias |
| SQL | `sql` | `.sql`, `.ddl`, `.dml` | sentencias, columnas de `CREATE TABLE`, filas y columnas de `INSERT` y `COPY` |
| Diffs y parches | `diff` | `.diff`, `.patch`, `.rej` | commits, archivos, hunks |
| Vue | `vue` | `.vue` | elementos del template, funciones del script, `style` |
| Svelte | `svelte` | `.svelte` | elementos, bloques `{#if}` / `{#each}`, funciones del script |
| Blade | `blade` | `.blade.php` | elementos, directivas de a pares (`@foreach`, `@section`) |
| GraphQL | `graphql` | `.graphql`, `.gql`, `.graphqls` | definiciones, campos |
| Protobuf | `protobuf` | `.proto` | mensajes, enums, servicios, `rpc`, `oneof`, campos |
| HAR | `har` | `.har` | entradas nombradas por método y URL, encabezados, cookies y parámetros por su nombre |
| JPEG, PNG, WebP, TIFF | `jpeg`, `png`, `webp`, `tiff` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre | segmentos y chunks, IFD y etiquetas EXIF, elementos XMP, en el offset hexadecimal |
| ID3 y MP4 | `id3`, `mp4` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre | frames ID3; cajas MP4, en el offset hexadecimal |
| WebAssembly | `wasm` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre | secciones, exports, imports, funciones, segmentos de datos, en el offset hexadecimal |
| Clase de Java | `java-class` | los archivos cuyos primeros bytes lo dicen, con cualquier nombre | clase, métodos, campos, constant pool, en el offset hexadecimal |
| Mail | `eml` | `.eml` | campos del encabezado, cuerpo, partes MIME y adjuntos |
| mbox | `mbox` | `mbox`, `.mbox`, `.mbx`, y los archivos sin extensión que empiezan con `From ` | mensajes por asunto, campos del encabezado, partes |
| iCalendar | `icalendar` | `.ics`, `.ical`, `.ifb` | componentes nombrados por su `SUMMARY`, propiedades |
| vCard | `vcard` | `.vcf`, `.vcard` | tarjetas nombradas por su `FN`, propiedades |
| BibTeX | `bibtex` | `.bib` | entradas por clave, campos |
| WhatsApp | `whatsapp` | `_chat.txt`, `*WhatsApp*.txt` | días, mensajes por hora y remitente |
| Telegram JSON | `telegram` | `**/ChatExport_*/result.json`, `**/DataExport_*/result.json` | chats, mensajes por fecha y remitente, claves |
| Telegram HTML | `telegram-html` | `**/ChatExport_*/messages*.html` | mensajes por fecha y remitente |

Los Formatos de código los parsean Grammars de tree-sitter compiladas dentro de `s4`; YAML,
JSON y TOML, parsers nativos; HTML, XML, CSV, TSV, los subtítulos y todos los Formatos que suma
1.1, escáneres propios de s4; los archivos objeto, goblin y lectores propios de s4; y los
documentos, Extractores incluidos en `s4`, escritos en Rust, sin ningún programa externo.

## Los Formatos de 1.0

### Datos y código

La misma ruta de claves en un archivo YAML y en uno JSON:

```
$ s4 needle.example data --flat --sort path
data/config.json:8: server › ports[2] › host │       {"host": "needle.example", "port": 8443},
data/config.yaml:9: server › ports[2] › host │     - host: needle.example
```

`ports[2]` es el tercer elemento del array que guarda `ports`, así que `ports` mismo queda
fuera del Trail. En el código, la salida de una línea imprime nombres y el árbol imprime la
línea que declara cada Segmento:

```
$ s4 needle code/cart.php --tree
code/cart.php
  class Cart
    public function addItem(Item $item, int $qty = 1): void
      10:        // needle: merge equal items
  15:function needle_helper(): string
    17:    return "needle";
```

```
$ s4 needle code/cart.php --flat
10: Cart › addItem │         // needle: merge equal items
15: function needle_helper(): string
17: needle_helper │     return "needle";
```

Un resultado en el nombre de una función, como el de la línea 15, cae fuera de esa función,
igual que un resultado en la línea de un título cae fuera de su sección.

### HTML y XML

En HTML y XML el Trail es la ruta de elementos:

```
$ s4 needle markup/page.html --flat
4: head › title │   <title>Pricing needle</title>
5: head › script │   <script>if (a < b) { label = "</div> needle"; }</script>
12: nav.top.menu › ul › li[3] │       <li><a href="/needle">Needle</a>
19: main#content › section#pricing.card.wide › p.note[2] │       <p class="note">The needle plan is free.
23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9
26: main#content › div › div › div │     <div><div><div>deep needle</div></div></div>
27: main#content │     <img src="needle.png" alt="">
```

`html` y `body` contienen la página entera, así que los Trails empiezan debajo de ellos.
`li[3]` es el tercer `li` de su `ul`, un elemento con `id` no lleva índice, y los tags de
cierre que HTML permite omitir (`<li>`, `<td>`, `<p>`) se deducen. Un resultado dentro de un
tag de apertura, como el de la línea 27, pertenece al elemento padre. En XML, la primera
construcción mal formada termina la estructura, y los resultados que vienen después se
imprimen con el Trail vacío.

### Los scripts de HTML, cambiados en 1.1

Desde 1.1 un resultado dentro de un `<script>` de una página lleva el Trail del código del
script debajo del elemento `script`, donde 1.0 se detenía en el elemento. Es el único cambio a
los Trails de un Formato de 1.0, y lo registra
[`docs/rg-compat.md`](rg-compat.md#trails-that-changed-since-10):

```
$ s4 needle code-c/m6-scripts.html --flat
4: head › script[1] › onClick │   return 'needle';
8: head › script[2] › load │ export function load() { return 'needle'; }
11: head › script[3] › offers › price │ {"@type":"Product","offers":{"price":"needle"}}
14: head › script[4] │ function tpl() { needle }
```

En 1.0 la primera línea era `4: head › script[1] │   return 'needle';`. Un script sin `type`,
con un tipo de JavaScript (`text/javascript` y sus alias) o `module` se lee con la Grammar de
JavaScript, o de TypeScript o TSX cuando su `lang` lo dice; `application/ld+json` se lee como
JSON, con sus rutas de claves. Tres límites conservan el Trail de 1.0, el elemento `script`
solo: cualquier otro tipo (`text/x-template` en la línea 14, `text/plain`, `importmap`), un
script de más de 1 MiB (`script_max_bytes`) y un script con una línea de más de 1 000 bytes
(`script_max_line`), que es código minificado. `--no-trail` imprime lo mismo que en 1.0. Una
Definición de usuario que copia `html.toml` con `scripts = false` devuelve los Trails de 1.0.

### CSV y TSV

En CSV y TSV el Trail es la fila y el encabezado de la columna:

```
$ s4 needle table/plans.csv --flat
3: row 2 › Plan │ 2,Bea,needle,"likes ""quotes"", and commas"
5: row 3 › Notes │ lines with needle"
7: row 4 › Notes │ 4,Dan,team,needle at the end,extra
```

`row 2` es el segundo registro después del encabezado. La línea 5 es la segunda línea de un
campo entre comillas de la fila 3, así que sigue estando en la fila 3.

### Subtítulos

En los subtítulos el Trail es el cue, nombrado por su número (o su identificador de WebVTT) y
su tiempo de inicio tal como está escrito; el árbol muestra la línea de tiempos completa:

```
$ s4 needle subs/movie.srt --tree
subs/movie.srt
  cue 1 00:00:01,000 --> 00:00:03,500
    3:Where is the needle?
  cue 2 00:00:04,000 --> 00:00:06,000
    8:- A needle, in there?
  cue 3 00:01:02,500 --> 00:01:04,000
    12:<i>needle</i>
```

### Archivos objeto

Un archivo ELF, PE o Mach-O se reconoce por sus primeros bytes, tenga el nombre que tenga
(`libc.so.6`, `ls`). Donde ripgrep imprimiría `binary file matches`, s4 imprime una fila por
cada cadena imprimible que tiene resultados, en el offset hexadecimal de su primer resultado. El
Trail es la imagen (el Formato y la arquitectura), la sección, y el símbolo más cercano en el
resultado o antes de él, con la distancia hasta ese símbolo:

```
$ s4 needle m4/bin/app --flat
0x68: ELF x86-64 › .rodata › main.version+0xe │ version=1.2.3-needle
0x77: ELF x86-64 › .rodata │ another needle here
```

```
$ s4 needle m4/bin/app --tree
m4/bin/app
  ELF x86-64
    .rodata
      main.version+0xe
        0x68:version=1.2.3-needle
      0x77:another needle here
```

La segunda cadena cae después del final de `main.version`, así que su Trail termina en la
sección, como todos los Trails de un binario sin símbolos (stripped). Un PE y cada arquitectura
de un Mach-O fat tienen los mismos tres Segmentos:

```
$ s4 needle m4/bin/app.exe m4/bin/app-mac --flat --sort path
m4/bin/app.exe:0x410: PE x86-64 › .rdata › version_string+0xe │ version=1.2.3-needle
m4/bin/app-mac:0x1141: Mach-O x86-64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle
m4/bin/app-mac:0x2141: Mach-O arm64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle
```

Sobre un binario real, el `/bin/ls` de esta máquina:

```
$ s4 --flat -m3 'GLIBC_2\.3' /bin/ls
0x14af: ELF x86-64 › .dynstr │ GLIBC_2.38
0x14c5: ELF x86-64 › .dynstr │ GLIBC_2.33
0x14fb: ELF x86-64 › .dynstr │ GLIBC_2.34
```

Las filas aparecen donde ripgrep encuentra un resultado en un archivo binario: un archivo
nombrado en la línea de comandos, cualquier archivo con `--binary`, o cualquier archivo con
`-a`. Un archivo objeto recorrido sin `--binary` se detiene en su primer byte NUL y no imprime
nada, como en ripgrep. Con `-v`, `-c`, `-l`, `-q` y `--no-trail` el archivo imprime lo que
imprime ripgrep. Una cadena con varios resultados es una sola fila, y `-m N` se queda con las
primeras N filas. Los símbolos de Rust se muestran decodificados (demangled) y sin su hash
(`core::fmt::write`).

### Documentos

Por defecto, los archivos PDF, Word, Excel, PowerPoint, OpenDocument, EPUB y SQLite se buscan
en el texto que les sacan los Extractores incluidos en s4, igual en una terminal que en un pipe. Cada párrafo, título, ítem de lista o
fila de tabla es una línea, con las celdas de una fila separadas por tabs, y cada resultado
lleva el Trail de la estructura del documento:

```
$ s4 needle m4/report.docx --tree
m4/report.docx
  # 1 Intro
    2:The intro needle.
    ## 1.1 Scope
      4:Scope needle.
  # 2 Costs
    table 1
      row 2
        column «Price»
          7:Pens	1.50 needle
```

Un título de Word o de OpenDocument se anida como uno de Markdown, y una tabla es
`table N › row R`, con la columna nombrada por la celda de la primera fila. Una celda de planilla
es `Hoja › celda`, una diapositiva es `slide N`, y sus notas del orador son `slide N › notes`:

```
$ s4 needle m4/budget.xlsx --flat
3: Budget › A3 │ needle pins	2
```

```
$ s4 needle m4/deck.pptx --flat
2: slide 1 │ a needle margin
3: slide 1 › notes │ tell the needle joke
5: slide 2 │ find the needle
```

Los marcadores de un PDF son capítulos que contienen sus páginas, y cada página es `page N`. Los
capítulos de un EPUB se nombran por su índice. Una fila de SQLite es
`tabla › row N › columna`, con sus valores separados por tabs:

```
$ s4 needle m4/report.pdf --tree
m4/report.pdf
  1 Intro
    page 2
      3:intro needle
  2 Costs
    page 3
      5:costs needle
```

```
$ s4 ana.needle docs/shop.sqlite --flat
12: users › row 12 › email │ 12	User 12	ana.needle@example.com	18.0
```

Cada documento de la búsqueda se extrae, tenga o no un resultado, así que un árbol con muchos
documentos tarda más que ripgrep, y `--no-trail` imprime las líneas de su texto donde ripgrep
imprime lo que encuentra en sus bytes. Con `--no-docs`, estos Formatos nunca se detectan: los
archivos se buscan como los busca ripgrep, sin costo extra, e imprimen lo que imprime ripgrep.
Sirve cuando sobre un árbol lleno de documentos importa más la velocidad que su texto, o cuando
un script necesita la salida exacta de ripgrep (`s4 --no-docs --no-trail` imprime lo mismo que
`rg`). `--docs` vuelve a prender el comportamiento por defecto, y entre `--docs` y `--no-docs`
gana el último que se pasa:

```
$ s4 --no-docs needle m4/report.pdf
binary file matches (found "\0" byte around offset 1136)
```

Un documento que el Extractor no puede leer da un aviso, que `--no-messages` silencia, y después
se busca como lo busca ripgrep; el código de salida sigue siendo el de ripgrep. Acá
`broken.docx` empieza con los primeros bytes de un zip sin ser un zip, y el resultado se cuenta
en sus bytes:

```
$ s4 -c needle m4/locked.pdf m4/broken.docx
s4: m4/locked.pdf: pdf: encrypted, and the empty password does not open it
s4: m4/broken.docx: docx: not a zip file the reader can open (invalid Zip archive: Could not find EOCD)
m4/broken.docx:1
```

Cada documento se lee en un proceso aparte, el mismo `s4` corrido de nuevo, nunca un programa
externo. Ese proceso puede reservar 1 024 MiB, una cuenta que lleva él mismo, y puede tardar
60 s en un documento. Pasado cualquiera de los dos límites, el documento da un aviso
(`reading it needed more than the 1024 MiB a reader may use`, o `reading it took more than
60 s`) y se busca como lo busca ripgrep. La cuenta es el mismo código en Linux, macOS y
Windows, y cada sistema suma una segunda barrera 64 MiB por encima: `RLIMIT_DATA` en Linux y
macOS (donde macOS lo acepta), y un límite de memoria de Job Object en Windows. La CI corre la
cuenta en los cuatro sistemas, y el límite del Job Object en Windows. Contra las bombas de
descompresión, una parte de un zip se descomprime hasta 256 MiB y un documento entero hasta
1 GiB, un stream de PDF hasta 64 MiB, y de un documento se sacan como mucho 256 MiB de texto.

- Sólo los archivos regulares son documentos: stdin y los pipes se buscan como los busca
  ripgrep.
- La búsqueda en documentos es independiente de `-z`, `-a` y `--binary`. Un archivo que lee
  `--pre` o `-z` nunca se extrae, así que `report.pdf.gz` con `-z` se busca en sus bytes
  descomprimidos.
- `--within` usa todos los Segmentos, y `--expand` imprime la Sección del último: la fila de
  una celda, la página de un resultado en un PDF. Los nombres de página, hoja, diapositiva,
  tabla, fila, columna, celda, capítulo y notas quedan fuera del texto, así que
  `--in-headings` sólo encuentra títulos en los documentos.
- Un PDF cifrado que abre con la contraseña vacía se lee. El texto dibujado como imagen
  (escaneos) queda sin texto, porque no hay OCR. Un objeto de un PDF con un entero de más de 64
  bits queda afuera del lector de PDF, y con él el texto que dibuja, sin aviso.
- Quedan sin leer los formatos binarios de Office (`.doc`, `.xls`, `.ppt`), los encabezados,
  pies de página, notas al pie y comentarios de los documentos de Office, y el archivo `-wal`
  de una base SQLite.
- Un archivo zip nunca se abre.


## Documentos de texto

AsciiDoc, reStructuredText, Org-mode y LaTeX tienen los Segmentos de Markdown: el título del
documento, los títulos con su nivel y los bloques de código nombrados por su lenguaje. Cada
título y cada bloque de código lleva su línea tal como está escrita como `signature`, que el
árbol muestra:

```
$ s4 needle docs-a/m6.adoc --tree
docs-a/m6.adoc
  Deploy guide
    == Install
      === Linux
        [source,bash]
          6:apt install needle-pkg
          7:== needle literal heading
```

La Sección de un título llega hasta el siguiente título del mismo nivel o de uno más alto. Un
título dentro de un bloque de código, literal o de comentario es texto, como muestra la línea 7.
Estos cuatro Formatos no tienen errores de sintaxis: un bloque que queda abierto llega hasta el
final del archivo, como un fence de Markdown sin cerrar. `include::`, `\input` e `\include`
nunca se siguen.

### AsciiDoc

`= Título` al principio es el título del documento, y de `==` a `======` son los niveles 1 a 5,
igual que los títulos `#` al estilo Markdown que acepta Asciidoctor; un título necesita un
espacio después de sus marcas. Un listado (`----`) después de `[source,LENG]` es un Segmento de
código nombrado `LENG`, y si no, `listing`; un bloque literal (`....`) es `literal`, y un fence
(```` ``` ````) se nombra por su info string. Los bloques de comentario, passthrough, sidebar,
ejemplo y cita contienen texto. Los títulos de dos líneas de la sintaxis vieja de AsciiDoc son
texto, y las tablas no dan Segmento de columna.

```
$ s4 needle docs-a/m6.adoc --flat
6: Deploy guide › Install › Linux › bash │ apt install needle-pkg
7: Deploy guide › Install › Linux › bash │ == needle literal heading
```

### reStructuredText

Un título es una línea subrayada, y opcionalmente sobrerrayada, por un mismo carácter de
puntuación repetido. Los niveles siguen el orden en que aparece por primera vez cada estilo,
como los asigna docutils; s4 nunca promueve una única sección de arriba a título del documento.
`.. code-block:: LENG`, `.. code::` y `.. sourcecode::` son Segmentos de código nombrados
`LENG`; un párrafo que termina en `::` abre un bloque `literal`; cualquier otra directiva es un
Segmento `directive` nombrado por su tipo. Las tablas no dan Segmento de columna; las
sustituciones y los roles son texto.

```
$ s4 needle docs-a/m6.rst --flat
10: Install › Linux › bash │    apt install needle-pkg
14: Install › Linux › note │    needle note
```

### Org-mode

`*` con cualquier profundidad es un título de ese nivel, con su palabra TODO, su prioridad y
sus etiquetas en el nombre, así que `--in-headings` las encuentra; `#+TITLE:` es el título del
documento. `#+BEGIN_SRC LENG` es un Segmento de código nombrado `LENG` (`src` si no tiene), y
`#+BEGIN_EXAMPLE` es `example`. Una tabla cuya primera fila va seguida de una regla
(`|---+---|`) nombra sus columnas, como una tabla de Markdown.

```
$ s4 needle docs-a/m6.org --flat
5: Budget › TODO Costs :work: › Price │ | Hosting | needle 9 |
7: Budget › TODO Costs :work: › bash │ * needle text
10: Budget › TODO Costs :work: › example │ needle example
```

### LaTeX

De `\part` a `\subparagraph`, con asterisco o sin él, son los niveles 1 a 7, nombrados por su
título entre llaves; `\title{…}` es el título del documento. `\begin{X}` … `\end{X}` es un
Segmento de entorno nombrado `X`; `lstlisting` con `[language=L]` y `minted` con `{L}` son
Segmentos de código nombrados `L`, y `verbatim`, `Verbatim` y `comment` son Segmentos de código
en los que nada se lee como comando. Los entornos de `hidden_environments` de la Definición
(`document`, `itemize`, `enumerate`, `description`) se ocultan, como el `body` de HTML. Un
comentario `%` es texto, y también una macro que se expande a un título.

```
$ s4 needle docs-a/m6.tex --flat
6: A Study of Needles › Methods › Sampling › figure │ \caption{needle distribution}
9: A Study of Needles › Methods › Sampling › bash │ \section{needle text}
```

### Notebooks de Jupyter

Un notebook es JSON leído con la vista `jupyter` (`view = "jupyter"`): los títulos de sus
celdas Markdown contienen las celdas de abajo, cada celda es `cell N`, desde 1, y cada salida
`output N`. Las claves `cells`, `source`, `outputs`, `data` y `text` se ocultan; las claves MIME
de una salida se conservan. `cell N` y `output N` son palabras de s4, así que `--in-headings`
encuentra sólo los títulos y las claves que se conservan. Un archivo sin un array `cells`
conserva los Trails de JSON, y un error de sintaxis de JSON sigue la regla de JSON.

```
$ s4 needle docs-a/m6.ipynb --flat
4: Analysis › Load data › cell 2 │   {"cell_type":"code","source":["df = load('needle.csv')\n"],
5: Analysis › Load data › cell 2 › output 1 › text/plain │    "outputs":[{"output_type":"execute_result","data":{"text/plain":["needle"]}}]},
6: Analysis › Load data › cell 3 │   {"cell_type":"raw","source":["needle raw"]}
```

## Configuración e infraestructura

### INI y unidades de systemd

`[sección]` abre una sección hasta el encabezado siguiente, nombrada por el texto entre los
corchetes tal como está escrito (`[remote "origin"]` es `remote "origin"`, y `[a.b]` una sola
sección `a.b`); `clave = valor` y `clave: valor` son claves. `;` y `#` empiezan comentarios, y
`\` al final de una línea la continúa. Una línea con sangría continúa la clave de arriba cuando
no tiene ni `=` ni `:`, y es una clave propia si tiene alguno, así que las claves con sangría de
`.gitconfig` son claves y los valores de varias líneas de `configparser` son valores. INI no
tiene errores de sintaxis: una línea que no es ni encabezado ni clave es texto de lo que tiene
arriba. Las unidades de systemd son INI con su propia Definición, `systemd`.

```
$ s4 needle config/m6.ini --flat
3: server › host │ host = needle.example
4: server › host │   continued needle
7: remote "origin" › url │ url=needle.git
9: a.b › k │ k=needle
```

```
$ s4 needle config/m6.service --flat
4: Service › ExecStart │ ExecStart=/usr/bin/needle \
```

### `.properties`

Una clave termina en el primer `=`, `:` o espacio sin escapar, y se decodifica (`\:`, `\=`,
`\ `, `\uXXXX`): `--json`, `--within` e `--in-headings` ven `café:key` para `café\:key`.
Una clave con puntos es un solo Segmento, porque el formato no tiene jerarquía. `#` y `!`
empiezan comentarios.

```
$ s4 needle config/m6.properties --flat
2: spring.datasource.url │ spring.datasource.url=jdbc:needle://db
3: café:key │ café\:key : needle\
```

### `.env`

`CLAVE=valor` y `export CLAVE=valor`; un valor entre comillas puede ocupar varias líneas, y la
Sección de su clave las cubre; `#` empieza un comentario fuera de las comillas. `.env`,
`.env.production` y `app.env` son `env`.

```
$ s4 needle config/.env.m6 --flat
2: DATABASE_URL │ export DATABASE_URL=postgres://needle@db/app
4: TEXT │ needle # literal
```

### Configuración de SSH

`Host …` y `Match …` abren una sección nombrada por su línea tal como está escrita, hasta la
siguiente; cualquier otra línea es una palabra clave, comparada sin distinguir mayúsculas como
lo hace ssh. `Include` nunca se sigue. Un archivo llamado `config` es de SSH sólo dentro de
`.ssh/`.

```
$ s4 needle config/ssh/ssh_config --flat
3: Host github.com gitlab.com › IdentityFile │   IdentityFile ~/.ssh/needle_ed25519
5: Match user ana › ProxyCommand │   ProxyCommand=needle --proxy
6: Match user ana › Include │ Include /never/read/needle
```

### nginx

Un bloque (`location /api`, `upstream backend`) se nombra por su directiva y sus argumentos tal
como están escritos, y un bloque `server` por `server` y su primer `server_name`, leído de
cualquier lugar del bloque. Una directiva es un Segmento de clave hasta su `;`. Se leen los
comentarios `#` y los strings entre comillas, así que una llave dentro de un string no es
estructura. Un `}` que no cierra nada termina la estructura: los resultados que vienen después
tienen el Trail vacío. Un bloque que sigue abierto al final del archivo termina ahí. Un `.conf`
es de nginx sólo debajo de un directorio `nginx/`; en cualquier otro lado sigue siendo Plain.

```
$ s4 needle config/nginx/nginx.conf --tree
config/nginx/nginx.conf
  http
    server example.com
      location /api
        proxy_pass
          4:   proxy_pass http://needle:8080;
```

### Apache

Las secciones `<Nombre args>` … `</Nombre>` se anidan, comparadas sin distinguir mayúsculas; un
`VirtualHost` se nombra con su primer `ServerName`. Una directiva es una línea, que `\`
continúa. Un tag de cierre que no corresponde a ninguna sección abierta se ignora, y una sección
abierta termina con el archivo.

```
$ s4 needle config/apache2/apache2.conf --flat
3: VirtualHost *:443 example.com › Directory /var/www › Require │   Require ip 10.0.0.0/8 # needle
```

### Terraform y HCL

Un bloque se nombra por su tipo y sus etiquetas sin comillas (`resource aws_instance web`); un
atributo es una clave, un valor objeto contiene claves y una lista contiene elementos `[N]`,
desde 0, como en JSON. Se leen los comentarios (`#`, `//`, `/* */`), los strings con `${…}` y
`%{…}`, y los heredocs, así que el `${not_a_block}` de la línea 10 no abre nada. Un `}` suelto
termina la estructura, como en nginx.

```
$ s4 needle config/m6.tf --flat
3: resource aws_instance web › tags › Name │   Name = "needle-web"
7: resource aws_instance web › ingress[1] › name │   { name = "needle" }
10: resource aws_instance web › script │   echo needle ${not_a_block}
```

### Dockerfile

Cada `FROM` abre una etapa nombrada por su nombre de `AS`, o `stage N` (desde 0, la numeración
de Docker para `--from=N`), hasta el `FROM` siguiente. Cada instrucción se nombra por su palabra
clave, sobre sus líneas de continuación y sus heredocs; el carácter de escape es `\` o el que
fija `# escape=`. `--expand` imprime una instrucción entera:

```
$ s4 needle config/m6.dockerfile --flat
5: build › RUN │  needle-build --release
7: build › RUN │ needle heredoc
10: stage 1 › COPY │ COPY --from=build /needle /app
```

```
$ s4 needle config/m6.dockerfile --flat --expand --within build
3-5: build › RUN
RUN echo start \
 # continuation comment
 needle-build --release

6-8: build › RUN
RUN <<EOF
needle heredoc
EOF
```

`stage 1` es una palabra de s4, así que `--in-headings` encuentra `build` y nunca `stage`.

### Makefile

Una regla se nombra por sus targets tal como están escritos, sobre su receta; una variable (`=`,
`:=`, `?=`, `+=`, `!=`, con `override` o `export`) y `define NOMBRE` … `endef` son claves;
`ifeq` … `endif` y sus hermanos son Segmentos condicionales nombrados por su línea, que
contienen lo que tienen adentro. `$(…)` se saltea al buscar el `:` de una regla. Un `endif` o
`endef` que no cierra nada es texto, y uno abierto termina con el archivo.

```
$ s4 needle config/m6.mk --flat
3: ifeq ($(OS),Windows_NT) › release test │ 	@echo needle
6: ifeq ($(OS),Windows_NT) › release test │ 	echo needle again
8: ifeq ($(OS),Windows_NT) › NAME │ NAME := needle
11: BODY │ needle body
15: %.o │ >echo needle pattern
```

### justfile

Una receta se nombra por su nombre, sobre su cuerpo con sangría; una variable o un `alias` es
una clave. Los comentarios de arriba de una receta son su documentación y quedan afuera de ella,
como muestra la línea 1.

```
$ s4 needle config/m6.just --flat
1: # needle documentation
4: deploy │     rsync -a build/ host:/srv/needle/
6: URL │ URL := 'needle'
7: ship │ alias ship := deploy # needle
```

## Al lado del código

### SQL

Un solo dialecto permisivo lee scripts de PostgreSQL, MySQL, SQLite y SQL Server y sus dumps.
Una sentencia termina en un `;` fuera de strings, identificadores entre comillas, dollar quotes
y comentarios, en una línea `GO`, o en el delimitador que fija una línea `DELIMITER`, y se
nombra por sus palabras clave iniciales y su objeto. En `CREATE TABLE … ( … )` cada columna es
un Segmento. En `INSERT … VALUES`, cada tupla es `row N`, desde 1 dentro de su sentencia, y la
columna de su valor se nombra por la lista de columnas de la sentencia, si no por el
`CREATE TABLE` anterior de la misma tabla, y si no `column K`. Las filas de `COPY … FROM stdin`
de PostgreSQL se leen igual, una por línea:

```
$ s4 needle code-c/m6-dump.sql --flat
2: INSERT INTO users › row 2 › email │ INSERT INTO users VALUES (1,'plain'),(2,'needle@example.com');
3: INSERT INTO users › row 1 › email │ INSERT INTO users (email,id) VALUES ('needle',3);
6: COPY public.users › row 2 › email │ 2	needle@example.com
8: INSERT INTO unknown › row 1 › column 2 │ INSERT INTO unknown VALUES (1,'needle');
```

```
$ s4 needle code-c/m6-schema.sql --flat
3: CREATE TABLE users › email │  email text NOT NULL, -- needle
6: CREATE OR REPLACE VIEW active_users │ CREATE OR REPLACE VIEW active_users AS SELECT 'needle' AS label;
```

`row N` y `column K` son palabras de s4, así que `--in-headings row` no encuentra nada. Un dump
de gigabytes cuesta sólo el índice de sus sentencias: s4 arma filas y columnas para las
sentencias que tienen resultados, y no lee más allá de la sentencia del último resultado. Un
string o un comentario que queda abierto termina la estructura en su sentencia. El cuerpo de un
`CREATE PROCEDURE` sin `DELIMITER` se parte en sus `;` internos, y cada pedazo conserva el
nombre de la sentencia que empieza.

### Diffs y parches

Diffs unificados, diffs combinados, `git log -p` y series de `git format-patch`. Un commit se
nombra por su hash corto y su asunto, un archivo por su ruta nueva (la vieja si se borra), y un
hunk por `+N` y su texto de contexto. Un resultado en una línea `diff --git`, `---` o `+++`
tiene el Trail de arriba del archivo, y uno en el encabezado de un hunk está en el archivo. Las
líneas del mensaje de un commit quedan afuera del commit, como muestra la línea 4:

```
$ s4 needle code-c/m6.patch --flat
4:     Fix the needle crash
10: 3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe( │ +    let needle = 1;
```

stdin no tiene nombre del que detectar un Formato, así que se lo nombra con `--format`:

```
$ git diff | s4 --format diff needle
```

Un hunk cuyas cuentas pasan el encabezado siguiente termina en ese encabezado.

### Vue, Svelte y Blade

Los componentes de un solo archivo son HTML, leído con las reglas de HTML. El código de un
`<script>` lleva los Trails de la Grammar que nombra su `lang` (ninguno o `js` JavaScript, `ts`
TypeScript, `tsx` TSX) debajo del elemento `script`; un script con otro `lang` conserva el
elemento solo, y `<style>` es texto.

```
$ s4 needle code-c/m6.vue --flat
2: template › ul.items › li[2] │ <ul class="items"><li>x</li><li>needle template</li></ul>
6: script › addItem │   const value: string = 'needle';
9: style │ <style>.needle { color:red }</style>
```

`{#if}`, `{#each}`, `{#await}`, `{#key}` y `{#snippet}` de Svelte abren un bloque nombrado por su
tag sin llaves, que cierra su `{/…}`:

```
$ s4 needle code-c/m6.svelte --flat
3: ul › #each items as item › li[1] │ <li>{item.needle}</li>
5: ul › #each items as item › li[2] │ <li>needle empty</li>
```

Las directivas de a pares de Blade (`@section`, `@if`, `@foreach`, `@php`…) abren un bloque
nombrado por la directiva y su argumento, hasta su `@end…`; `{{-- … --}}` es un comentario.
`cart.blade.php` es Blade por su extensión compuesta, y `cart.php` sigue siendo PHP.

```
$ s4 needle code-c/m6.blade.php --flat
4: @section('content') › div.cart › @foreach($items as $item) › li │ <li>{{ $item->needle }}</li>
```

Un cierre de bloque sin bloque abierto es texto, como un tag de cierre de HTML.

### GraphQL y Protobuf

Las definiciones de GraphQL (`type`, `input`, `query`, `fragment`…) son Segmentos con su
palabra clave como tipo, y un campo con argumentos o con una selección, y todo campo de un tipo,
es un Segmento `field`; se leen los strings, los block strings y los comentarios `#`. El árbol
muestra el tipo antes del nombre (`type User`, `field email`). En Protobuf, `message`, `enum`,
`service`, `rpc`, `oneof` y `extend` se anidan, y un campo o un valor de enum es un Segmento
`field`. En los dos, un `}` suelto termina la estructura.

```
$ s4 needle code-c/m6.graphql --flat
2: User › email │  email: String! # needle
7: GetUser › user › posts › title │    title(needle: true)
```

```
$ s4 needle code-c/m6.proto --flat
2: User › email │  string email = 1 [json_name = "needle"];
4: User › choice › nick │   string nick = 2; // needle
8: Users › Get │  rpc Get (User) returns (User) { option note = "needle"; }
```

### Capturas HAR

Un HAR es JSON leído con la vista `har`: cada entrada se nombra por el método y la URL de su
pedido sin el esquema (cortada en 60 caracteres), los elementos de `headers`, `cookies`,
`queryString` y `postData.params` por su `name`, y los de `pages` por su `title`. La URL puede
venir después del resultado en el archivo.

```
$ s4 needle code-c/m6.har --flat
3: GET api.example.com/v1/users › response › content › text │   "response":{"content":{"text":"needle"},"status":200},
5: GET api.example.com/v1/users › request › header Authorization › value │    "headers":[{"name":"Authorization","value":"needle"}],
6: GET api.example.com/v1/users › request › cookie session › value │    "cookies":[{"name":"session","value":"needle"}],
7: GET api.example.com/v1/users › request › queryString q › value │    "queryString":[{"name":"q","value":"needle"}],
8: GET api.example.com/v1/users › request › postData › param body › value │    "postData":{"params":[{"name":"body","value":"needle"}]}}
9: Login › id │  }],"pages":[{"title":"Login","id":"needle"}]}}
```

## Imágenes, audio y binarios

Los archivos JPEG, PNG, WebP, TIFF, ID3 (MP3), MP4 (y las marcas QuickTime, 3GP, HEIF y AVIF),
WebAssembly y las clases de Java se reconocen por sus primeros bytes, con cualquier nombre, y se
leen con las reglas de los [archivos objeto](#archivos-objeto): donde rg encuentra un resultado
en un archivo binario (un archivo nombrado en la línea de comandos, `--binary` o `-a`), s4
imprime una fila por cada string imprimible que tiene resultados, en su offset hexadecimal, con
su Trail. Un archivo recorrido sin `--binary` imprime lo que imprime rg, y `-v`, `-c`, `-l`,
`-q` y `--no-trail` imprimen la salida de rg. Sólo se decoran los bytes crudos que encuentra
rg: una parte de mail en base64, un chunk PNG comprimido (`zTXt`), un frame ID3 en UTF-16 y los
streams comprimidos de una clase quedan invisibles para la búsqueda, como lo son para rg.

| Formato | Fila de ejemplo |
|---|---|
| `jpeg` | `0x26: JPEG › APP1 Exif › IFD0 › Model │ needle-cam 3000` |
| `png` | `0x31: PNG › tEXt Comment │ needle`; también `PNG › eXIf › IFD0 › Artist` y `PNG › iTXt XML:com.adobe.xmp › …` |
| `webp` | `0x2e: WebP › EXIF › IFD0 › Model │ needle` |
| `tiff` | `0x1a: TIFF › IFD0 › Model │ needle-cam 3000` |
| `id3` | `0x15: ID3v2.4 › TIT2 │ needle song` |
| `mp4` | `0x50: MP4 M4A › moov › udta › meta › ilst › ©nam │ needle` |
| `wasm` | `0x30: WebAssembly › data › data[0] │ needle not found` |
| `java-class` | `0x50: Java 17 › com.example.Cart › addItem │ needle merged` |

- **EXIF** se lee donde esté (APP1 de JPEG, `eXIf` de PNG, `EXIF` de WebP, un archivo TIFF):
  los IFD son secciones, y una etiqueta se nombra por la tabla estándar (`Model`, `Artist`), o
  `tag 0x9c9b`.
- **XMP** es XML: su paquete sigue con la ruta de elementos debajo de `rdf:Description`.
- Los frames **ID3** se nombran por su id; `TXXX` y `WXXX` suman su descripción. Un MP3 con sólo
  una etiqueta ID3v1 no tiene magic al principio y no se detecta.
- Las cajas **MP4** se recorren a través de las cajas contenedoras, y `mdat` no contiene nada.
- Las secciones de **WebAssembly** se nombran por su id, una función por la sección `name` o
  `func[N]`, y un segmento de datos `data[N]`.
- **Clases de Java**: la imagen es `Java N` según la versión de la clase; un string que carga el
  `ldc` de un método pertenece a ese método, si no al campo o al método que nombra, y si no a
  `constant pool`.
- `cafebabe` empieza tanto una clase de Java como un Mach-O fat: se prueba cada Definición cuyo
  magic coincide hasta que una lee el archivo.
- Un archivo cuyo magic coincide y cuyo layout no se lee se imprime como lo imprime rg, sin
  aviso. Los archivos comprimidos (zip, tar, jar…) nunca se abren.

## Mail, calendarios, contactos y chats

### Mail y mbox

Un archivo `.eml` es un mensaje; un mbox tiene mensajes, cada uno empezando en una línea
`From `, y cada uno nombrado por su `Subject` decodificado. Un campo del encabezado se nombra
por su nombre, sobre sus líneas plegadas; un cuerpo es `body`, y las partes de un mensaje
multipart son `part N` con la numeración de IMAP y su tipo de medio, o `attachment NOMBRE`. Los
nombres RFC 2047 y RFC 2231 se decodifican; los cuerpos nunca.

```
$ s4 needle mail/m6.eml --flat
3: Subject │ Subject: Your needle invoice
5: X-Long │  needle folded
7: body │ needle body
```

```
$ s4 needle mail/m6-mime.eml --flat
10: part 1 multipart/alternative › part 1.1 text/plain │ needle plain
16: attachment café.txt │ needle attachment
```

```
$ s4 needle mail/m6.mbox --tree
mail/m6.mbox
  Ana · Today · Budget
    body
      6:needle one
      7:>From escaped needle
  Next
    body
      12:needle two
```

Un mbox de gigabytes cuesta sólo el índice de sus mensajes, como un dump de SQL. Un boundary
que nunca se cierra termina su parte al final del mensaje. El `Inbox` y el `Sent` de
Thunderbird, que no tienen extensión, se reconocen por sus primeros bytes, `From `.

### iCalendar y vCard

`BEGIN:X` … `END:X` es un componente nombrado `X` y su `SUMMARY`, `FN` o `TZID` (`VEVENT Team
sync`, `VCARD Ana Pérez`), y si no `X[N]`; un único `VCALENDAR` alrededor del archivo se oculta.
Una propiedad se nombra sin sus parámetros, sobre sus líneas plegadas. Un `END` cierra el
componente más interno de su nombre; uno sin ninguno abierto es texto.

```
$ s4 needle config/m6.ics --flat
3: VEVENT Team sync › DESCRIPTION │ DESCRIPTION:Bring the needle
4: VEVENT Team sync › DESCRIPTION │  folded needle
```

```
$ s4 needle config/m6.vcf --flat
2: VCARD Ana Pérez › EMAIL │ EMAIL;TYPE=work:ana@needle.example
```

### BibTeX

Una entrada se nombra por su clave, y cada campo es una clave; `@string` y `@preamble` se
nombran así, mientras que `@comment` y el texto entre entradas son texto con el Trail vacío.
Una entrada cuyas llaves nunca se cierran termina en el próximo `@` al principio de una línea.

```
$ s4 needle docs-a/m6.bib --flat
2: knuth1984 › title │  title = {Literate needle {programming}},
3: knuth1984 › note │  note = "needle" # suffix,
5: @comment{needle ignored}
```

### WhatsApp

Una exportación de texto de WhatsApp se detecta por su nombre: `_chat.txt`, el archivo de
adentro del zip de la exportación, y `*WhatsApp*.txt`, porque el nombre de la aplicación está en
el nombre del archivo exportado en todos los idiomas (`WhatsApp Chat with Ana.txt`,
`Chat de WhatsApp con Ana.txt`). Los mensajes de una fecha forman un día, nombrado por la fecha
tal como está escrita, y un mensaje se nombra por su hora y su remitente:

```
$ s4 needle chat/_chat.txt --flat
1: 31/12/23 › 21:41 Ana │ 31/12/23, 21:41 - Ana: where's the needle?
2: 31/12/23 › 21:41 Ana │ continued needle
3: 31/12/23 › 21:42 │ 31/12/23, 21:42 - system needle
4: 01/01/24 › 09:00:01 Bob │ [01/01/24, 09:00:01] Bob: needle
```

Un archivo cuya primera línea no abre un mensaje no es una exportación, y sus resultados tienen
el Trail vacío. s4 nunca mira el contenido de un archivo para elegir su Formato, así que una
exportación renombrada con un nombre que la Definición no reclama se lee nombrando su Formato:

```
$ s4 --format 'chats/*.txt=whatsapp' needle chats/ana.txt
```

### Telegram

Telegram Desktop exporta un chat a una carpeta `ChatExport_<fecha>` (una cuenta entera a
`DataExport_<fecha>`), como `result.json` o `messages.html`, `messages2.html`… Esos nombres son
de Telegram sólo dentro de una carpeta así; en cualquier otro lado siguen siendo JSON y HTML.

```
$ s4 needle chat/ChatExport_m6/result.json --flat
4: Ana › 2024-01-31T10:04:12 Ana › text │   {"text":"needle","date":"2024-01-31T10:04:12","from":"Ana"},
6: Ana › 2024-01-31T10:05:00 Ana › text │    "text":["needle",{"type":"bold","text":"needle"}],
6: Ana › 2024-01-31T10:05:00 Ana › text › text │    "text":["needle",{"type":"bold","text":"needle"}],
7: Ana › 2024-01-31T10:05:00 Ana › text_entities › text │    "text_entities":[{"type":"plain","text":"needle"}]}
```

```
$ s4 needle chat/ChatExport_m6/messages.html --flat
5: 31.01.2024 10:04:12 Ana │ <div class='text'>needle</div>
9: 31.01.2024 10:05:00 Ana │ <div class='text'>needle</div>
```

Un mensaje sin nombre de remitente toma el remitente del anterior. La vista de HTML depende de
los nombres de clase de la plantilla de exportación de Telegram: sin ellos, la página conserva
los Trails de HTML.

## Las reglas de todos los Formatos

- **La búsqueda es la de rg.** Un Formato decide sólo el Trail de los resultados que encuentra
  rg: `s4 --no-docs --no-trail` imprime la salida de rg byte por byte sobre cada fixture.
- **Lo que un Formato no puede leer** conserva los Trails de los resultados de antes del punto
  donde falló la lectura y les da el Trail vacío a los de después, sin aviso y con el código de
  salida de rg; un Formato sin errores de sintaxis (INI, los documentos de texto, HTML) sigue
  leyendo.
- **Los nombres** son el texto del archivo tal como está escrito, cortado en el `name_chars` de
  la Definición (40 si no está) con `…`. Las palabras que s4 suma a un nombre (`row 2`,
  `cell 3`, `stage 1`, `part 1.1`, `[1]`) no salen de ningún archivo, así que `--in-headings`
  nunca las encuentra; un nombre decodificado (el asunto de un mail, un escape de
  `.properties`, un string de JSON) se busca decodificado.
- **Un resultado en el nombre propio de un Segmento** tiene el Trail del padre del Segmento,
  como en una línea de título; los cues conservan el suyo.
- **Secciones.** `--expand` imprime la Sección del último Segmento de un resultado, y
  `--json --expand` la lleva; para los Formatos binarios imprime las filas.
- **Codificaciones y fines de línea** como en todos los Formatos: el BOM de UTF-8 se saca, UTF-16
  se transcodifica, los bytes que no son UTF-8 se muestran como U+FFFD en los nombres, y CRLF se
  lee como LF.
- **Profundidad.** Pasados 256 Segmentos abiertos no se abren más, y ninguna profundidad de
  anidamiento hace caer a s4.
- **Parámetros** que una copia de una Definición puede cambiar: `syntax` (qué reglas aplica un
  Processor compartido), `view` (la forma del JSON o del HTML: `jupyter`, `har`, `telegram`),
  `scripts`, `script_max_bytes`, `script_max_line` y `templates` para markup, `layout` para los
  binarios, `hidden_environments` para LaTeX y `name_chars` para cualquiera. `s4 --export-formats
  DIR` escribe las Definiciones incluidas para empezar.
