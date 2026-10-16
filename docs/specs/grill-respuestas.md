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

- **8355** — Move the Agencia Project to the «Apps» space. Done by Sebastián from the UI: the
  API reserves it to him.
- **8361** — Grammars are WebAssembly, not native shared libraries (replaces the literal answer
  to #8; its comments still apply: all included by default, easy installation helper).
- **8362** — «Nuestro --json es nuestro, y si alguien quiere el de rg podemos darle --json-rg».
- **#18 comment** — npm is a usual channel for Rust tools (Biome, oxlint), so it is included.
