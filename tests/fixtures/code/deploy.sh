#!/usr/bin/env bash
set -euo pipefail

greet() {
  echo "hello needle"
}

function deploy {
  rsync -a build/ host:/srv/needle/
}
