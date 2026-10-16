> 🇬🇧 Read in English: [README.md](README.md)

# searchfor

`sfor` es un buscador compatible con ripgrep que te dice **dónde** cae cada resultado dentro
de la estructura de su archivo.

```
$ sfor ExecStart docs/
docs/deploy.md
  # Deploy de apps Go
    ## systemd
      42: ExecStart=/opt/apps/notas/notas
```

- **Markdown**: la cadena de títulos, más el bloque de código y la columna de la tabla.
- **Código**: clase › función, con tree-sitter.
- **YAML, JSON, TOML**: la ruta de claves, como `server.ports[2].host`.
- **HTML, XML**: la ruta de elementos.
- **CSV**: la fila y el nombre de la columna.
- **Binarios (ELF, PE, Mach-O)**: formato, sección, offset, símbolo más cercano y la cadena
  imprimible alrededor del resultado.
- **Contenedores (zip, docx, epub, jar, tar.gz)**: busca adentro y muestra el archivo interno.

Las líneas largas se muestran como un fragmento centrado en el resultado, en vez de
ocultarse, y `--json` emite un objeto por resultado para agentes de IA.

## Estado

En etapa de diseño. Todavía no hay nada para instalar. El glosario está en
[CONTEXT.md](CONTEXT.md) y las decisiones de arquitectura en [docs/adr](docs/adr).

## Uso

Todavía no disponible. La línea de comandos va a aceptar los flags de ripgrep con los mismos
nombres.

## Instalación

Todavía no disponible. Canales previstos: crates.io, GitHub Releases, Homebrew y npm.

## Licencia

Doble licencia, [MIT](LICENSE-MIT) o [Apache-2.0](LICENSE-APACHE), a elección.
