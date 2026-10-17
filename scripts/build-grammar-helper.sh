#!/usr/bin/env bash
# Builds the Grammar Helper of one Grammar for this system and prints its index entry
# (spec §7, M2 plan D13). M5's release pipeline runs it per system and fills grammars/index.json.
# Usage: scripts/build-grammar-helper.sh LANG [OUT_DIR [BASE_URL]]
# Needs jq and sha256sum (or shasum on macOS).
set -euo pipefail
lang="${1:?usage: $0 LANG [OUT_DIR [BASE_URL]]}"
out="${2:-target/grammar-helpers}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
base="${3:-file://$out}"
case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) os=windows ;; esac
arch="$(uname -m)"; [ "$arch" = arm64 ] && arch=aarch64
system="$os-$arch"
definition="helpers/grammar/definitions/$lang.toml"
[ -f "$definition" ] || { echo "no $definition" >&2; exit 2; }
cargo build --release --quiet -p sfor-grammar --no-default-features --features "$lang"
file="sfor-grammar-$lang-$system"
cp target/release/sfor-grammar "$out/$file"
sha=$( (sha256sum "$out/$file" 2>/dev/null || shasum -a 256 "$out/$file") | cut -d' ' -f1)
version=$(cargo metadata --format-version 1 --quiet \
  | jq -r --arg l "tree-sitter-$lang" '.packages[] | select(.name == $l) | .version' | head -1)
jq -n --arg name "$lang" --arg version "$version" --arg system "$system" \
  --arg url "$base/$file" --arg sha "$sha" --rawfile definition "$definition" \
  '{name: $name, version: $version, source: ("https://crates.io/crates/tree-sitter-" + $name),
    license: "MIT", definition: $definition,
    binaries: {($system): {url: $url, sha256: $sha}}}'
