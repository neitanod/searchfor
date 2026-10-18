# Respuestas — searchfor: arquitectura de la v1

> grill `searchfor-arquitectura-v1` · task 768 · 16/10/2026, 03:40:37

## A · Alcance de la v1

**1. ¿Qué formatos entran en la v1?**

→ Markdown: cadena de títulos *(la recomendada)*
→ YAML, JSON y TOML: ruta de claves *(la recomendada)*
→ Código con tree-sitter: clase › función *(la recomendada)*
→ Binarios ELF, PE y Mach-O: sección, offset y símbolo
→ Contenedores: zip, docx, epub, jar, tar.gz
→ HTML y XML: ruta de elementos
→ CSV: fila y nombre de la columna

**2. ¿Cómo usamos el motor de ripgrep?**

→ Como librería adentro del binario (grep-searcher, grep-regex, ignore) *(la recomendada)*

**3. ¿Qué relación tienen los flags de `sfor` con los de rg?**

→ Compatibilidad completa con rg

  💬 Vamos a necesitar algunos flags propios también, supongo.

**4. Además del regex, ¿qué búsquedas que usan la estructura entran?**

→ `--within "Deploy › Systemd"`: sólo resultados adentro de esa ruta *(la recomendada)*
→ `--expand`: imprimir la sección entera que contiene cada resultado *(la recomendada)*
→ `--in-headings`: buscar sólo en títulos, claves o nombres de funciones

## B · Detector y definiciones de formatos

**5. ¿Qué señales usa el detector para decidir el formato de un archivo?**

→ Extensión *(la recomendada)*
→ Nombre exacto *(la recomendada)*
→ Shebang *(la recomendada)*
→ Bytes mágicos del principio *(la recomendada)*
→ Forzarlo con `--format md` *(la recomendada)*

**6. ¿Dónde viven las definiciones de formatos?**

→ En archivos TOML embebidos en el binario, más una carpeta del usuario que suma o pisa *(la recomendada)*

  💬 Con un comando para exportar los .toml embebidos a una carpeta indicada, (no a la carpeta formats), para que sirvan de ejemplo para crear nuevos o para modificar alguno que el usuario quiera modificar.

**7. ¿Admitimos procesadores externos, escritos en cualquier lenguaje?**

→ Sí, en la v1

**8. ¿Cómo se cargan las gramáticas de tree-sitter?**

→ Cargadas como librerías `.so` desde una carpeta

  💬 Incluyamos todos en la instalación por defecto pero hagamosla fácil.  
  Si podemos hacer que un subcomando de sfor ayuda en la instalación de una librería estaría bueno.

## C · Qué muestra cada resultado

**9. ¿Cómo se llama, en el código, en el JSON y en la documentación, la cadena que lleva hasta el resultado?**

→ `trail` *(la recomendada)*

**10. En código, ¿qué muestra cada eslabón de la ruta?**

→ Nombres en la salida de una línea, firmas en el árbol *(la recomendada)*

**11. En Markdown, además de los títulos, ¿qué más entra en la ruta?**

→ El bloque de código y su lenguaje *(la recomendada)*
→ La columna de la tabla, por su encabezado *(la recomendada)*
→ El `title` del frontmatter como raíz *(la recomendada)*
→ Los ítems de lista padre

**12. En binarios, ¿qué describe cada resultado?**

→ Formato y arquitectura *(la recomendada)*
→ Sección *(la recomendada)*
→ Offset en hexadecimal *(la recomendada)*
→ Símbolo más cercano *(la recomendada)*
→ La cadena imprimible alrededor del resultado *(la recomendada)*

## D · Salida

**13. Cuando la línea es demasiado larga, ¿cómo se arma el fragmento?**

→ Centrado en el resultado y ajustado al ancho de la terminal, con marcas de lo omitido *(la recomendada)*

**14. Si una línea larga tiene varios resultados lejos entre sí, ¿qué se muestra?**

→ Un fragmento que junta los cercanos, y fragmentos aparte para los lejanos *(la recomendada)*

**15. ¿Qué forma tiene `--json`?**

→ Un objeto por resultado, uno por línea (NDJSON) *(la recomendada)*

## E · Proyecto, distribución y forma de trabajo

**16. ¿Dónde vive el repo?**

→ Renombrar a `/home/sebas/robotin/apps/searchfor` y actualizar el Proyecto en Agencia *(la recomendada)*

**17. ¿Cuándo se hace público el repo, y con qué licencia?**

