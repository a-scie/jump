# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd mktemp

gc "${PWD}/cowsay"
"${SCIE_JUMP}" "${LIFT}"

# Force downloads to occur to exercise the load functionality even if nce cache has the JDK and the
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

# Specify an alternate URL database via load_dotenv capability.
gc "${PWD}/.env"
echo GET_CONFIG=alt-metadata.json > .env
./cowsay "Alt Curl!"

source .env
grep "${GET_CONFIG}" "${GET_LOG_CONFIG}"

# Motivated by: https://github.com/pantsbuild/scie-pants/issues/307
# And ammended by: https://github.com/a-scie/jump/issues/166
# shellcheck disable=SC2016 # We with this text to be included verbatim in the .env file.
echo 'PYTHONPATH="/foo/bar:$PYTHONPATH"' >> .env
./cowsay "Should succeed!"

# See motivating case here: https://github.com/arniu/dotenvs-rs/issues/4
cat << EOF >> .env
A=foo bar
B="notenough
C='toomany''
D=valid
export NOT_SET
E=valid
EOF
if ./cowsay "Should fail!"; then
  die "The expected .env file loading failure did not happen."
else
  log "The expected .env file loading failure was successfully propagated."
fi
