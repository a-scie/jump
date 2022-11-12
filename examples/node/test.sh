# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

"${SCIE_JUMP}" "${LIFT}"
gc "${PWD}/node.js"

# Get help on scie boot commands.
SCIE="help" ./node.js

# Verify byte-wise identical pack -> split -> pack round tripping.
SCIE="split" ./node.js split
gc "${PWD}/split"

sha256 node.js* > split/node.js.sha256
cd split && ./scie-jump
sha256 --check node.js.sha256
sha256 node.js* ../node.js*
cd .. && rm -rf split

# Use the built-in BusyBox functionality via binary base name.
ln node.js "npm${EXE_EXT}"
./npm install cowsay
gc "${PWD}/npm" "${PWD}/node_modules" "${PWD}/package.json" "${PWD}/package-lock.json"

# Build a scie from another scie's tip-embedded scie-jump.
SCIE="boot-pack" ./node.js "cowsay-lift.${OS_ARCH}.json"
gc "${PWD}/cowsay.js" "${PWD}/node_modules.zip"
rm -rf npm node_modules* package*.json

./cowsay.js -b 'All the binaries belong to us!'

# Verify byte-wise identical pack -> split -> pack round tripping.
SCIE="split" ./cowsay.js split
sha256 cowsay.js* > split/cowsay.js.sha256
cd split && ./scie-jump
sha256 --check cowsay.js.sha256
sha256 cowsay.js* ../cowsay.js*
