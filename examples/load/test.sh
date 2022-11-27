# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd mktemp

gc "${PWD}/cowsay"
"${SCIE_JUMP}" "${LIFT}"

# Force downloads to occur to exercise the load functionality even if ~/.nce has the JDK and the
# cowsay jars already from other examples.
SCIE_BASE="$(mktemp -d)"
gc "${SCIE_BASE}"
export SCIE_BASE

time RUST_LOG=info ./cowsay "Curl!"
time RUST_LOG=info ./cowsay "Local!"

SCIE_BASE="$(mktemp -d)"
gc "${SCIE_BASE}"
export SCIE_BASE

GET_LOG_CONFIG="$(mktemp)"
gc "${GET_LOG_CONFIG}"
export GET_LOG_CONFIG

export GET_CONFIG="alt-metadata.json"
./cowsay "Alt Curl!"

grep "${GET_CONFIG}" "${GET_LOG_CONFIG}"
