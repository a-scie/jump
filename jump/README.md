# jump

The core logic of the scie-jump binary.

Most modules are self-explanatory but the relationship between [config](src/config.rs) and [lift](
src/lift.rs) and the overall scie-jump execution flow deserve to be fleshed out a bit since they
are key aspects of operation.

## The lift manifest

The configuration of a scie is provided in a json format defined in [config](src/config.rs). The
format is permissive on the input side allowing most fields to be elided. They will be calculated if
left out or else verified if specified. In either case, the end product used internally is always a
fully specified and eagerly or lazily verified depending on the flow. The fully specified model is
defined and hydrated in the [lift](src/lift.rs).

As an example, here is a minimal input lift manifest with two input files, a jdk and an executable
jar, and one default command that is what is executed when the assembled scie is run. It defines a
"native" coursier binary for Linux x86_64:
```json
{
  "scie": {
    "lift": {
      "boot": {
        "commands": {
          "": {
            "args": [
              "-jar",
              "{coursier.jar}"
            ],
            "env": {
              "=JAVA_HOME": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64",
              "=PATH": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin:{scie.env.PATH}"
            },
            "exe": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin/java"
          }
        }
      },
      "files": [
        {
          "key": "jdk",
          "name": "amazon-corretto-11.0.17.8.1-linux-x64.tar.gz"
        },
        {
          "name": "coursier.jar"
        }
      ],
      "name": "coursier"
    }
  }
}
```

That is reified to this fully specified lift manifest on ingestion via the scie-jump boot-pack:
```json
{
  "scie": {
    "jump": {
      "size": 1557952,
      "version": "0.1.9"
    },
    "lift": {
      "base": "~/.nce",
      "boot": {
        "commands": {
          "": {
            "args": [
              "-jar",
              "{coursier.jar}"
            ],
            "env": {
              "=JAVA_HOME": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64",
              "=PATH": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin:{scie.env.PATH}"
            },
            "exe": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin/java"
          }
        }
      },
      "files": [
        {
          "hash": "9628b1c1ec298a6e0f277afe383b342580086cfd7eee2be567b8d00529ca9449",
          "key": "jdk",
          "name": "amazon-corretto-11.0.17.8.1-linux-x64.tar.gz",
          "size": 194998805,
          "type": "tar.gz"
        },
        {
          "hash": "a1799d6418fbcbad47ac9e388affc751b4fc2d8678f89c332df9592d2dd3a202",
          "name": "coursier.jar",
          "size": 42284054,
          "type": "blob"
        }
      ],
      "name": "coursier"
    }
  }
}
```

Notably, file sizes, hashes and types were calculated automatically by the scie-jump boot-pack and
the details of the scie-jump used to build the scie were filled in as well along with the lift base
to use for file extraction.

## The scie-jump execution flow

The scie-jump main entry point calls into `prepare_boot` in [lib.rs](src/lib.rs) with the aim of
getting back a boot command to execute. The boot command is nominally the default (name of `""`)
user-defined command in the lift manifest, but it could also be a named user-defined command or a
scie-jump intrinsic command. The checking proceeds in order:

1. See if the scie-jump is bare in which case the only sensible thing to do is run the boot-pack.
2. Load the lift manifest from the scie tail via [lift.rs](src/lift.rs) if the sice-jump is embedded
   in a scie tip.
3. Check if `SCIE` is defined as an intrinsic command to run and dispatch if so.
4. Construct an execution [Context](src/context.rs) and ask it to calculate the selected
   user-defined command to execute. This may result in no selection in the case of a BusyBox scie.
5. If a user command was selected, have the [installer](src/installer.rs) prepare it by extracting
   any files not yet extracted and substituting their paths into placeholders in the command
   definition.
