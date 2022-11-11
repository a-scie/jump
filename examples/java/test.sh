# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

"${SCIE_JUMP}" "${LIFT}"
gc "${PWD}/coursier"


# Verify arbitrary json is allowed and preserved outside the root "scie" key.
test "3more" = "$(
  SCIE=inspect ./coursier | jq -r '(.custom.arbitrary | tostring) + .more[1]'
)"

# Verify byte-wise identical pack -> split -> pack round tripping.
SCIE="split" ./coursier split
gc "${PWD}/split"

sha256 coursier* > split/coursier.sha256
cd split && ./scie-jump
sha256 --check coursier.sha256
sha256 coursier* ../coursier*

time RUST_LOG=debug ./coursier version
time ./coursier java-home
time ./coursier launch org.pantsbuild:jar-tool:0.0.17 \
  -M org.pantsbuild.tools.jar.Main -- -h
