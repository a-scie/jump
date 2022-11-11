# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

"${SCIE_JUMP}" "lift.${OS_ARCH}.json"
gc "${PWD}/pants" "${PWD}/.pants.d" "${PWD}/.pids"

time RUST_LOG=trace ./pants --no-pantsd -V
time RUST_LOG=debug ./pants --no-pantsd -V

# Use the built-in BusyBox functionality via env var.
SCIE_BOOT=repl ./pants -c 'from pants.util import strutil; print(strutil.__file__)'