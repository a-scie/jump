# Release Notes

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
