> 🇬🇧 Read in English: [README.md](README.md)

# searchfor

`sfor` es un buscador compatible con ripgrep que te dice **dónde** cae cada resultado dentro
de la estructura de su archivo.

````
$ sfor needle-pkg md --tree
md/guide.md
  Deploy guide
    # Install
      ## Linux
        ```bash
          12:apt install needle-pkg
````

Las líneas que están arriba de `12:apt install needle-pkg` son el **Trail** del resultado:
el título del documento tomado del frontmatter, los títulos y el bloque de código en el que
cae el resultado. Todos los ejemplos de este README son salidas reales sobre
[`tests/fixtures/`](tests/fixtures), corridas desde adentro de ese directorio.

## Estado

**v0.1: Markdown.** El motor de búsqueda son los crates de ripgrep, así que la búsqueda se
comporta como `rg`; encima de eso, los archivos Markdown tienen Trails (título del
frontmatter, títulos, bloques de código, ítems de lista padres y columnas de tabla), las
líneas largas se muestran como Fragmentos, y `--within`, `--expand` e `--in-headings` filtran
y expanden por estructura. Cualquier otro archivo es **Plain**: sus resultados se imprimen con
el Trail vacío. Los demás Formatos llegan por hito (ver [Hoja de ruta](#hoja-de-ruta)).

El glosario está en [CONTEXT.md](CONTEXT.md), el diseño en
[docs/specs](docs/specs/2026-10-16-searchfor-design.md) y las decisiones de arquitectura en
[docs/adr](docs/adr).

## Instalación

```
cargo install searchfor
```

Eso funciona cuando v0.1.0 esté publicada en crates.io. Mientras tanto, se instala desde el
repositorio:

```
cargo install --git https://github.com/neitanod/searchfor
```

Los dos instalan el binario `sfor`. Los binarios precompilados, Homebrew y npm llegan en
v1.0.

## Uso

`sfor PATRÓN [RUTAS…]` acepta los flags de ripgrep con los nombres de ripgrep. En una terminal
la salida por defecto es el árbol; en un pipe es una línea por resultado,
`ruta:línea: Trail │ texto`.

### Una línea por resultado (`--flat`, el default en un pipe)

```
$ sfor needle md --flat
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
$ sfor needle md --within "install › linux" --flat
md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
md/guide.md:16: Deploy guide › Install › Linux › step one │   - nested needle-item
```

`--within` se queda con los resultados cuyo Trail contiene esos nombres de Segmento en ese
orden. Los nombres se separan con `›`, `>` o `/`, y cada uno es una subcadena que se compara
sin distinguir mayúsculas. Como `/` siempre separa, un heading con una barra, como `TCP/IP`,
se nombra con una parte sin la barra, entre `›` o `>`: `--within "network › tcp"`.

### La Sección entera (`--expand`)

```
$ sfor needle-10 md --expand --tree
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
$ sfor needle misc/min.json --flat --fragment-width 120
1: …[+980] aaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaa […+18923]
1: …[+14955] aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaneedleaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa […+4948]
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
| `--in-headings` | busca sólo dentro de los nombres de Segmento (en Markdown, los títulos); procesa todos los archivos candidatos, así que es más lento |
| `--format <F>` / `--format <GLOB>=<F>` | fuerza un Formato para todos los archivos o para los que coinciden con un glob |
| `--flat` / `--tree` | fuerza la salida de una línea o la de árbol |
| `--no-trail` | imprime exactamente lo que imprimiría ripgrep |
| `--fragment-width <N>` | ancho del Fragmento cuando no hay terminal (200 por defecto) |
| `--list-formats` | lista cada Formato con su origen: built-in, del usuario o reemplazado |
| `--export-formats <DIR>` | escribe las Definiciones de Formato incluidas en `DIR` |

Las Definiciones de Formato son archivos TOML. Un archivo en `searchfor/formats/` dentro del
directorio de configuración del usuario (`~/.config` o `$XDG_CONFIG_HOME` en Linux) con el
nombre de un Formato incluido lo reemplaza, y un nombre nuevo agrega un Formato.

### Compatibilidad con ripgrep

ripgrep 14.1.1 tiene 146 nombres de flag (104 flags, 30 negaciones `--no-` y 12 negaciones de
otra forma). v0.1 implementa 51. Los otros 95 se aceptan por nombre y fallan con
`sfor: --<flag> is not supported yet` y código de salida 2, así que ninguno se ignora en
silencio. La tabla está en [docs/rg-compat.md](docs/rg-compat.md). Los códigos de salida son
los de ripgrep: 0 si algo coincidió, 1 si nada, 2 si hubo un error.

Con `--no-trail`, la salida la escribe el propio printer de ripgrep, y `--json --no-trail`
imprime los mismos mensajes que `rg --json`, salvo los tiempos.

Diferencias conocidas con ripgrep 14.1.1:

- En la salida `--json`, las `stats` de los mensajes `end` y `summary` cuentan los resultados
  que encontró la búsqueda, antes de que `--within` e `--in-headings` los filtren.

## Para agentes de IA

Un agente que encuentra un resultado con `rg` suele abrir el archivo entero para saber en qué
sección cae la línea. `sfor` imprime esa sección en el Trail, y `--expand` imprime la sección
misma, así que el agente lee la parte del archivo que importa y nada más.

En qué sección cae cada resultado:

```
$ sfor --json needle md | jq -r 'select(.type == "match") | .data.trail | map(.name) | join(" › ")'
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
$ sfor needle md --within linux --expand --flat
md/guide.md:11-13: Deploy guide › Install › Linux › bash
```bash
apt install needle-pkg
```

md/guide.md:15-16: Deploy guide › Install › Linux › step one
- step one
  - nested needle-item

````

`--json` emite los JSON Lines de ripgrep (`begin`, `match`, `context`, `end`, `summary`), así
que cualquier consumidor de `rg --json` sigue funcionando, y agrega tres campos a cada
`match`:

```
{"type":"match","data":{"path":{"text":"md/guide.md"},"lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,"submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown","trail":[{"kind":"document","name":"Deploy guide","line":1,"end_line":38},{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},{"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2},{"kind":"code","name":"bash","line":11,"end_line":13}],"fragment":null}}
```

- `format`: el Formato del archivo, o `null` para los archivos Plain.
- `trail`: los Segmentos desde la raíz del archivo hasta el resultado, cada uno con `kind`,
  `name`, `line` y `end_line` (las líneas de su Sección), y `level` en los títulos.
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
| M2 | v0.2 | Grammars de tree-sitter como WebAssembly; rutas de claves en YAML, JSON y TOML (`server › ports[2] › host`); código (PHP, JavaScript, TypeScript, Bash, Go, Rust, Python) como `class Cart › addItem` |
| M3 | v0.3 | HTML, XML, CSV y subtítulos (.srt, .vtt) |
| M4 | v0.4 | archivos objeto (ELF, PE, Mach-O); `--docs`, extractores incluidos que buscan en PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB y SQLite con su estructura; el `--pre` de ripgrep |
| M5 | v1.0 | Processors externos, un servidor MCP, el resto de los flags de ripgrep, binarios precompilados, Homebrew y npm, Windows y macOS |

Los Formatos de empaquetado (zip, tar, 7z, rar, deb, rpm, jar) quedan afuera: searchfor busca
donde busca ripgrep, y `-z` conserva el significado de ripgrep (archivos comprimidos de un
solo flujo, como `.gz`).

## Licencia

Doble licencia, [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE), a elección.
