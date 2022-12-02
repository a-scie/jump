# Contributing

The scie-jump is intended to be a simple, fast, and robust executable launcher. Any help pushing
forward those goals is very welcome. Thank you in advance for your time and effort.

## Development Environment

You'll need just a few tools to hack on the scie-jump:
+ If you're new to Rust you'll need to ensure you have its toolchain available to you. You might
  start with [`rustup`](https://rustup.rs/).
+ Integration tests are currently `bash`/~coreutils and `jq` driven. If you're on Windows you'll
  also need `pwsh`. The `bash` environment that comes with [Git for Windows](
  https://gitforwindows.org/) will suffice. If you're on a Unix system, you'll just need
  `bash`/~coreutils which you almost certainly already have. In either case you can get [`jq` here](
  https://stedolan.github.io/jq/download/).

## Development Cycle

You might want to open a [discussion](https://github.com/a-scie/jump/discussions) or [issue](
https://github.com/a-scie/jump/issues) to vet your idea first. It can often save overall
effort and lead to a better end result.

The code is run through the ~standard `cargo` gamut. Before sending off changes you should have:
+ Formatted the code (requires Rust nightly): `cargo +nightly fmt --all`
+ Linted the code: `cargo clippy --all`
+ Tested the code: `cargo test --all`

Additionally, you can run any existing integration tests using [`examples/run.sh`](examples/run.sh).
Learn more about those in the [README](examples/README.md).

The scie-jump binary can be built via `cargo build` but that does not produce a fully featured
`scie-jump`. For that you should instead use `cargo run -p package`. That will build the scie-jump
binary for the current machine to the `dist/` directory by default (run
`cargo run -p package -- --help` to find out more options). Two files will be produced there:
1. The scie jump binary: `scie-jump-<os>-<arch>(.<ext>)`
2. The scie jump fingerprint file: `scie-jump-<os>-<arch>(.<ext>).sha256`

The latter is primarily of use for the automated release process of scie-jump binaries.

When you're ready to get additional eyes on your changes, submit a [pull request](
https://github.com/a-scie/jump/pulls).

## Guiding Principles

There are just a few guiding principles to keep in mind as alluded to in the [README](README.md):
+ The scie-jump should be fast: It's currently launches warm scies in less than a millisecond and
  that should only improve.
+ The scie-jump should be small: It's currently ~1.5MB on Linux (the largest binary). It would be
  nice to not grow much larger.
+ The scie should be transparent and simple: Scie's should always be able to be constructed with
  just a text editor and `cat`.
+ The scie-jump lift manifest format should remain stable. If breaking changes need to be made or
  new features added the format should start respecting a format version and doing the right thing.
