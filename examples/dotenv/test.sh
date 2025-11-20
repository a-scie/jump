# Copyright 2025 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd mktemp

gc "${PWD}/dotenv"
"${SCIE_JUMP}" "${LIFT}"

SCIE_BASE="$(mktemp -d)"
gc "${SCIE_BASE}"
export SCIE_BASE

unset SLARTIBARTFAST ZAPHOD FORD ARTHUR

function assert_binding() {
  result="$(./dotenv)"
  [[ "$(./dotenv)" == "$1" ]] && log "Binding works as expected ($1)." || die "Env did not propagate to binding.\\nExpected: $1\\nGot: ${result}"
}

assert_binding "42"

SCIE_BASE="$(mktemp -d)"
gc "${SCIE_BASE}"
export SCIE_BASE

gc "${PWD}/.env"
echo "ARTHUR=1/137" > .env

assert_binding "1/137"

function assert_command() {
  result="$(./dotenv)"
  [[ "${result}" == "$1" ]] && log "Command works as expected ($1)." || die "Env did not propagate to command.\\nExpected: $1\\nGot: ${result}"
}

SCIE_BASE="$(mktemp -d)"
gc "${SCIE_BASE}"
export SCIE_BASE

export ZAPHOD=37
assert_command "37"

echo "SLARTIBARTFAST=00" > .env
assert_command "00"