→ Público desde el primer commit, MIT + Apache-2.0

**18. ¿Por qué canales se distribuye?**

→ crates.io (`cargo install searchfor`) *(la recomendada)*
→ Binarios estáticos en GitHub Releases *(la recomendada)*
→ Homebrew, con un tap propio

  💬 Se suele distribuir por npm software hecho en rust?
  Si la respuesta es si, incluyamoslo.

**19. ¿Cómo se integra con agentes de IA?**

→ Un skill de Claude Code que me enseña a usar `sfor` *(la recomendada)*
→ Una sección del README escrita para agentes *(la recomendada)*
→ Un servidor MCP: `sfor mcp`

**20. ¿Reservamos el nombre ya?**

→ Publicar el crate cuando haya una v0.1 que funcione *(la recomendada)*

**21. ¿Con qué metodología lo construimos?**

→ `/ciclo`: spec, plan, rondas de revisión de Fable y Codex, chunks con TDD y revisión de código *(la recomendada)*

**22. ¿Qué me estoy olvidando de preguntar?**

→ ⏸ sin contestar

---

**Sin contestar:** #22


---

# Follow-up decisions in the Hilo (16/10/2026)

- **8355** — Move the Agencia Project to the «Apps» space. Only Sebastián can do it, from the
  UI: the API answers 403 to an agent.
