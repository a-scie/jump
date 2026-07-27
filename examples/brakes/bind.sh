#!/usr/bin/env bash

set -euo pipefail

if (( $# != 1 )); then
  echo "Usage: $0 <bind base dir>"
  exit 1
fi
BIND_BASE_DIR="$1"

BOUND_DIR="${BIND_BASE_DIR}/dir"
BOUND_FILE="${BIND_BASE_DIR}/file"
BOUND_EXISTS="${BIND_BASE_DIR}/exists"

mkdir -p "${BOUND_DIR}"
echo "file" > "${BOUND_FILE}"
echo "exists" > "${BOUND_EXISTS}"

if [ "${BIND_JSON:-0}" = "1" ]; then
  cat << EOF > "${SCIE_BINDING_JSON}"
[
  {"key": "BOUND_DIR", "value": "${BOUND_DIR}", "brake": "exists"},
  {"key": "BOUND_FILE", "value": "${BOUND_FILE}", "brake": "exists"},
  {"key": "BOUND_EXISTS", "value": "${BOUND_EXISTS}"}
]
EOF
else
  cat << EOF > "${SCIE_BINDING_ENV}"
BOUND_DIR=${BOUND_DIR}
BOUND_FILE=${BOUND_FILE}
BOUND_EXISTS=${BOUND_EXISTS}
EOF
fi