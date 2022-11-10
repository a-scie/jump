# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd sha256sum

"${SCIE_JUMP}" "lift.${OS_ARCH}.json"
gc "${PWD}/coursier"


# Verify arbitrary json is allowed and preserved outside the root "scie" key.
test "3more" = "$(
  SCIE=inspect ./coursier | jq -r '(.custom.arbitrary | tostring) + .more[1]'
)"

# Verify byte-wise identical pack -> split -> pack round tripping.
SCIE="split" ./coursier split
gc "${PWD}/split"

sha256sum coursier* > split/coursier.sha256sum
cd split && ./scie-jump
sha256sum --check coursier.sha256sum
sha256sum coursier* ../coursier*

time RUST_LOG=debug ./coursier version
time ./coursier java-home
time ./coursier launch org.pantsbuild:jar-tool:0.0.17 \
  -M org.pantsbuild.tools.jar.Main -- -h
