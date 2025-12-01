# Release Notes

## 1.9.0

This release adds glibc `scie-jump` binaries for Linux aarch64 & x86_64. The default binaries for
these platforms are still the musl libc static binaries, but `-{gnu,musl}-linux-<arch>` binaries
are now produced to allow explicitly selecting the desired libc implementation.

## 1.8.3

This release is a follow-up to 1.8.2 that fixes lookup of custom scie-jump versions when the custom
scie-jump is for a foreign platform. Since there is no way to look those versions up, the old, buggy
behavior of recording the current scie-jump version for old, foreign scie-jumps is used. A warning
is logged in this case now.

## 1.8.2

This release fixes a bug recording the correct scie-jump version in the lift manifest when using 
the scie-jump boot pack with a custom scie-jump (`-sj`|`--jump`|`--scie-jump`).

## 1.8.1

This release fixes a bug in `.env` handling. Previously, when `"load_dotenv" = true` was configured
in the lift manifest and a `.env` was present, its values were read but only applied to `scie.env`
substitutions in the lift manifest itself and not propagated in binding or command environments
inherited by the associated processes.

## 1.8.0

This release adds support for Linux riscv64.

## 1.7.0

Add support for the `{scie.argv0}` placeholder and plumb `SCIE` and `SCIE_ARGV0` env vars into
the `{scie.env.SCIE}` and `{scie.env.SCIE_ARGV0}` env var placeholders respectively. Previously
these two env vars were set and observable by the executing command, but they were not available
for env var substitutions.

## 1.6.1

The `SCIE=split` file selection feature now warns when selected files can't be found in the scie.

## 1.6.0

This release adds support for restricting `SCIE=split` to a subset of files in the scie as well as
adding support for a `-n` / `--dry-run` mode.

In addition, the bare `scie-jump` now displays a help message when executed without arguments and
there is no `lift.json` manifest in the current directory. Help can also be requested via
`-h` / `--help` and the `scie-jump` version can be displayed via `-V` / `--version`.

## 1.5.0

This release adds support for Linux powerpc64le.

## 1.4.1

This release fixes the `scie.platform` and `scie.platform.arch` placeholders for armv7l.

## 1.4.0

This release adds support for Linux s390x.

## 1.3.0

This release adds support for Linux ARM (armv7l and armv8l 32 bit mode).

## 1.2.0

This release adds support for Windows ARM64.

## 1.1.1

This release fixes missing attestations for Linux ARM64 artifacts.

## 1.1.0

This release updates various dependencies as well as upgrading to Rust
1.79.0. In addition, this is the first release to include artifact
attestations in Sigstore.

## 1.0.0

This release updates various dependencies as well as upgrading to Rust
1.78.0 and dropping support for Windows versions prior to Windows 10.

## 0.14.0

Change `.env` parsing libraries to gain support for double quoted values with variable
substitution; e.g.: the `.env` line `PYTHONPATH="/Users/A. Space:$PYTHONPATH"` now has the
`$PYTHONPATH` portion of the value substituted.

## 0.13.3

Ensure liblzma is statically linked.

## 0.13.2

When `load_dotenv` is requested, propagate errors loading any `.env` file found.

## 0.13.1

Support regex removal of env vars with non-utf8 names in commands.

## 0.13.0

This release improves the help screen for BusyBox scies with more clear messages for the various
causes of boot command selection failure. It also adds the ability to hide internal-only named boot
commands by omitting a description for those commands (This only kicks in if at least one named
command has a description).

## 0.12.0

This release adds support for using placeholders in the `scie.lift.base` lift manifest value as well
as the corresponding `SCIE_BASE` runtime control env var. A new placeholder is exposed in support of
establishing custom scie base `nce` cache directories that respect the target OS user cache dir
structure in the form of `{scie.user.cache_dir=<fallback>}`. Typically, this placeholder will expand
to `~/Library/Caches` on macOS, `~\AppData\Local` on Windows and `~/.cache` on all other Unix
systems unless over-ridden via OS-specific means or else unavailable for some reason, in which case
the supplied `<fallback>` is used.

## 0.11.1

This release fixes a bug handling environment variable removal via regex when the environment
contains non-utf8 entries.

## 0.11.0

