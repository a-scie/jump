#!/usr/bin/env bash
# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

set -eou pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
EXAMPLE_DIR="${REPO_ROOT}/examples"
cd "${EXAMPLE_DIR}"

COMMON="${EXAMPLE_DIR}/common.sh"
# shellcheck source=common.sh
source "${COMMON}"
export COMMON

for cmd in basename jq uname; do
  check_cmd "${cmd}"
done

function calculate_os() {
  local os

  os="$(uname -s)"
  if [[ "${os}" =~ [Ll]inux ]]; then
    echo linux
  elif [[ "${os}" =~ [Dd]arwin ]]; then
    echo macos
  elif [[ "${os}" =~ [Ww]in|[Mm][Ii][Nn][Gg] ]]; then
    # Powershell reports something like: Windows_NT
    # Git bash reports something like: MINGW64_NT-10.0-22621
    echo windows
  else
    die "Integration tests are not supported for this operating system (${os})."
  fi
}

function calculate_arch() {
  local arch

  arch="$(uname -m)"
  if [[ "${arch}" =~ x86[_-]64 ]]; then
    echo x86_64
  elif [[ "${arch}" =~ arm64|aarch64 ]]; then
    echo aarch64
  else
    die "Integration tests are not supported for this chip architecture (${arch})."
  fi
}

OS="$(calculate_os)"
if [[ "${OS}" == "windows" ]]; then
  check_cmd pwsh
else
  check_cmd curl
fi

function fetch_one() {
  local url="$1"
  local dest
  dest="$(basename "${url}")"
  if [[ -f "${dest}" ]]; then
    echo "Already fetched ${dest} from ${url}"
  else
    echo "Fetching ${url} ..."
    if [[ "${OS}" == "windows" ]]; then
      pwsh -c "Invoke-WebRequest -OutFile $dest -Uri $url"
    else
      curl -fL -o "$(basename "${url}")" "${url}"
    fi
  fi
}

function fetch_all() {
  local example="$1"
  (
    cd "${example}"
    while read -r url; do
      if [ -n "${url}" ]; then
       fetch_one "${url}"
      fi
    done
  )
}

function fetch() {
  local example="$1"
  if [ -f "${example}.fetch" ]; then
    fetch_all "${example}" < "${example}.fetch"
  fi
  jq -r '.fetch[]?' "${example}/lift.${OS_ARCH}.json" | fetch_all "${example}"
}

_USAGE="$(
  cat << EOF
Usage: $0 [--no-gc] [example]*

Runs all examples by default. List example directory names to run specific ones.

--no-gc: Prevents example artifacts generated during the run from being garbage collected.
         This is useful for experimenting or test development."

EOF
)"

_EXAMPLE_PATHS=()
for arg in "$@"; do
  if [[ "${arg}" =~ -h|--help ]]; then
    echo "${_USAGE}"
    exit 0
  elif [[ "${arg}" =~ --no-gc ]]; then
    export NO_GC=1
  elif [[ -d "${arg}" ]]; then
    _EXAMPLE_PATHS+=("${arg}")
  else
    log "${_USAGE}"
    die "\nERROR: ${arg} is not a recognized option or an example directory."
  fi
done

ARCH="$(calculate_arch)"
OS_ARCH="${OS}-${ARCH}"
DIST_DIR="${REPO_ROOT}/dist"
SCIE_JUMP="${DIST_DIR}/scie-jump-${OS_ARCH}"
if [[ ! -e "${SCIE_JUMP}" ]]; then
  cargo run --release -p package "${DIST_DIR}"
fi

export ARCH OS OS_ARCH SCIE_JUMP

if (( "${#_EXAMPLE_PATHS[@]}" == 0 )); then
  for path in *; do
    if [[ -d "${path}" ]]; then
      _EXAMPLE_PATHS+=("${path}")
    fi
  done
fi

for example_path in "${_EXAMPLE_PATHS[@]}"; do
  example="$(basename "${example_path}")"
  log
  log "*** Running ${example} example ***"
  log
  fetch "${example}"
  (
    cd "${example}"
    bash -eou pipefail test.sh
  )
done
