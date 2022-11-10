# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

function log() {
  echo >&2 "$@"
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
  if (( $# > 0 )); then
    check_cmd rm
    _GC+=("$@")
  else
    rm -rf "${_GC[@]}"
  fi
}