- **8361** — Grammars are WebAssembly, not native shared libraries (replaces the literal answer
  to #8; its comments still apply: all included by default, easy installation helper).
- **8362** — «Nuestro --json es nuestro, y si alguien quiere el de rg podemos darle --json-rg».
- **8365** — Replaces 8362: «Si podemos usar el json de rg y montar lo nuestro encima, y se puede
  consumir vía streaming, entonces hagamos eso». `--json` = ripgrep's messages plus our fields.
- **8366** — The repository goes in the `neitanod` account.
- **#18 comment** — npm is a usual channel for Rust tools (Biome, oxlint), so it is included.
- **8375** — After the first working version: `/cpd`, code review rounds, `/cpd` again; then
  keep going without stopping until every ripgrep flag and every Format discussed is
  supported. Afterwards, make sure it works on Windows and macOS. The whole development is in
  English (code, UI, comments, docs); docs also in Spanish, English being the primary language.
- **8377 / Questionnaire 49** — Sebastián asked for more Formats; 35 options were proposed.
- **8380** — Add the Formats called the most valuable (PDF with chapter, SQLite with table/row/
  column, subtitles with timestamp) before the Windows/macOS step.
- **8381** — Console output must be coloured.
- **8385 / Questionnaire 49 answers** — Every recommended Format plus HAR, Java .class,
  .deb/.rpm and 7z/rar. They go to a new milestone M6 after M5, except the most valuable ones
  (PDF, SQLite, subtitles), which stay in the existing milestones. Comments: «the search part
  must be as fast as ripgrep; the added functionality should only be in the presentation. Not
  interested in digging into compressed packages ripgrep does not already handle. When our
  presentation finds no structure for a Match, at least show it the generic way ripgrep would.»
  Open follow-up (8386/8387): archive Formats out; extraction-based Formats only with -z.
- **8390** — Answer to 8386/8387: «decorate the output of every Format proposed as long as rg
  already finds things in them; leave out the ones rg does not open to inspect inside.»
  Consequences (8391): archive Formats out (zip, tar, 7z, rar, deb, rpm, jar); PDF, Word, Excel,
  PowerPoint and EPUB out (compressed text rg cannot see); binary Formats whose text rg reads in
  binary mode stay (ELF/PE/Mach-O, SQLite, image/audio metadata, WebAssembly, Java .class), as
  decoration of the Match offset; `-z` keeps ripgrep's meaning (gz, xz, bz2 streams).
- **8393 / 8394 / 8395** — Sebastián does want to find text inside PDF, Word/ODT, Excel/ODS,
  PowerPoint/ODP, EPUB and SQLite. Decided: built-in extractors (Rust, no external programs)
  that return text plus structure, enabled by a new flag `--docs`, independent of `-z` (which
  keeps ripgrep's meaning: single-stream compressed files). Without `--docs` those files are
  treated as ripgrep does. Also implement ripgrep's `--pre`. Milestone M4. Archive Formats stay
  out (8390).

---

# Follow-up decisions in the Hilo (17/10/2026): Grammars and Helper Processors

Messages 8449–8459, quoted from the Hilo. What Sebastián wrote, and what he accepted with «Va»,
is a decision; the agent's measurements are the reasons given at the time.

- **8449** — Question, no decision: «En rust se puede derivar trabajo a procesos concurrentes?
  Lo estamos haciendo?» The answer (8450): sfor already searches with one thread per core, as
  ripgrep does, through the `ignore` crate's parallel walker.
- **8451** — «Y tendría mucho overhead poner los procesadores de sintaxis en ejecutables aparte?
  Binarios independientes que se puedan ejecutar y comunicar con el proceso principal? Tal vez
  correrlos una vez y usarlos para procesar muchos archivos mediante IPC?» Sebastián proposes
  Processors as separate executables, started once and fed many files over IPC.
- **8452 / 8453** — The agent's proposal, measured on this machine: starting a process costs
  1.5 ms (`true`) and starting sfor 3 ms; a JSON round trip over a pipe costs 60 µs (measured
  with Python); sfor sends the path and the Match positions, never the content. Against WASM:
  no wasmtime (sfor stays around 5–7 MB and builds about ten times faster), native parsing speed
  and no per-Grammar load time (400 ms for Bash under WASM), and the same mechanism as M5's
  External Processors. Costs: one binary per OS and architecture instead of one `.wasm`, no
  sandbox (mitigated with a SHA-256 per binary in the index and signed releases), and the logic
  to start, restart and feed helpers from many threads. Proposal, replacing the one of 8439: the
  v1 Grammars compiled natively into sfor; everything extra (`--install-grammar` and M5's
  External Processors) as persistent helper binaries speaking JSON lines over stdin and stdout;
  WASM out.
- **8454** — «Va.» Accepts 8453: WASM out, v1 Grammars compiled in, extras as helper binaries.
  Supersedes 8361 and ADR 0003 (replaced by ADR 0005). Same message, a follow-up question:
  «Y si el vez de correr una vez por formato lo corremos una vez por archivo a procesar o le
  pasamos un grupo a procesar y que se cierre cuando termina? Y ejecutar otra vez si hace
  falta?»
- **8455** — The agent's answer: one process per file costs 1.5–3 ms per file with Matches
  (1.5–3 s for 1000 files, against 15 ms for the whole `rg` run); groups delay the output and
  complicate its order. Recommended: **once per search** — the helper starts with the first
  file of its Format, serves all the others, and exits when sfor ends because its stdin is
  closed; if it crashes, it is restarted and that file prints as Plain. Also proposed: an
  optional `mode = "oneshot"` in the Format Definition, one process per file, for helpers
  written as simple scripts. Sebastián did not answer this message with «Va»; 8458 builds on it
  («Pero…») and 8459 announced writing it into the M2 plan with no objection, so it is taken as
  accepted.
- **8456 / 8457** — Question, no decision: «Que es menos tiempo? 1 milisegundo o menos un µs?
  Cómo se pronuncia este último?» (a microsecond is a thousandth of a millisecond, so the 60 µs
  round trip is 25 times cheaper than starting a process).
- **8458** — «Pero diseñarlo de modo que libere recursos entre un archivo que procesa y el
  siguiente, y tal vez que procese varios en paralelo». Two requirements on the helper design:
  release resources between files, and process several files in parallel.
- **8459** — The agent's design, written into the M2 plan as announced, with no objection:
  - **Release between files:** every request is independent; the helper reads the file,
    builds its tree, answers the Trails and frees everything before the next request; only the
    Grammar stays loaded. A memory test processes thousands of files and requires that memory
    use does not grow.
  - **Parallel:** every request carries an id and its answer carries the same id, so a helper
    can serve several at once in its own threads and answer in any order (concurrent mode). For
    a simple helper that does not handle threads, sfor starts several copies of it (up to the
    search's thread count) and spreads the files among them (pool mode). The Format Definition
    says which mode a helper uses.
  - Together with «once per search» as the lifetime and the optional one-per-file mode of 8455.
- **Still valid from M1** (8385): the search must be as fast as ripgrep, the added cost belongs
  to the files with Matches only, and a Match with no structure prints the way ripgrep prints
  it.

---

# Follow-up decisions in the Hilo (17/10/2026): the size of the binary

- **8612** — «21 o 22 Mb no cambian gran cosa respecto de 20. Que el límite sea 23Mb.» The
  ceiling on the release binary (`target/release/sfor`, Linux x86-64) moves from the 20 MB the
  M3 plan set to 23 MB: one or two megabytes more make no real difference. M4's dependencies
  are measured against 23 MB.
