# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd mktemp

gc "${PWD}/cowsay"
"${SCIE_JUMP}" "${LIFT}"

# Verify byte-wise identical pack -> split -> pack round tripping.
gc "${PWD}/split"
SCIE="split" ./cowsay split

sha256 cowsay* > split/cowsay.sha256
cd split && ./scie-jump
sha256 --check cowsay.sha256
sha256 cowsay* ../cowsay*
cd .. && rm -rf split

# Force downloads to occur to exercise the load functionality even if nce cache has the JDK and the
# cowsay jars already from other examples.
SCIE_BASE="$(mktemp -d)"
gc "${SCIE_BASE}"
export SCIE_BASE

time RUST_LOG=info ./cowsay "PTEX!"
time RUST_LOG=info ./cowsay "Local!"
