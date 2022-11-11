# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

"${SCIE_JUMP}" "lift.${OS_ARCH}.json"
gc "${PWD}/pants" "${PWD}/.pants.d" "${PWD}/.pids"

time RUST_LOG=trace ./pants --no-pantsd -V
time ./pants --no-pantsd -V

# Use the built-in BusyBox functionality via env var.
SCIE_BOOT="inspect" ./pants interpreter --verbose --indent 2