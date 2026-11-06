> 🇬🇧 Read in English: [README.md](README.md)

# searchfor

`s4` es un buscador compatible con ripgrep que te dice **dónde** cae cada resultado dentro
de la estructura de su archivo. El producto se llama searchfor (el crate, el repositorio y los
directorios de configuración), y su comando es `s4`.

````
$ s4 needle-pkg md --tree
md/guide.md
  Deploy guide
    # Install
      ## Linux
        ```bash
          12:apt install needle-pkg
````

Las líneas que están arriba de `12:apt install needle-pkg` son el **Trail** del resultado:
el título del documento tomado del frontmatter, los títulos y el bloque de código en el que
cae el resultado. En el código el Trail es la clase y la función; en YAML, JSON y TOML, la ruta
de claves; en HTML y XML, la ruta de elementos; en CSV y TSV, la fila y la columna; en los
subtítulos, el cue; en un binario ELF, PE o Mach-O, la sección y el símbolo más cercano; y en un
documento (que se busca por defecto), el capítulo y la página de un PDF, el título y la tabla de un
archivo de Word, la hoja y la celda de una planilla, la diapositiva de una presentación, o la
tabla, la fila y la columna de una base SQLite. Desde 1.1 también es la sección y la clave de un
archivo de configuración (`http › server example.com › location /api`), la sentencia y la fila
de un dump de SQL, el commit, el archivo y el hunk de un parche, la etiqueta de los metadatos de
una foto (`JPEG › APP1 Exif › IFD0 › Model`), y el mensaje de un buzón de mail o de una
exportación de chat, sobre los [setenta Formatos](#formatos).

La página del proyecto es https://searchfor.ip1.cc/

Todos los ejemplos de este README son salidas reales sobre [`tests/fixtures/`](tests/fixtures),
corridas desde adentro de ese directorio. Los archivos objeto y los documentos de `m4/` los
escriben ahí los mismos writers que usa [`tests/cli_m4.rs`](tests/cli_m4.rs)
(`searchfor::processors::object::fixture` y `searchfor::docs::fixture`), con el mismo contenido.
Los ejemplos sobre `/bin/ls`, sobre el script de `--pre` de
[Compatibilidad con ripgrep](#compatibilidad-con-ripgrep) y el de
[Helper Processors](#helper-processors) dicen sobre qué corren.

## Estado

**v1.0 está preparada y todavía sin publicar; 1.1, con los Formatos de M6, está en desarrollo.**
v1.0 corre en Linux, macOS y Windows, y suma binarios precompilados, Homebrew, npm y scripts de
instalación (ver [Instalación](#instalación)); cada canal se abre cuando se aprueba su
publicación, y hasta entonces se instala desde el repositorio. 1.1 le suma cuarenta Formatos a
los de v1.0: documentos de texto (AsciiDoc, reStructuredText, Org-mode, LaTeX, Jupyter),
configuración (INI, systemd, `.properties`, `.env`, SSH, nginx, Apache, Terraform y HCL,
Dockerfile, Makefile, justfile), los archivos que viven al lado del código (SQL, diffs, Vue,
Svelte, Blade, GraphQL, Protobuf, HAR), los metadatos de imágenes, audio y binarios (JPEG, PNG,
WebP, TIFF, ID3, MP4, WebAssembly, clases de Java), y mail, calendarios, contactos, BibTeX y los
chats de WhatsApp y Telegram (ver [Formatos](#formatos)). Su único cambio a un Trail de v1.0
está dentro de los scripts de las páginas HTML
([docs/rg-compat.md](docs/rg-compat.md#trails-that-changed-since-10)).

El motor de búsqueda son los crates de ripgrep, así que la búsqueda se comporta como `rg`, y
funcionan todos los flags de ripgrep 14.1.1 (ver
[Compatibilidad con ripgrep](#compatibilidad-con-ripgrep)); encima de eso, cada Formato les da
Trails a sus resultados, y los archivos PDF, Word, Excel, PowerPoint, OpenDocument, EPUB y SQLite
se buscan por defecto en su texto, con su estructura; `--no-docs` lo apaga. Las líneas largas se
muestran como Fragmentos, y `--within`, `--expand` e `--in-headings` filtran y expanden por
estructura. `s4 --mcp` ofrece la búsqueda a los agentes de IA por el Model Context Protocol (ver
[Para agentes de IA](#para-agentes-de-ia)). Otros lenguajes se pueden describir con un
[Helper Processor](#helper-processors), un programa aparte con el que s4 se comunica. Cualquier
otro archivo es **Plain**: sus resultados se imprimen con el Trail vacío.

El glosario está en [CONTEXT.md](CONTEXT.md), el diseño en
[docs/specs](docs/specs/2026-10-16-searchfor-design.md) y las decisiones de arquitectura en
[docs/adr](docs/adr).

## Instalación

v1.0 está preparada y todavía sin publicar: cada canal de abajo se abre cuando se aprueba su
publicación, y el GitHub Release de v1.0 es el primero. Hasta entonces, se instala desde el
repositorio, que compila `s4` con Rust 1.90 o posterior:

```
cargo install --git https://github.com/neitanod/searchfor
```

Una vez publicada v1.0, cualquiera de estos instala el binario `s4`:

| Canal | Comando | Sistemas |
|---|---|---|
| Homebrew | `brew install neitanod/tap/searchfor` | macOS |
| npm | `npm install -g searchfor` | los cinco sistemas de la tabla de abajo |
| crates.io | `cargo install searchfor` | cualquier sistema con Rust 1.90 o posterior; compila desde el código |
| Script de shell | `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/neitanod/searchfor/releases/download/v1.0.0/searchfor-installer.sh \| sh` | Linux y macOS |
| Script de PowerShell | `powershell -ExecutionPolicy Bypass -c "irm https://github.com/neitanod/searchfor/releases/download/v1.0.0/searchfor-installer.ps1 \| iex"` | Windows |
| GitHub Releases | bajar el archivo de tu sistema de https://github.com/neitanod/searchfor/releases | los cinco sistemas de la tabla de abajo |

Los binarios precompilados cubren cinco sistemas:

| Sistema | Target | Archivo |
|---|---|---|
| Linux x86-64 | `x86_64-unknown-linux-musl` | `searchfor-x86_64-unknown-linux-musl.tar.xz` |
| Linux arm64 | `aarch64-unknown-linux-musl` | `searchfor-aarch64-unknown-linux-musl.tar.xz` |
| macOS arm64 (Apple silicon) | `aarch64-apple-darwin` | `searchfor-aarch64-apple-darwin.tar.xz` |
| macOS x86-64 (Intel) | `x86_64-apple-darwin` | `searchfor-x86_64-apple-darwin.tar.xz` |
| Windows x86-64 | `x86_64-pc-windows-msvc` | `searchfor-x86_64-pc-windows-msvc.zip` |

Los binarios de Linux son estáticos (musl), así que corren en cualquier distribución sin
bibliotecas compartidas. Cada archivo trae `s4`, las dos licencias y los dos READMEs. Windows
sobre arm64 está previsto para una 1.x. Los binarios no tienen firma Authenticode ni
notarización de Apple: Homebrew, npm, cargo y los scripts de instalación los bajan sin la marca
que hace preguntar a SmartScreen o a Gatekeeper, mientras que un archivo bajado con un
navegador puede recibir esa pregunta.

### Verificar una descarga

Cada archivo tiene al lado un archivo `.sha256`, y el release tiene un `sha256.sum` con todos
los checksums. Calculá el SHA-256 del archivo y comparalo con el de su `.sha256`:

```
sha256sum searchfor-x86_64-unknown-linux-musl.tar.xz        # Linux
shasum -a 256 searchfor-aarch64-apple-darwin.tar.xz         # macOS
Get-FileHash searchfor-x86_64-pc-windows-msvc.zip           # Windows, PowerShell
```

Cada archivo, script de instalación y archivo de checksums tiene además una attestation de
artefacto de GitHub: una firma, hecha por GitHub sin ninguna clave privada nuestra (Sigstore,
sin claves), que ata el archivo a la corrida del workflow y al commit que lo armaron. Con la
[CLI de GitHub](https://cli.github.com/):

```
gh attestation verify searchfor-x86_64-unknown-linux-musl.tar.xz --repo neitanod/searchfor
```

Imprime el workflow y el commit de los que salió el archivo, y falla si el archivo cambió
después de armarse.

El paquete npm no verifica ningún checksum: `npm install -g searchfor` baja el archivo de tu
sistema por HTTPS desde el GitHub Release. Para verificar lo que instala, bajá el mismo archivo
del release y comprobá su attestation con `gh attestation verify`, como arriba.

### Sistemas soportados

`s4`, sus Helper Processors y toda su suite de tests compilan y pasan en Linux x86-64, macOS
arm64, macOS x86-64 y Windows x86-64, en CI en cada push (la suite saltea en cada sistema sólo
los tests cuya funcionalidad ese sistema no tiene, listados en
[docs/platforms.md](docs/platforms.md)). `--no-docs --no-trail` imprime lo mismo que el
ripgrep 14.1.1 de ese sistema. Donde los sistemas difieren:

| Tema | Linux | macOS | Windows |
|---|---|---|---|
| Configuración (Definiciones de Formato, `searchfor/formats/`) | `$XDG_CONFIG_HOME/searchfor` (`~/.config/searchfor`) | `~/Library/Application Support/searchfor` | `%APPDATA%\searchfor` |
| Datos (Grammar Helpers, `searchfor/grammars/`) | `$XDG_DATA_HOME/searchfor` (`~/.local/share/searchfor`) | `~/Library/Application Support/searchfor` | `%APPDATA%\searchfor` |
| Rutas impresas | `/` | `/` | como rg: `\` entre los componentes que une el walker; `--path-separator /` lo cambia |
| Hipervínculos (`--hyperlink-format`) | `file://{host}{path}` | lo mismo | como rg: `file:///C:/…`, con la letra de unidad |
| Archivos ocultos | los que empiezan con punto | los que empiezan con punto | los que empiezan con punto, y los que tienen el atributo oculto, como rg |
| Colores | ANSI | ANSI | como rg: ANSI donde la consola lo acepta, la API de consola si no; `--color ansi` fuerza ANSI |
| Mapeo en memoria (`--mmap`) | como rg | como rg en macOS: una búsqueda siempre lee el archivo; s4 igual mapea un archivo de 8 MiB o más para dar sus Trails, y un lector de documentos mapea su documento | como rg |
| La memoria de un lector de documentos | su propia cuenta, y `RLIMIT_DATA` 64 MiB por encima | lo mismo | su propia cuenta, y un límite de memoria de Job Object 64 MiB por encima |
| Ctrl-C | s4 termina cada helper y lector que arrancó, y después muere por la señal | lo mismo | s4 termina cada helper y lector que arrancó, y sale como sale un Ctrl-C no atendido (estado `0xC000013A`, el de rg) |
| Cuando matan a s4 | sus lectores de documentos mueren con él; sus helpers reciben el fin de su stdin y terminan | lo mismo: cada lector vigila a s4 y termina | un Job Object termina cada lector y helper junto con s4 |
| `-P` (PCRE2) | con JIT | sin JIT en Apple silicon, más lento, como el rg de ese sistema; `--pcre2-version` lo dice | con JIT |

