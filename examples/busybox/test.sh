# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

# shellcheck source=../common.sh
source "${COMMON}"
trap gc EXIT

check_cmd diff mktemp

OUTPUT="$(mktemp)"
gc "${OUTPUT}"


# Test no boot commands error.
"${SCIE_JUMP}" "${LIFT}"
gc "${PWD}/no-commands${EXE_EXT}"

./no-commands 2>"${OUTPUT}" && die "Expected ./no-commands to fail to execute."
diff -u \
<(cat <<EOF
Error: The ./no-commands${EXE_EXT} scie is malformed - it has no boot commands.

You might begin debugging by inspecting the output of \`SCIE=inspect ./no-commands${EXE_EXT}\`.
EOF
) "${OUTPUT}"


# Test no named commands error.
"${SCIE_JUMP}" "default-only-lift.${OS_ARCH}.json"
gc "${PWD}/default-only${EXE_EXT}"

diff -u <(echo "3.11.5") <(./default-only)

SCIE_BOOT=dne ./default-only 2>"${OUTPUT}" && die "Expected SCIE_BOOT=dne ./default-only to fail to execute."
diff -u \
<(cat <<EOF
Error: \`SCIE_BOOT=dne\` was found in the environment but "dne" does not correspond to any default-only commands.

The ./default-only scie contains no alternate boot commands.
EOF
) "${OUTPUT}"


# Test named commands only help is displayed for all commands when no named commands have
# descriptions - a bare BusyBox.
"${SCIE_JUMP}" "named-commands-only-no-desc-lift.${OS_ARCH}.json"
gc "${PWD}/named-commands-only-no-desc${EXE_EXT}"

diff -u <(echo "foo") <(SCIE_BOOT=foo ./named-commands-only-no-desc)
diff -u <(echo "foo") <(./named-commands-only-no-desc foo)
diff -u <(echo "bar") <(SCIE_BOOT=bar ./named-commands-only-no-desc)
diff -u <(echo "bar") <(./named-commands-only-no-desc bar)

SCIE_BOOT=dne ./named-commands-only-no-desc 2>"${OUTPUT}" && die "Expected SCIE_BOOT=dne ./named-commands-only-no-desc to fail to execute."
diff -u \
<(cat <<EOF
Error: \`SCIE_BOOT=dne\` was found in the environment but "dne" does not correspond to any named-commands-only-no-desc commands.

Please select from the following boot commands:

foo
bar

You can select a boot command by setting the SCIE_BOOT environment variable or else by passing it as the 1st argument.
EOF
) "${OUTPUT}"


# Test named commands only help is displayed only for commands with descriptions.
"${SCIE_JUMP}" "named-commands-only-with-desc-lift.${OS_ARCH}.json"
gc "${PWD}/named-commands-only-with-desc${EXE_EXT}"

diff -u <(echo "foo") <(SCIE_BOOT=foo ./named-commands-only-with-desc)
diff -u <(echo "foo") <(./named-commands-only-with-desc foo)
diff -u <(echo "bar") <(SCIE_BOOT=bar ./named-commands-only-with-desc)
diff -u <(echo "bar") <(./named-commands-only-with-desc bar)
diff -u <(echo "ran baz") <(SCIE_BOOT=runs-baz ./named-commands-only-with-desc)
diff -u <(echo "ran baz") <(./named-commands-only-with-desc runs-baz)

SCIE_BOOT=dne ./named-commands-only-with-desc 2>"${OUTPUT}" && die "Expected SCIE_BOOT=dne ./named-commands-only-with-desc to fail to execute."
diff -u \
<(cat <<EOF
Error: \`SCIE_BOOT=dne\` was found in the environment but "dne" does not correspond to any named-commands-only-with-desc commands.

Please select from the following boot commands:

foo       Prints foo.
runs-baz  Runs baz.

You can select a boot command by setting the SCIE_BOOT environment variable or else by passing it as the 1st argument.
EOF
) "${OUTPUT}"


# Test a mixed-mode scie with a default command, named commands with descriptions and named commands
# with no descriptions (hidden commands).
"${SCIE_JUMP}" "mixed-no-default-desc-lift.${OS_ARCH}.json"
gc "${PWD}/mixed-no-default-desc${EXE_EXT}"

diff -u <(echo "3.11.5") <(./mixed-no-default-desc)
diff -u <(echo "3.11.5") <(./mixed-no-default-desc "1st arg goes to default command which ignores all args.")
diff -u <(echo "foo") <(SCIE_BOOT=foo ./mixed-no-default-desc)
diff -u <(echo "bar") <(SCIE_BOOT=bar ./mixed-no-default-desc)
diff -u <(echo "ran baz") <(SCIE_BOOT=runs-baz ./mixed-no-default-desc)

SCIE_BOOT=dne ./mixed-no-default-desc 2>"${OUTPUT}" && die "Expected SCIE_BOOT=dne ./mixed-no-default-desc to fail to execute."
diff -u \
<(cat <<EOF
Error: \`SCIE_BOOT=dne\` was found in the environment but "dne" does not correspond to any mixed-no-default-desc commands.

The scie's overall description.

Please select from the following boot commands:

<default> (when SCIE_BOOT is not set in the environment)
foo                                                       Prints foo.
runs-baz                                                  Runs baz.

You can select a boot command by setting the SCIE_BOOT environment variable.
EOF
) "${OUTPUT}"


# Test default command with its own description.
"${SCIE_JUMP}" "mixed-with-default-desc-lift.${OS_ARCH}.json"
gc "${PWD}/mixed-with-default-desc${EXE_EXT}"

SCIE_BOOT=dne ./mixed-with-default-desc 2>"${OUTPUT}" && die "Expected SCIE_BOOT=dne ./mixed-with-default-desc to fail to execute."
diff -u \
<(cat <<EOF
Error: \`SCIE_BOOT=dne\` was found in the environment but "dne" does not correspond to any mixed-with-default-desc commands.

The scie's overall description.

Please select from the following boot commands:

<default> (when SCIE_BOOT is not set in the environment)  Prints the Python version.
foo                                                       Prints foo.
runs-baz                                                  Runs baz.

You can select a boot command by setting the SCIE_BOOT environment variable.
EOF
) "${OUTPUT}"
