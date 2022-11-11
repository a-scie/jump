# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

function log() {
  echo -e >&2 "$@"
}

function warn() {
  log "WARNING: " "$@"
}

function die() {
  log "$@"
  exit 1
}

function check_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null ||  die "This script requires the ${cmd} binary to be on the PATH."
}

_GC=()

function gc() {
  if [[ -z "${NO_GC:-}" ]]; then
    if (( $# > 0 )); then
      check_cmd rm
      _GC+=("$@")
    else
      rm -rf "${_GC[@]}"
    fi
  fi
}

function sha256() {
  if [[ "${OS}" == "macos" ]]; then
    shasum --algorithm 256 "$@"
  else
    sha256sum "$@"
  fi
}
