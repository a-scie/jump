# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

"${SCIE_JUMP}" "${LIFT}"
gc "${PWD}/pants" "${PWD}/.pants.d" "${PWD}/.pids"

# Observe initial install and subsequent short-circuiting of install activity.
time RUST_LOG=trace ./pants -V
time RUST_LOG=debug ./pants -V

# Use the built-in BusyBox functionality via env var.
SCIE_BOOT=repl ./pants -c 'from pants.util import strutil; print(strutil.__file__)'

# Confirm boot bindings re-run successfully when the lift manifest changes - which allocates a new
# boot bindings directory.
jq '
setpath(["extra"]; 42)
| setpath(["scie", "lift", "name"]; "pants-extra")
' "${LIFT}" > lift.json
gc "${PWD}/lift.json" "${PWD}/pants-extra"
"${SCIE_JUMP}"
time RUST_LOG=debug ./pants-extra -V