`--help` imprime los dos directorios del sistema en el que corre. El programa de `--pre` se
busca como lo busca rg: en Windows un nombre relativo se busca sólo en el `PATH`, nunca en el
directorio actual, así que un programa que quedó en el directorio donde buscás nunca se corre
por accidente. Cuando buscan varios threads y la salida de un archivo pasa los 8 MiB, esa salida
va a un archivo temporal hasta que el archivo termina; en Windows, la salida coloreada con la API de
consola queda en memoria, porque una copia de sus bytes perdería los colores.

## Formatos

Cada Formato de abajo les da a sus archivos el Trail del ejemplo, y
[docs/formats.es.md](docs/formats.es.md) describe cada uno: sus Segmentos, sus límites, qué hace
con lo que no puede leer, y salidas reales sobre [`tests/fixtures/`](tests/fixtures). La columna
`--format` es el nombre de su Definición, el que usan `--format` y `--list-formats`.

| Formato | `--format` | Archivos | Trail de ejemplo |
|---|---|---|---|
| Markdown | `markdown` | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | `Deploy guide › Install › Linux › bash` |
| YAML | `yaml` | `.yaml`, `.yml` | `server › ports[2] › host` |
| JSON | `json` | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest` | `server › ports[2] › host` |
| TOML | `toml` | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | `products[1] › tags[1]` |
| PHP | `php` | `.php`, `.phtml` | `Cart › addItem` |
| JavaScript | `javascript` | `.js`, `.mjs`, `.cjs`, `.jsx` | `Widget › render` |
| TypeScript | `typescript` | `.ts`, `.mts`, `.cts` | `Geometry › Base › describe` |
| TSX | `tsx` | `.tsx` | `View › onClick` |
| Bash | `bash` | `.sh`, `.bash`, `.bats`, `.bashrc`, `.profile` | `deploy` |
| Go | `go` | `.go` | `Server.Start` |
| Rust | `rust` | `.rs` | `outer › Thing › helper` |
| Python | `python` | `.py`, `.pyi`, `.pyw` | `Outer › method › inner` |
| HTML | `html` | `.html`, `.htm`, `.xhtml` | `main#content › section#pricing.card.wide › p.note[2]`; dentro de un script, desde 1.1, `head › script[1] › onClick` ([qué cambió](docs/rg-compat.md#trails-that-changed-since-10)) |
| XML | `xml` | `.xml`, `.svg`, `.rss`, `.plist`, `.csproj`, y los archivos que empiezan con `<?xml` | `catalog › book[2] › title` |
| CSV | `csv` | `.csv` | `row 2 › Plan` |
| TSV | `tsv` | `.tsv` | `row 2 › Role` |
| SubRip | `srt` | `.srt` | `cue 2 00:00:04,000` |
| WebVTT | `vtt` | `.vtt` | `cue intro 00:00.000` |
| ELF | `elf` | cualquier nombre, por sus primeros bytes | `ELF x86-64 › .rodata › main.version+0xe` |
| PE | `pe` | cualquier nombre, por sus primeros bytes | `PE x86-64 › .rdata › version_string+0xe` |
| Mach-O | `macho` | cualquier nombre, por sus primeros bytes; también los fat | `Mach-O arm64 › __TEXT,__cstring › _version+0xe` |
| PDF | `pdf` | `.pdf` | `1 Intro › page 2` |
| Word | `docx` | `.docx`, `.docm`, `.dotx`, `.dotm` | `2 Costs › table 1 › row 2 › Price` |
| Excel | `xlsx` | `.xlsx`, `.xlsm`, `.xltx`, `.xltm` | `Budget › A3` |
| PowerPoint | `pptx` | `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm` | `slide 1 › notes` |
| OpenDocument | `odt`, `ods`, `odp` | `.odt`, `.ott`, `.ods`, `.ots`, `.odp`, `.otp` | como Word, Excel y PowerPoint |
| EPUB | `epub` | `.epub` | `1. Arrival › The Harbour` |
| SQLite | `sqlite` | `.sqlite`, `.sqlite3`, `.db`, `.db3`, `.s3db`, `.sl3` | `users › row 12 › email` |
| AsciiDoc | `asciidoc` | `.adoc`, `.asciidoc` | `Deploy guide › Install › Linux › bash` |
| reStructuredText | `rst` | `.rst`, `.rest` | `Install › Linux › bash` |
| Org-mode | `org` | `.org` | `Budget › TODO Costs :work: › Price` |
| LaTeX | `latex` | `.tex`, `.ltx`, `.latex` | `A Study of Needles › Methods › Sampling › figure` |
| Jupyter | `jupyter` | `.ipynb` | `Analysis › Load data › cell 2 › output 1 › text/plain` |
| INI | `ini` | `.ini`, `.cfg`, `.desktop`, `.gitconfig`, `.editorconfig`, `.git/config` | `remote "origin" › url` |
| Unidades de systemd | `systemd` | `.service`, `.socket`, `.timer`, `.mount`, `systemd/**/*.target` | `Service › ExecStart` |
| Properties de Java | `properties` | `.properties` | `spring.datasource.url` |
| dotenv | `env` | `.env`, `.env.*`, `*.env` | `DATABASE_URL` |
| Configuración de SSH | `ssh-config` | `ssh_config`, `sshd_config`, `.ssh/config` | `Host github.com gitlab.com › IdentityFile` |
| nginx | `nginx` | `nginx.conf`, `nginx/**/*.conf`, `nginx/sites-enabled/*` | `http › server example.com › location /api › proxy_pass` |
| Apache | `apache` | `httpd.conf`, `apache2.conf`, `.htaccess`, `apache2/**/*.conf` | `VirtualHost *:443 example.com › Directory /var/www › Require` |
| Terraform y HCL | `hcl` | `.tf`, `.tfvars`, `.hcl`, `.nomad` | `resource aws_instance web › tags › Name` |
| Dockerfile | `dockerfile` | `Dockerfile`, `Containerfile`, `Dockerfile.*`, `.dockerfile` | `build › RUN` |
| Makefile | `make` | `Makefile`, `GNUmakefile`, `Makefile.*`, `.mk` | `ifeq ($(OS),Windows_NT) › release test` |
| justfile | `just` | `justfile`, `.justfile`, `.just` | `deploy` |
| SQL | `sql` | `.sql`, `.ddl`, `.dml` | `INSERT INTO users › row 2 › email` |
| Diffs y parches | `diff` | `.diff`, `.patch`, `.rej` | `3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe(` |
| Vue | `vue` | `.vue` | `script › addItem` |
| Svelte | `svelte` | `.svelte` | `ul › #each items as item › li[1]` |
| Blade | `blade` | `.blade.php` | `@section('content') › div.cart › @foreach($items as $item) › li` |
| GraphQL | `graphql` | `.graphql`, `.gql`, `.graphqls` | `GetUser › user › posts › title` |
| Protobuf | `protobuf` | `.proto` | `User › choice › nick` |
| HAR | `har` | `.har` | `GET api.example.com/v1/users › request › header Authorization › value` |
| JPEG | `jpeg` | cualquier nombre, por sus primeros bytes | `JPEG › APP1 Exif › IFD0 › Model` |
| PNG | `png` | cualquier nombre, por sus primeros bytes | `PNG › tEXt Comment` |
| WebP | `webp` | cualquier nombre, por sus primeros bytes | `WebP › EXIF › IFD0 › Model` |
| TIFF | `tiff` | cualquier nombre, por sus primeros bytes | `TIFF › IFD0 › Model` |
| ID3 (MP3) | `id3` | cualquier nombre, por sus primeros bytes | `ID3v2.4 › TIT2` |
| MP4 | `mp4` | cualquier nombre, por sus primeros bytes | `MP4 M4A › moov › udta › meta › ilst › ©nam` |
| WebAssembly | `wasm` | cualquier nombre, por sus primeros bytes | `WebAssembly › data › data[0]` |
| Clase de Java | `java-class` | cualquier nombre, por sus primeros bytes | `Java 17 › com.example.Cart › addItem` |
| Mail | `eml` | `.eml` | `part 1 multipart/alternative › part 1.1 text/plain` |
| mbox | `mbox` | `mbox`, `.mbox`, `.mbx` | `Budget › body` |
| iCalendar | `icalendar` | `.ics`, `.ical`, `.ifb` | `VEVENT Team sync › DESCRIPTION` |
| vCard | `vcard` | `.vcf`, `.vcard` | `VCARD Ana Pérez › EMAIL` |
| BibTeX | `bibtex` | `.bib` | `knuth1984 › title` |
| WhatsApp | `whatsapp` | `_chat.txt`, `*WhatsApp*.txt` | `31/12/23 › 21:41 Ana` |
| Telegram | `telegram` | `ChatExport_*/result.json` | `Ana › 2024-01-31T10:04:12 Ana › text` |
| Telegram HTML | `telegram-html` | `ChatExport_*/messages*.html` | `31.01.2024 10:04:12 Ana` |

