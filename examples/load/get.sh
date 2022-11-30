#!/usr/bin/env bash

set -euo pipefail

scie_lift="$1"
file_name="$2"

if [[ -n "${GET_LOG_CONFIG:-}" ]]; then
  echo "${scie_lift}" > "${GET_LOG_CONFIG}"
fi
echo >&2 "Using scie lift at ${scie_lift} to determine URL to fetch ${file_name}."

url="$(jq -r ".get[\"${file_name}\"]" "${scie_lift}")"
exec curl -fL "${url}"