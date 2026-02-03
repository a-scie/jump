# Release Process

## Preparation

### Version Bump and Changelog

1. Bump the version in at least [`Cargo.toml`](Cargo.toml) and `src/main.rs` and possibly other
   crates if they were modified.
2. Run `cargo test --all && cargo run -p package` to update [`Cargo.lock`](Cargo.lock) with the new
   version and as a sanity check on the state of the project.
3. Update [`CHANGES.md`](CHANGES.md) with any changes that are likely to be useful to consumers.
4. Open a PR with these changes and land it on https://github.com/a-scie/jump main.

## Release

### Push Release Tag

Sync a local branch with https://github.com/a-scie/jump main and confirm it has the version bump
and changelog update as the tip commit:

```
$ git log --stat -1 HEAD
Author: John Sirois <john.sirois@gmail.com>
Date:   Sun Nov 6 21:11:22 2022 -0800

    Prepare the 0.1.8 release.

    Fan in the GH release to trigger 1 Circle CI release.
    Fix the Circle CI release to be able to see workspace files.

 .circleci/config.yml          | 17 ++++++++++++-----
 .github/workflows/release.yml |  7 +++++++
 Cargo.lock                    |  4 ++--
 Cargo.toml                    |  2 +-
 jump/Cargo.toml               |  2 +-
 5 files changed, 23 insertions(+), 9 deletions(-)
```

Tag the release as `v<version>` and push the tag to https://github.com/a-scie/jump main:

```
$ git tag --sign -am 'Release 0.1.8' v0.1.8
$ git push --tags https://github.com/a-scie/jump HEAD:main
```

The release is automated and will create a GitHub Release page at
[https://github.com/a-scie/jump/releases/tag/v&lt;version&gt;](
https://github.com/a-scie/jump/releases) with binaries for Linux, Mac and Windows.