Los archivos de esta tabla están abreviados: [docs/formats.es.md](docs/formats.es.md#todos-los-formatos)
lista cada nombre, extensión y ruta que reclama cada Formato (en inglés, en
[docs/formats.md](docs/formats.md)). Un archivo sin extensión también se reconoce por su shebang
(`#!/usr/bin/env python3`, `node`, `bash`…), y los Formatos binarios por sus primeros bytes, con
cualquier nombre. s4 nunca mira el contenido de un archivo de texto para elegir su Formato:
`--format 'GLOB=NOMBRE'` lo nombra donde el nombre no lo dice (`--format 'chats/*.txt=whatsapp'`),
y `git diff | s4 --format diff needle` les da Trails a stdin. Cualquier otro archivo es **Plain**:
sus resultados se imprimen con el Trail vacío, como los imprime `rg`.

El Trail es el mismo en todos los layouts, y los Formatos binarios imprimen una fila por string
en su offset hexadecimal donde ripgrep imprimiría `binary file matches`:

```
$ s4 needle config/nginx/nginx.conf --tree
config/nginx/nginx.conf
  http
    server example.com
      location /api
        proxy_pass
          4:   proxy_pass http://needle:8080;
```

```
$ s4 needle m4/bin/app --flat
0x68: ELF x86-64 › .rodata › main.version+0xe │ version=1.2.3-needle
0x77: ELF x86-64 › .rodata │ another needle here
```

Por defecto, los archivos PDF, Word, Excel, PowerPoint, OpenDocument, EPUB y SQLite se buscan en
el texto que les sacan los Extractores incluidos en s4, en una terminal y en un pipe por igual,
cada uno leído en un proceso worker con sus propios límites de memoria y de tiempo. Con
`--no-docs`, estos Formatos nunca se detectan: los archivos se buscan como los busca ripgrep,
sin costo extra, y `s4 --no-docs --no-trail` imprime lo mismo que `rg`.
[docs/formats.es.md](docs/formats.es.md#documentos) tiene los detalles, y
[docs/rg-compat.md](docs/rg-compat.md#trails-that-changed-since-10) el único Trail que cambió
1.1: los scripts de las páginas HTML.

## Uso

`s4 PATRÓN [RUTAS…]` acepta los flags de ripgrep con los nombres de ripgrep. En una terminal
la salida por defecto es el árbol; en un pipe es una línea por resultado,
`ruta:línea: Trail │ texto`.

### Una línea por resultado (`--flat`, el default en un pipe)

```
$ s4 needle md --flat
md/guide.md:7: Deploy guide › Install │ Intro paragraph with needle-intro.
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
md/guide.md:23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |
md/guide.md:24: Deploy guide › Install › Plans › Plan │ | needle-a | needle-b |
md/guide.md:24: Deploy guide › Install › Plans › Price │ | needle-a | needle-b |
md/guide.md:28: Deploy guide › Install › Plans › Cell │ > | q | needle-quoted |
md/guide.md:31: Deploy guide › Install › Plans › Heading in item › Heading in item │   needle-in-item
md/guide.md:33: Deploy guide › Install › Plans │ needle-after-list
md/guide.md:38: Deploy guide › Setext Title │ needle-setext body
```

La línea 24 tiene dos resultados en dos celdas de la tabla, así que se imprime una vez por
columna.

### Sólo los resultados que caen bajo un Trail (`--within`)

```
$ s4 needle md --within "install › linux" --flat
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
```

`--within` se queda con los resultados cuyo Trail contiene esos nombres de Segmento en ese
orden. Los nombres se separan con `›`, `>` o `/`, y cada uno es una subcadena que se compara
sin distinguir mayúsculas. Como `/` siempre separa, un heading con una barra, como `TCP/IP`,
se nombra con una parte sin la barra, entre `›` o `>`: `--within "network › tcp"`.

### La Sección entera (`--expand`)

```
$ s4 needle-10 md --expand --tree
md/guide.md
  Deploy guide
    # Install
      ## Plans
        column «Price»
          21-| Plan | Price |
          22-|------|-------|
          23:| Pro  | needle-10 |
          24-| needle-a | needle-b |
```

`--expand` imprime la Sección del último Segmento de cada resultado: el bloque de código, el
ítem de lista, la tabla o la sección del título completos. Una Sección con varios resultados
se imprime una sola vez.

### Líneas largas

Una línea más ancha que la terminal (o que `--fragment-width`, 200 por defecto, en un pipe)
se muestra como Fragmentos centrados en los resultados, con la cantidad de caracteres omitidos
a cada lado:

```
$ s4 needle misc/min.json --flat --fragment-width 120
1: [0] │ …[+983] aaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaa […+18926]
1: [0] │ …[+14958] aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa […+4951]
```

Los resultados que están lo bastante cerca como para entrar en un Fragmento lo comparten; los
demás van en su propia línea. El `-M/--max-columns` de ripgrep conserva su significado: con
él, las líneas largas imprimen el mensaje `[Omitted long matching line]` de ripgrep en lugar
de Fragmentos.

### Los flags propios de searchfor

| Flag | Qué hace |
|---|---|
| `--within <TRAIL>` | se queda con los resultados cuyo Trail contiene esos nombres de Segmento, en orden (`›`, `>` o `/` entre nombres; subcadenas sin distinguir mayúsculas) |
| `--expand` | imprime la Sección entera del último Segmento de cada resultado, una vez por Sección |
| `--in-headings` | busca sólo dentro de los nombres de Segmento (títulos, claves, nombres de función, elementos, encabezados de CSV, cues); procesa todos los archivos candidatos, así que es más lento |
| `--no-docs` | busca en los archivos PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB y SQLite como ripgrep, a la velocidad de ripgrep, en vez de en su texto con los Extractores incluidos, que es lo que hace por defecto (ver [Documentos](docs/formats.es.md#documentos)) |
| `--docs` | vuelve a buscar en el texto de los documentos después de un `--no-docs`; gana el último de los dos |
| `--format <F>` / `--format <GLOB>=<F>` | fuerza un Formato para todos los archivos o para los que coinciden con un glob |
| `--flat` / `--tree` | fuerza la salida de una línea o la de árbol |
| `--no-trail` | imprime lo que imprimiría ripgrep; con `--no-docs`, exactamente eso |
| `--fragment-width <N>` | ancho del Fragmento cuando no hay terminal (200 por defecto) |
| `--list-formats` | lista cada Formato con su origen: built-in, instalado, del usuario o reemplazado |
| `--export-formats <DIR>` | escribe las Definiciones de Formato incluidas en `DIR` |
| `--list-grammars` | lista las Grammars compiladas, las instaladas y las disponibles (ver [Grammars](#grammars)) |
| `--install-grammar <LANG>` | instala un Grammar Helper desde el índice de Grammars |
| `--mcp` | ofrece la búsqueda como servidor MCP por stdin y stdout (ver [Para agentes de IA](#para-agentes-de-ia)) |

Las Definiciones de Formato son archivos TOML. Un archivo en `searchfor/formats/` dentro del
directorio de configuración del usuario (`~/.config` o `$XDG_CONFIG_HOME` en Linux,
`~/Library/Application Support` en macOS, `%APPDATA%` en Windows; ver
[Sistemas soportados](#sistemas-soportados)) con el nombre de un Formato incluido o instalado lo reemplaza, y un nombre nuevo agrega un Formato.

### Compatibilidad con ripgrep

ripgrep 14.1.1 tiene 146 nombres de flag (104 flags, 30 negaciones `--no-` y 12 negaciones de
otra forma), y s4 los implementa todos; la tabla está en [docs/rg-compat.md](docs/rg-compat.md).
Los códigos de salida son los de ripgrep: 0 si algo coincidió, 1 si nada, 2 si hubo un error.
`SEARCHFOR_CONFIG_PATH` nombra un archivo de configuración con el formato del
`RIPGREP_CONFIG_PATH` de ripgrep, y `--no-config` lo saltea.

Con `--no-trail`, la salida la escribe el propio printer de ripgrep, y `--json --no-trail`
imprime los mismos mensajes que `rg --json`, salvo los tiempos. Los documentos son la
excepción: para tener también sobre ellos la salida de ripgrep, sumá `--no-docs`.

Los flags de salida de ripgrep también cambian las salidas con Trail: `--column` y `-b` agregan
la columna y el offset en bytes, `-o` imprime cada resultado solo bajo su propio Trail,
`--vimgrep` una línea por resultado para un editor, `-r` reemplaza los resultados, `-U` imprime
cada línea de un resultado que ocupa varias, y los separadores, `-0`, `--path-separator`,
`--colors` (con un tipo `trail` propio) y `--hyperlink-format` funcionan como en ripgrep.
`--heading` es el árbol y `--no-heading` la salida de una línea:

```
$ s4 --flat --column -o 'needle-\w+' md/guide.md
7:22: Deploy guide › Install │ needle-intro
12:13: Deploy guide › Install › Linux › bash │ needle-pkg
16:12: Deploy guide › Install › Linux › step one │ needle-item
23:10: Deploy guide › Install › Plans › Price │ needle-10
…
$ s4 --vimgrep needle-pkg md
md/guide.md:12:13: Deploy guide › Install › Linux › bash │ apt install needle-pkg
$ s4 -P --flat 'needle-(?=pkg)' md
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
```

`--pre`, `--pre-glob`, `--no-pre`, `-z` y `--no-search-zip` funcionan como en ripgrep 14.1.1,
errores y códigos de salida incluidos, y entre `-z` y `--pre` gana el último que se pasa. Lo que
devuelven se busca como se busca stdin, con los Trails del Formato que dice la ruta del archivo.
Acá `pre.sh`, escrito para este ejemplo, es `sed s/needle-pkg/NEEDLE-PKG/ "$1"`:

```
$ s4 --pre ./pre.sh --pre-glob '*.md' --flat NEEDLE-PKG md
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install NEEDLE-PKG
```

Diferencias conocidas con ripgrep 14.1.1:

- En la salida `--json`, las `stats` de los mensajes `end` y `summary` cuentan los resultados
  que encontró la búsqueda, antes de que `--within` e `--in-headings` los filtren.
- Un archivo de un Formato binario con un resultado (ELF, PE, Mach-O, y desde 1.1 imágenes,
  audio, WebAssembly y clases de Java) imprime las filas de
  [Archivos objeto](docs/formats.es.md#archivos-objeto) donde ripgrep imprime `binary file matches`, así que s4
  busca en el archivo entero; en `--json`, `bytes_searched` es el tamaño del archivo y
  `binary_offset` es null.
- Los documentos se buscan por defecto en su texto, también con `--no-trail`, `-c`, `-l`, `-q`
  y `--json`, y cada uno se extrae, lo que cuesta más que la búsqueda de ripgrep en sus bytes.
  `--no-docs` da la salida y el costo de ripgrep (ver [Documentos](docs/formats.es.md#documentos)).
- En las salidas con Trail, un archivo cuyo Formato no lo decide su nombre, stdin entre ellos,
  se lee entero antes de imprimir sus filas; `--no-trail` imprime cada línea como ripgrep.
- `--debug` imprime los mensajes de los crates de ripgrep como ripgrep, y los propios de s4;
  `--generate` escribe la página de manual y los completados de s4; `-P` usa el PCRE2 con el
  que se compiló s4.

La lista completa está en [docs/rg-compat.md](docs/rg-compat.md#known-deviations).

## Helper Processors

Un Helper Processor es un programa, escrito en cualquier lenguaje, que describe los archivos
de un Formato: s4 lo arranca cuando el primer archivo de ese Formato tiene un resultado, le
manda la ruta y las posiciones de los resultados de cada archivo como líneas JSON por su
stdin, lee los Trails de su stdout, y le cierra el stdin cuando termina la búsqueda. Una
Definición de Formato con `processor = "external"` lo nombra:

```toml
name       = "po-guide"
extensions = ["po"]
processor  = "external"

[params]
command    = ["python3", "/absolute/path/to/docs/examples/po-helper.py"]
mode       = "oneshot"
```

`mode` dice cómo le pasa s4 los archivos:

- `pool` (el default): cada copia recibe un archivo por vez, y s4 arranca más copias, hasta
  la cantidad de threads de la búsqueda, cuando todas están ocupadas. Un programa que lee una
  línea, la contesta y lee la siguiente funciona en este modo.
- `concurrent`: un solo proceso recibe varios archivos a la vez y puede contestarlos en
  cualquier orden, porque cada respuesta lleva el `id` de su pedido.
- `oneshot`: un proceso por archivo, para scripts chicos que leen todo su stdin antes de
  contestar.

Un helper nunca frena la búsqueda ni cambia el código de salida. Un archivo cuyo pedido falla
(una respuesta `error`, un crash, ninguna respuesta dentro de `timeout_ms`, 5000 por defecto)
se imprime como Plain, con un aviso; después de tres crashes, timeouts o líneas que no son
JSON en una misma corrida, el Formato se imprime como Plain por el resto de la corrida, con un
aviso más.

s4 no tiene un Formato de gettext, así que un archivo `.po` es Plain. Con la Definición de
arriba en el directorio de formatos, [`po-helper.py`](docs/examples/po-helper.py) le da al
resultado de [`docs/examples/sample.po`](docs/examples/sample.po) el Trail de su contexto y su
entrada, y sus `names` le permiten a `--in-headings` encontrar la entrada por su texto original:

```
$ s4 --flat needle sample.po
3: menu › Open │ msgstr "needle"
$ s4 --flat --in-headings Open sample.po
2: menu │ msgid "Open"
```

El protocolo está en
[la spec, §4.4](docs/specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459).
[docs/external-processors.es.md](docs/external-processors.es.md) es la guía para escribir uno:
el protocolo mensaje por mensaje, los tres modos, las fallas y cómo las informa s4, y este
ejemplo completo en Python, primero con `--pre` y después como Helper Processor, con la receta
para cada sistema. [`helpers/example/`](helpers/example) es una implementación de referencia en
Rust, que describe archivos INI; una Definición suya llamada `ini` reemplaza al Formato INI
incluido.

## Grammars

Las Grammars de los ocho Formatos de código están compiladas dentro de `s4`. Cualquier otro
lenguaje llega como un Grammar Helper: un Helper Processor que arma searchfor con una sola
Grammar, y que `--install-grammar` baja del índice de Grammars, verifica contra su SHA-256 e
instala con su Definición de Formato en `$XDG_DATA_HOME/searchfor/grammars/` (en Linux,
`~/.local/share/searchfor/grammars/` por defecto).

```
$ s4 --list-grammars
php            compiled in
javascript     compiled in
typescript     compiled in
tsx            compiled in
bash           compiled in
go             compiled in
rust           compiled in
python         compiled in
```

v1.0 suma seis Grammar Helpers: **c, cpp, java, ruby, c-sharp y elixir**, cada uno armado
para los cinco sistemas de [Instalación](#instalación) y publicado en un GitHub Release
propio, `grammars-v1`. Una vez publicado ese release y listado en el índice, esto instala uno:

```
s4 --install-grammar ruby
```

El índice, [`grammars/index.json`](grammars/index.json), queda vacío hasta entonces, así que
`--install-grammar` todavía no tiene nada para instalar desde el repositorio.
[`helpers/grammar/`](helpers/grammar) es el template del que se arman, y
[`scripts/build-grammar-helper.sh`](scripts/build-grammar-helper.sh) arma uno para el sistema
en el que corre e imprime su entrada del índice (el binario es `<lang>`, y `<lang>.exe` en
Windows). `SEARCHFOR_GRAMMAR_INDEX` apunta s4 a otro índice, por ejemplo uno propio con
`file://`. En macOS y Windows el directorio de Grammars está dentro del directorio de datos de
[Sistemas soportados](#sistemas-soportados).

## Para agentes de IA

Un agente que encuentra un resultado con `rg` suele abrir el archivo entero para saber en qué
sección cae la línea. `s4` imprime esa sección en el Trail, y `--expand` imprime la sección
misma, así que el agente lee la parte del archivo que importa y nada más.

### Servidor MCP

`s4 --mcp` habla el Model Context Protocol por stdin y stdout y ofrece una herramienta,
`search`, con `pattern`, `paths`, `within`, `expand`, `glob`, `case_insensitive` y
`max_results` (100 si no se pasa, 1 000 como máximo). Responde con la ruta, la línea, el Trail
y el texto de cada resultado, y con `expand` su Sección, en 256 KiB como máximo: una línea con
resultados bajo dos Trails llega una vez por Trail, una línea larga como su Fragmento, una
Sección larga cortada, una Sección que comparten varios resultados una sola vez, y `truncated`
avisa que la búsqueda encontró más de lo que entra en la respuesta, y `truncated_by` por qué
(`max_results`, `bytes`, o `long_line` por una línea de más de 4 MiB, cuyo archivo nombra
`errors`, las primeras 20, y cuenta las demás).
Sus búsquedas no leen ningún archivo de configuración, así que un `--pre` en
`SEARCHFOR_CONFIG_PATH` nunca corre para un cliente. Para sumarlo a Claude Code:

```
claude mcp add searchfor -- s4 --mcp
```

Una llamada a la herramienta, y su respuesta:

```
{"name":"search","arguments":{"pattern":"needle","paths":["md"],"within":"Install › Linux"}}
{"matches":[{"path":"md/guide.md","line":12,"trail":["Deploy guide","Install","Linux","bash"],
  "text":"apt install needle-pkg","format":"markdown"}, …],"truncated":false}
```

### Desde la línea de comandos

En qué sección cae cada resultado:

```
$ s4 --json needle md | jq -r 'select(.type == "match") | .data.trail | map(.name) | join(" › ")'
Deploy guide › Install
Deploy guide › Install › Linux › bash
Deploy guide › Install › Linux › step one
Deploy guide › Install › Plans › Price
Deploy guide › Install › Plans › Plan
Deploy guide › Install › Plans › Cell
Deploy guide › Install › Plans › Heading in item › Heading in item
Deploy guide › Install › Plans
Deploy guide › Setext Title
```

Leer sólo las secciones que caen bajo un título:

````
$ s4 needle md --within linux --expand --flat
md/guide.md:11-13: Deploy guide › Install › Linux › bash
```bash
apt install needle-pkg
```

md/guide.md:15-16: Deploy guide › Install › Linux › step one
- step one
  - nested needle-item

````

Leer sólo el método en el que cae un resultado, en el código:

```
$ s4 needle code --within "Cart › addItem" --expand --flat
code/cart.php:8-12: Cart › addItem
    public function addItem(Item $item, int $qty = 1): void
    {
        // needle: merge equal items
        $this->items[] = [$item, $qty];
    }

```

Leer cada función con un nombre como `helper` que tenga un resultado:

```
$ s4 needle code --within helper --expand --flat --sort path
code/cart.php:15-18: needle_helper
function needle_helper(): string
{
    return "needle";
}

code/server.go:11-13: helper
func helper() string {
	return "needle"
}

code/thing.rs:7-9: outer › Thing › helper
        pub fn helper(&self) -> u32 {
            self.needle + 1
        }

```

Encontrar la diapositiva, la hoja o la página donde está una frase:

```
$ s4 --flat 'needle joke' m4/deck.pptx m4/deck.odp
m4/deck.pptx:3: slide 1 › notes │ tell the needle joke
m4/deck.odp:3: slide 1 › notes │ tell the needle joke
```

Encontrar qué sección y qué símbolo de un binario tienen una cadena, con `--binary` sobre un
directorio:

```
$ s4 --binary --flat --sort path version= m4/bin
m4/bin/app:0x5a: ELF x86-64 › .rodata › main.version │ version=1.2.3-needle
m4/bin/app-mac:0x1133: Mach-O x86-64 › __TEXT,__cstring › _version │ version=1.2.3-needle
m4/bin/app-mac:0x2133: Mach-O arm64 › __TEXT,__cstring › _version │ version=1.2.3-needle
m4/bin/app-stripped:0x5a: ELF x86-64 › .rodata │ version=1.2.3-needle
m4/bin/app.exe:0x402: PE x86-64 › .rdata › version_string │ version=1.2.3-needle
```

`--json` emite los JSON Lines de ripgrep (`begin`, `match`, `context`, `end`, `summary`), así
que cualquier consumidor de `rg --json` sigue funcionando, y agrega tres campos a cada
`match`:

```
{"type":"match","data":{"path":{"text":"md/guide.md"},"lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,"submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown","trail":[{"kind":"document","name":"Deploy guide","line":1,"end_line":38},{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},{"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2},{"kind":"code","name":"bash","line":11,"end_line":13}],"fragment":null}}
```

- `format`: el Formato del archivo, o `null` para los archivos Plain.
- `trail`: los Segmentos desde la raíz del archivo hasta el resultado, cada uno con `kind`,
  `name`, `line` y `end_line` (las líneas de su Sección), `level` en los títulos, y
  `signature` en el código (la línea que lo declara) y en los cues de subtítulos (el cue con su
  tiempo de fin).
- `fragment`: `null`, o el Fragmento de una línea más larga que `--fragment-width`.
- Con `--expand`, `section`: `{"start_line", "end_line", "text"}` de la Sección.
- Cuando los resultados de una misma línea tienen Trails distintos (dos celdas de una tabla),
  cada objeto de `submatches` lleva su propio `trail`.

El esquema de estos campos está en [docs/json-schema.json](docs/json-schema.json). Las
`stats` de los mensajes `end` y `summary` cuentan los resultados antes de que `--within` e
`--in-headings` los filtren.

En [skills/searchfor](skills/searchfor/SKILL.md) hay un skill de Claude Code con estas
recetas; para instalarlo, copiá ese directorio a `~/.claude/skills/`.

## Hoja de ruta

| Hito | Versión | Contenido |
|---|---|---|
| M1 | v0.1 | Markdown, los flags centrales de ripgrep, salida en árbol / una línea / JSON, Fragmentos, `--within`, `--expand`, `--in-headings`, skill de Claude Code |
| M2 | v0.2 | rutas de claves en YAML, JSON y TOML (`server › ports[2] › host`); código (PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust, Python) como `Cart › addItem`, con las Grammars de tree-sitter compiladas adentro; Helper Processors (`processor = "external"`); `--install-grammar` y `--list-grammars` |
| M3 (terminado, 2026-10-17) | v0.3 | rutas de elementos en HTML y XML (`section#pricing › table › tr[3] › td[2]`); filas y columnas en CSV y TSV (`row 2 › Plan`); cues de SubRip y WebVTT (`cue 2 00:00:04,000`); los caracteres de control de los nombres impresos como escapes |
| M4 (terminado, 2026-10-19) | v0.4 | archivos objeto (ELF, PE, Mach-O, Mach-O fat) como `ELF x86-64 › .rodata › main.version+0xe`; documentos buscados por defecto (`--no-docs` lo apaga), con Extractores para PDF, Word, Excel, PowerPoint, texto, planilla y presentación de OpenDocument, EPUB y SQLite, cada documento leído en un proceso aparte que cuenta su propia memoria; los flags `--pre`, `--pre-glob`, `--no-pre`, `-z` y `--no-search-zip` de ripgrep; el benchmark de M4, los techos de rendimiento y los fuzz targets |
| M5a (terminado, 2026-10-20) | v0.5 | todos los flags de ripgrep 14.1.1, en `--no-trail` como los imprime ripgrep y en las salidas con Trail (PCRE2, búsqueda en varias líneas, codificaciones, los flags de ignorar y de ordenar, `--files`, `-o`, `--column`, `--vimgrep`, `-r`, `--stats`, hipervínculos, un archivo de configuración, `--generate`, `--debug`); un servidor MCP (`s4 --mcp`) |
| M5b (preparado, 2026-10-20; publicación pendiente) | v1.0 | binarios precompilados (GitHub Releases), Homebrew y npm; Windows y macOS en CI, donde se corre la cuenta de memoria de los lectores, y un Job Object de Windows sobre cada lector de documentos como segunda barrera; los binarios de los Grammar Helpers publicados en el índice de Grammars; releases firmados; la guía para escribir un Helper Processor; los Trails en streaming para los archivos donde coinciden casi todas las líneas |
| M6 (en desarrollo) | v1.1 | cuarenta Formatos: AsciiDoc, reStructuredText, Org-mode, LaTeX y Jupyter; INI, systemd, `.properties`, `.env`, SSH, nginx, Apache, Terraform y HCL, Dockerfile, Makefile y justfile; SQL, diffs, Vue, Svelte, Blade, GraphQL, Protobuf y HAR; JPEG, PNG, WebP, TIFF, ID3, MP4, WebAssembly y clases de Java (en el offset hexadecimal del resultado, como los archivos objeto); mail, mbox, iCalendar, vCard, BibTeX, WhatsApp y Telegram; Trails dentro de los scripts de las páginas HTML; `paths` y extensiones compuestas en las Definiciones de Formato |

Los formatos binarios de Office (`.doc`, `.xls`, `.ppt`) y los encabezados, pies de página,
notas al pie y comentarios de los documentos de Office, que figuraban en M6 antes de su diseño,
quedan afuera de él. Los Formatos de empaquetado (zip, tar, 7z, rar, deb, rpm, jar) también
quedan afuera: searchfor busca
donde busca ripgrep, y `-z` conserva el significado de ripgrep (archivos comprimidos de un
solo flujo, como `.gz`).

## Licencia

Doble licencia, [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE), a elección.
