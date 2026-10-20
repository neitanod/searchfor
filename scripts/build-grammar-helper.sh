#!/usr/bin/env bash
# Builds the Grammar Helper of one Grammar for one target and prints its index entry
# (spec §7, M2 plan D13, M5b plan Task 8.3). The release workflow runs it per Grammar and per
# target and assembles grammars/index.json from the entries.
#
# Usage: scripts/build-grammar-helper.sh LANG [OUT_DIR [BASE_URL [TARGET]]]
#   LANG      a Grammar feature of helpers/grammar: lua, c, cpp, java, ruby, c-sharp, elixir
#   OUT_DIR   where the binary is copied; default $CARGO_TARGET_DIR/grammar-helpers
#   BASE_URL  the URL the binary will be downloaded from, without the file name; default the
#             file:// URL of OUT_DIR
#   TARGET    a Rust target triple; default the host's (`rustc -vV`)
# CARGO_TARGET_DIR is inherited. The binary is the one cargo reports for TARGET
# (--message-format=json), never a fixed target/release path, and it is named
# searchfor-grammar-<lang>-<os>-<arch>, plus .exe for Windows. The index key is <os>-<arch>
# as s4 computes it (linux, macos or windows; x86_64 or aarch64), derived from TARGET.
# Needs cargo, rustc and jq, and sha256sum or shasum.
set -euo pipefail
usage="usage: $0 LANG [OUT_DIR [BASE_URL [TARGET]]]"
lang="${1:?$usage}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${2:-${CARGO_TARGET_DIR:-$root/target}/grammar-helpers}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
if [ -n "${3:-}" ]; then
  base="${3%/}"
else
  # Git Bash on Windows: `pwd -W` gives the drive form (C:/...), which a file URL needs.
  native="$( (cd "$out" && pwd -W 2>/dev/null) || echo "$out")"
  case "$native" in /*) base="file://$native" ;; *) base="file:///$native" ;; esac
fi
target="${4:-$(rustc -vV | sed -n 's/^host: //p')}"
[ -n "$target" ] || { echo "$0: could not find the host target" >&2; exit 2; }

case "$target" in
  *-linux-*) os=linux ;;
  *-apple-darwin) os=macos ;;
  *-windows-*) os=windows ;;
  *) echo "$0: unsupported target $target" >&2; exit 2 ;;
esac
case "${target%%-*}" in
  x86_64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) echo "$0: unsupported architecture in $target" >&2; exit 2 ;;
esac
system="$os-$arch"
exe=""
[ "$os" = windows ] && exe=".exe"

cd "$root"
# One source per Definition (grammars/<lang>/<lang>.toml); Lua, the template's default, has
# only the helper's copy.
definition="grammars/$lang/$lang.toml"
[ -f "$definition" ] || definition="helpers/grammar/definitions/$lang.toml"
[ -f "$definition" ] || { echo "$0: no Definition for $lang" >&2; exit 2; }

built="$(cargo build --release --locked -p searchfor-grammar --no-default-features \
    --features "$lang" --target "$target" --message-format=json-render-diagnostics \
  | jq -r 'select(.reason == "compiler-artifact" and .target.name == "searchfor-grammar"
                  and (.target.kind | index("bin")) and .executable != null) | .executable' \
  | tail -n 1)"
[ -n "$built" ] && [ -f "$built" ] || { echo "$0: cargo reported no executable" >&2; exit 1; }

file="searchfor-grammar-$lang-$system$exe"
cp "$built" "$out/$file"
sha=$( (sha256sum "$out/$file" 2>/dev/null || shasum -a 256 "$out/$file") | cut -d' ' -f1)
crate="tree-sitter-$lang"
# Every Grammar feature, so that the optional Grammar crates are listed.
meta="$(cargo metadata --locked --all-features --format-version 1 --quiet)"
version=$(jq -r --arg c "$crate" '[.packages[] | select(.name == $c) | .version] | first // empty' <<<"$meta")
license=$(jq -r --arg c "$crate" '[.packages[] | select(.name == $c) | .license] | first // empty' <<<"$meta")
[ -n "$version" ] || { echo "$0: $crate is not in the workspace's dependencies" >&2; exit 2; }
jq -n --arg name "$lang" --arg version "$version" --arg system "$system" \
  --arg url "$base/$file" --arg sha "$sha" --arg license "${license:-MIT}" \
  --arg source "https://crates.io/crates/$crate" --rawfile definition "$definition" \
  '{name: $name, version: $version, source: $source, license: $license,
    definition: $definition, binaries: {($system): {url: $url, sha256: $sha}}}'