Support is added for `{scie.env.*}` placeholders referring to environment variables defined in the
lift command environment in addition to the existing support for referring to environment variables
defined in the ambient environment.

## 0.10.0

In addition to the `SCIE` environment variable being exposed to scies, `SCIE_ARGV0` is now exposed
as well. On Unix systems this value can differ from `SCIE` and can be used to detect the name of the
scie executable launched by the user. Although the `scie-jump` uses this internally to allow for
BusyBox style dispatch based on symlinks, exposing `SCIE_ARGV0` allows non BusyBox scies to do the
same.  See the [packaging guide](docs/packaging.md) for more details on environment variables
supported by `scie-jump`.

## 0.9.0

Support is added for specifying an alternate `scie-jump` binary to embed in the scie tip when
executing a `scie-jump` boot-pack. This allows "cross-building" a scie for another platform.

## 0.8.0

Support is added for `.env` file loading for scies that opt-in via the new `scie.lift.load_dotenv`
boolean lift manifest field. This release also fixes `SCIE=split` to work with scies that include
sourced files (ptex'ed scies).

## 0.7.2

This release fixes a bug in argv0 / current exe determination handling that led to a scie-jump
being fooled by a file in `CWD` with the same name as the active scie invoked from elsewhere via
the `PATH`.

## 0.7.1

This release fixes `SCIE_BOOT` re-directions to clear the `SCIE_BOOT` environment variable before
executing the `SCIE_BOOT` selected command. This avoids the need for these commands to clear the
`SCIE_BOOT` environment variable when re-executing the `SCIE` to avoid infinite loops.

## 0.7.0

This release brings support for removing env vars to command definitions. Now, in addition to
defaulting a variable with a `"NAME": "VALUE` entry in the `"env"` object and unconditionally
writing a variable with `"=NAME": "VALUE"`, ambient environment variables can be removed by adding
an `"env"` entry object with a `null` value. See the [packaging guide](docs/packaging.md) for more
details.

## 0.6.0

This release brings various improvements and features whose need was fleshed out by the
[scie-pants](https://github.com/pantsbuild/scie-pants) project.

Support is added for:

+ New placeholders:

  - `{scie.base}`: The current `SCIE_BASE`.
  - `{scie.files.<name>}`: Another way to say `{<name>}`.
  - `{scie.files.<name>:hash}`: The sha256 hash of the named file.
  - `{scie.bindings.<name>:<key>}`: The output named `<key>` of the named binding.

  For the last, bindings have access to a `SCIE_BINDING_ENV` environment variable pointing to a
  file they can write `<key>=<value>` lines to propagate binding information via the
  `{scie.bindings.<name>:<key>}` placeholder.

+ Environment sensitive bindings:

  Binding commands are now locked based on the content hash of their `env`, `exe` and `args`. This
  allows for binding commands that are still guaranteed to run only once, but once for each unique
  context.

## 0.5.0

This release brings fully static binaries for Linux with zero runtime
linkage by switching the Linux targets to use musl. As part of this
switch, the Rust toolchain used is stabilized to stable / 1.65.0.

## 0.4.0

This release beings support for `{scie.env.*}` defaults which allows for ptex'ed scies that opt
in to having file urls over-ridden behind corporate firewalls as the motivating use case.

The default `nce` cache location is also updated to follow conventions for user cache directories
on most operating systems. The defaults are now:
+ Linux and non macOS Unix: `~/.cache/nce` with respect for `XDG*` configuration.
+ macOS: `~/Lirary/Caches/nce`
+ Windows: `~\AppData\Local\nce`

## 0.3.9

This release fixes a bug that caused the scie-tote in scies using one to always be extracted and
thus impact startup latency on warm runs.

## 0.3.8

This release brings support for files with sources other than the scie itself. This allows for
shipping skinny skis that later materialize certain files from the internet or elsewhere just when
needed at runtime.

## 0.2.1

This release fixes blob file locks in the presence of boot bindings that delete blobs as part of
their post install preparations.

## 0.2.0

This release brings support for boot bindings: commands that will be run exactly once to perform
any needed installation operations.

## 0.1.11

The 1st release including macOS aarch64 binaries.

## 0.1.10

The 1st public release of the project.
