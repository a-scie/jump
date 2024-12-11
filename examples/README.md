# examples

The top level directories each contain an example of a scie that can be assembled and run using the
`scie-jump`.

## Structure

There is a [`run.sh`](run.sh) script that can be used to run the examples. By default, it will run
all of them, but you can pass specific example directory names to have it just run those. Some
examples only work for certain platforms.

The `run.sh` script looks for a `.fetch` file in the `examples` directory with the same stem as the
example directory name and fetches each non-blank URL line in the file into the example directory.
If writing a new example, use a  top-level `.fetch` file like this to list the URLS of
platform-independent items that should be fetched for the example. Java jars are a good example of
this sort of artifact.

The `run.sh` script then looks for the default lift manifest for the example. The default lift
manifest is named `lift.<os>-<arch>.json` in the example directory where `<os>` is currently one of
`linux`, `macos` or `windows` and `<arch>` is currently one of `s390x` (Linux only), `armv7l` (Linux
only), `aarch64` or `x86_64`. If that lift manifest has a top-level "fetch" key, it's expected to
have a list of URL string values and all of those will be fetched. Use this facility when writing a
new example to ensure platform-specific artifacts are fetched - typically the interpreter
distribution being used by the example.

Inside the example's directory there should be a `test.sh` bash script that need not be executable.
It will be run by the `run.sh` script using `bash -eou pipefail test.sh` with the example's
directory as the `PWD` if there is a default lift manifest for the current platform; otherwise the
example will be skipped with a warning. The script will have the following available in the
environment when run:

+ `OS`: This is the `<os>` value described above for the current operating system.
+ `ARCH`: This is the `<arch>` value described above for the current processor architecture.
+ `OS_ARCH`: This is the `<os>-<arch>` value described above and can be used to operate on the
  appropriate lift manifest file for the current platform.
+ `COMMON`: The absolute path of the [`common.sh`](common.sh) script for sourcing. This script is a
  sibling of `run.sh` and contains useful functions for the test to use.
+ `SCIE_JUMP`: The absolute path of a `scie-jump` binary built for the current platform.
+ `LIFT`: The relative path of the default lift manifest for the current platform.
+ `EXE_EXT`: The extension to append to binaries. This is blank ("") except for Windows where it's
             ".exe".
+ `NEWLINE`: The newline characters for the current OS. This is "\n" except for Windows where it's
             "\r\n"

## Use

Simply run `examples/run.sh [example name]*`. You can also pass `--no-gc` if you want artifacts
created during the test run to stick around for inspection. Do note that these will likely need to
be cleaned up manually for the next example test run to succeed. For more help on the `run.sh` tool
just pass `-h` or `--help`.
