# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

"${SCIE_JUMP}" "${LIFT}"
gc "${PWD}/node.js${EXE_EXT}"

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
applets=()
for applet in $(SCIE="list" ./node.js); do
  applets+=("${applet}")
done
SCIE="install" ./node.js -s .
for applet in ${applets[*]}; do
  gc "${PWD}/${applet}"
done

cat <<EOF | ./node --input-type=module -
import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';

assert.equal(path.basename(fs.realpathSync(process.env.SCIE)), 'node.js${EXE_EXT}');
assert.equal(path.basename(process.env.SCIE_ARGV0), 'node${EXE_EXT}');
EOF

./npm install cowsay
gc "${PWD}/node_modules" "${PWD}/package.json" "${PWD}/package-lock.json"

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
