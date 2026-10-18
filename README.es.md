> 🇬🇧 Read in English: [README.md](README.md)

# searchfor

`s4` es un buscador compatible con ripgrep que te dice **dónde** cae cada resultado dentro
de la estructura de su archivo.

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
de claves; en HTML y XML, la ruta de elementos; en CSV y TSV, la fila y la columna; y en los
subtítulos, el cue. Todos los ejemplos de este README son salidas reales sobre
[`tests/fixtures/`](tests/fixtures), corridas desde adentro de ese directorio, salvo el de
[Helper Processors](#helper-processors), que dice sobre qué corre.

## Estado

**v0.3: Markdown, archivos de configuración, código, markup, tablas y subtítulos.** El motor de
búsqueda son los crates de ripgrep, así que la búsqueda se comporta como `rg`; encima de eso,
Markdown, YAML, JSON, TOML, el código en ocho lenguajes, HTML, XML, CSV, TSV, SubRip y WebVTT
tienen Trails (ver [Formatos](#formatos)), las líneas largas se
muestran como Fragmentos, y `--within`, `--expand` e `--in-headings` filtran y expanden por
estructura. Otros lenguajes se pueden describir con un [Helper Processor](#helper-processors),
un programa aparte con el que s4 se comunica. Cualquier otro archivo es **Plain**: sus
resultados se imprimen con el Trail vacío. Los demás Formatos llegan por hito (ver
[Hoja de ruta](#hoja-de-ruta)).

El glosario está en [CONTEXT.md](CONTEXT.md), el diseño en
[docs/specs](docs/specs/2026-10-16-searchfor-design.md) y las decisiones de arquitectura en
[docs/adr](docs/adr).

## Instalación

```
cargo install searchfor
```

Eso instala la última versión publicada en crates.io. Hasta que v0.3.0 esté publicada ahí,
v0.3 se instala desde el repositorio:

```
cargo install --git https://github.com/neitanod/searchfor
```

Los dos instalan el binario `s4`. Los binarios precompilados, Homebrew y npm llegan en
v1.0.

## Formatos

| Formato | Archivos | Segmentos |
|---|---|---|
| Markdown | `.md`, `.markdown`, `.mdx`, `.mkd`, `.mdown` | título del frontmatter, títulos, bloques de código, ítems de lista padres, columnas de tabla |
| YAML | `.yaml`, `.yml` | claves y elementos de arrays; `document N` para cada documento de un stream |
| JSON | `.json`, `.jsonc`, `.jsonl`, `.geojson`, `.webmanifest`, `.babelrc`, `.eslintrc` | claves y elementos de arrays; acepta comentarios y comas finales |
| TOML | `.toml`, `Cargo.lock`, `Pipfile`, `poetry.lock`, `uv.lock` | claves, tablas y arrays de tablas |
| PHP | `.php`, `.phtml` | namespaces, clases, interfaces, traits, enums, métodos, funciones |
| JavaScript | `.js`, `.mjs`, `.cjs`, `.jsx` | clases, métodos, funciones, y arrow functions o function expressions que reciben un nombre |
| TypeScript | `.ts`, `.mts`, `.cts` | los de JavaScript, más namespaces, interfaces y clases abstractas |
| TSX | `.tsx` | los de TypeScript |
| Bash | `.sh`, `.bash`, `.bats`, `.bashrc`, `.bash_profile`, `.bash_aliases`, `.profile` | funciones |
| Go | `.go` | tipos, funciones, y métodos nombrados por su receptor (`Server.Start`) |
| Rust | `.rs` | módulos, structs, enums, traits, bloques `impl`, funciones |
| Python | `.py`, `.pyi`, `.pyw` | clases y funciones, también las anidadas |
| HTML | `.html`, `.htm`, `.xhtml` | elementos como `tag#id.class`, con `[n]` entre los hermanos con el mismo tag; `html` y `body` ocultos |
| XML | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.rss`, `.atom`, `.plist`, `.wsdl`, `.xlf`, `.xliff`, `.kml`, `.gpx`, `.csproj`, `.vbproj`, `.fsproj`, `.nuspec`, `.resx`, `.xaml`, y los archivos que empiezan con `<?xml` | elementos por tag, con `[n]` entre los hermanos con el mismo tag |
| CSV | `.csv` | `row N › Columna`; el separador (`,` `;` tab `\|`) se lee de la primera línea |
| TSV | `.tsv` | `row N › Columna` |
| SubRip | `.srt` | cues: `cue 12 00:01:02,500` |
| WebVTT | `.vtt` | cues: `cue intro 00:00.000` |

Un archivo sin extensión también se reconoce por su shebang (`#!/usr/bin/env python3`,
`node`, `bash`…). Los Formatos de código los parsean Grammars de tree-sitter compiladas
dentro de `s4`; YAML, JSON y TOML, parsers nativos; HTML, XML, CSV, TSV y los subtítulos,
scanners propios de s4. `s4 --list-formats` los lista todos.

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

En CSV y TSV el Trail es la fila y el encabezado de la columna:

```
$ s4 needle table/plans.csv --flat
3: row 2 › Plan │ 2,Bea,needle,"likes ""quotes"", and commas"
5: row 3 › Notes │ lines with needle"
7: row 4 › Notes │ 4,Dan,team,needle at the end,extra
```

`row 2` es el segundo registro después del encabezado. La línea 5 es la segunda línea de un
campo entre comillas de la fila 3, así que sigue estando en la fila 3.

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
| `--format <F>` / `--format <GLOB>=<F>` | fuerza un Formato para todos los archivos o para los que coinciden con un glob |
| `--flat` / `--tree` | fuerza la salida de una línea o la de árbol |
| `--no-trail` | imprime exactamente lo que imprimiría ripgrep |
| `--fragment-width <N>` | ancho del Fragmento cuando no hay terminal (200 por defecto) |
| `--list-formats` | lista cada Formato con su origen: built-in, instalado, del usuario o reemplazado |
| `--export-formats <DIR>` | escribe las Definiciones de Formato incluidas en `DIR` |
| `--list-grammars` | lista las Grammars compiladas, las instaladas y las disponibles (ver [Grammars](#grammars)) |
| `--install-grammar <LANG>` | instala un Grammar Helper desde el índice de Grammars |

Las Definiciones de Formato son archivos TOML. Un archivo en `searchfor/formats/` dentro del
directorio de configuración del usuario (`~/.config` o `$XDG_CONFIG_HOME` en Linux) con el
nombre de un Formato incluido o instalado lo reemplaza, y un nombre nuevo agrega un Formato.

### Compatibilidad con ripgrep

ripgrep 14.1.1 tiene 146 nombres de flag (104 flags, 30 negaciones `--no-` y 12 negaciones de
otra forma). v0.3 implementa 51. Los otros 95 se aceptan por nombre y fallan con
`s4: --<flag> is not supported yet` y código de salida 2, así que ninguno se ignora en
silencio. La tabla está en [docs/rg-compat.md](docs/rg-compat.md). Los códigos de salida son
los de ripgrep: 0 si algo coincidió, 1 si nada, 2 si hubo un error.

Con `--no-trail`, la salida la escribe el propio printer de ripgrep, y `--json --no-trail`
imprime los mismos mensajes que `rg --json`, salvo los tiempos.

Diferencias conocidas con ripgrep 14.1.1:

- En la salida `--json`, las `stats` de los mensajes `end` y `summary` cuentan los resultados
  que encontró la búsqueda, antes de que `--within` e `--in-headings` los filtren.

## Helper Processors

Un Helper Processor es un programa, escrito en cualquier lenguaje, que describe los archivos
de un Formato: s4 lo arranca cuando el primer archivo de ese Formato tiene un resultado, le
manda la ruta y las posiciones de los resultados de cada archivo como líneas JSON por su
stdin, lee los Trails de su stdout, y le cierra el stdin cuando termina la búsqueda. Una
Definición de Formato con `processor = "external"` lo nombra:

```toml
name       = "ini"
extensions = ["ini"]
processor  = "external"

[params]
command    = ["sfor-example-helper"]
mode       = "pool"
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

[`helpers/example/`](helpers/example) es la implementación de referencia, un helper en Rust
que describe archivos INI. Con la Definición de arriba en el directorio de formatos y
`sfor-example-helper` en el `PATH` (`cargo build --release -p sfor-example-helper`), sobre un
archivo `app.ini` escrito para este ejemplo:

```
$ s4 needle app.ini --tree
app.ini
  section database
    host
      2:host = needle.local
  section cache
    6:; needle in a comment
```

Los tres modos dan esta misma salida. El protocolo está en
[la spec, §4.4](docs/specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459).

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

El índice, [`grammars/index.json`](grammars/index.json), queda vacío hasta que v1.0 compile y
publique los binarios de los Grammar Helpers para cada sistema, así que por ahora
`--install-grammar` no tiene nada para instalar. [`helpers/grammar/`](helpers/grammar) es el
template del que se arman (Lua en v0.3), y
[`scripts/build-grammar-helper.sh`](scripts/build-grammar-helper.sh) arma uno para el sistema
en el que corre e imprime su entrada del índice. `SEARCHFOR_GRAMMAR_INDEX` apunta s4 a otro
índice, por ejemplo uno propio con `file://`.

## Para agentes de IA

Un agente que encuentra un resultado con `rg` suele abrir el archivo entero para saber en qué
sección cae la línea. `s4` imprime esa sección en el Trail, y `--expand` imprime la sección
misma, así que el agente lee la parte del archivo que importa y nada más.

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
| M4 | v0.4 | archivos objeto (ELF, PE, Mach-O); `--docs`, extractores incluidos que buscan en PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB y SQLite con su estructura; el `--pre` de ripgrep |
| M5 | v1.0 | un servidor MCP, el resto de los flags de ripgrep, binarios precompilados, Homebrew y npm, Windows y macOS; los binarios de los Grammar Helpers publicados en el índice de Grammars; la guía para escribir un Helper Processor |

Los Formatos de empaquetado (zip, tar, 7z, rar, deb, rpm, jar) quedan afuera: searchfor busca
donde busca ripgrep, y `-z` conserva el significado de ripgrep (archivos comprimidos de un
solo flujo, como `.gz`).

## Licencia

Doble licencia, [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE), a elección.
