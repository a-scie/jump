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

for cmd in basename cargo jq uname; do
  check_cmd "${cmd}"
done

function calculate_os_arch() {
  local os arch

  os="$(uname -o)"
  if [[ "${os}" =~ [Ll]inux ]]; then
    os="linux"
  elif [[ "${os}" =~ [Dd]arwin ]]; then
    os="macos"
  elif [[ "${os}" =~ [Ww]in|[Mm]sys ]]; then
    os="windows"
  else
    die "Integration tests are not supported for this operating system (${os})."
  fi

  arch="$(uname -m)"
  if [[ "${arch}" =~ x86[_-]64 ]]; then
    arch="x86_64"
  elif [[ "${arch}" =~ arm64|aarch64 ]]; then
    arch="aarch64"
  else
    die "Integration tests are not supported for this chip architecture (${arch})."
  fi

  echo "${os}-${arch}"
}

OS_ARCH="$(calculate_os_arch)"

if [[ "${OS_ARCH}" =~ windows ]]; then
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
    if [[ "${OS_ARCH}" =~ windows ]]; then
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

export OS_ARCH

DIST_DIR="${REPO_ROOT}/dist"
SCIE_JUMP="${DIST_DIR}/scie-jump-${OS_ARCH}"
if [[ ! -e "${SCIE_JUMP}" ]]; then
  cargo run --release -p package "${DIST_DIR}"
fi
export SCIE_JUMP

_EXAMPLE_PATHS=("$@")
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
