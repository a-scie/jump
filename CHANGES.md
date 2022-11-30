# Release Notes

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
