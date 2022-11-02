#!/usr/bin/env bash

set -eou pipefail

function check_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null || {
    echo >&2 "This script requires the ${cmd} binary to be on the PATH."
    exit 1
  }
}

for cmd in basename curl git; do
  check_cmd "${cmd}"
done

REPO_ROOT="$(git rev-parse --show-toplevel)"

function fetch() {
  local example="$1"
  (
    cd "${example}"
    while read -r url; do
      echo "Fetching ${url} ..."
      curl -fL -O "${url}"
    done
  ) < "${example}.fetch"
}

if (( $# == 0 )); then
  echo >&2 "Usage: $0 EXAMPLE_DIR+"
  exit 1
fi

cd "${REPO_ROOT}/examples"
for example in "$@"; do
  fetch "$(basename "$1")"
done