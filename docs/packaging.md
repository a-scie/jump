# Packaging a scie executable

In order to use the `scie-jump` as your application launcher, you need to 1st package it together
with your application code and support binaries into a single file scie executable. Packaging can be
accomplished with the `cat` command line tool alone, but you can also use the `scie-jump` itself to
make this a bit easier. In either case you'll first need the following:

1. A copy of the scie-jump executable built for the platform your scie executable will be targeting.
2. All the files you want to package in your application.
3. A lift manifest describing one or more commands the scie executable will be able to execute using
   those files.

There is probably a pre-built `scie-jump` binary for the platform you're targeting in our
[releases](https://github.com/a-scie/jump/releases) that you can download. If not, you can clone
this project on a machine of your target platform and run `cargo run --release -p package dist` and
a `scie-jump` binary for that platform will be deposited in the `dist/` directory. See the
[contributing guide](../CONTRIBUTING.md) for more on the development environment setup if you're not
already setup for Rust development.

As for the files needed to compose your application, that varies! The `scie-jump` is language
agnostic and can package a scie for any interpreted language provided you can find or create a
reasonably hermetic interpreter distribution for the platforms you want to target.

Some common languages and runtimes that are known to work when packaged as scies include:

+ Python: You might use [Python Build Standalone](
  https://github.com/indygreg/python-build-standalone/releases) as a source for your portable Python
  distribution and [Pex](https://github.com/pantsbuild/pex/releases) to package up your code and
  dependencies.
+ JavaScript: You might use [Node.js](https://nodejs.org/en/download/) for your runtime
  distribution and the `node_modules` directory populated by an `npm install` for your application
  code.
+ JVM: you might use a JVM packaged by any number of vendors including Oracle's [OpenJDK](
  https://jdk.java.net/19/) and an executable deploy jar that most JVM build systems can produce for
  you.

There are examples of all of these in the [examples](../examples/README.md) directory that you can
examine for more details.

For the purposes of this example, we'll package [Coursier](https://get-coursier.io/), a popular JVM
application for resolving JVM project dependencies, for Linux X86_64. Although Coursier ships native
binaries using [Graal](https://www.graalvm.org/22.2/reference-manual/java/compiler/), we'll provide
a more basic scie as the native launcher[^1]. Coursier just requires its executable deploy jar,
found as `coursier.jar`[here](https://github.com/coursier/launchers/). Additionally, a JVM will be
needed to execute that jar. We grab a JDK from Amazon [here](
https://docs.aws.amazon.com/corretto/latest/corretto-11-ug/downloads-list.html).

## Setting the `scie-jump` boot-pack

With the `coursier.jar` and JDK in hand, we just need to author a lift manifest that describes the
files and commands our scie needs to run. That looks like this:

```json
{
  "scie": {
    "lift": {
      "name": "coursier",
      "files": [
        {
          "name": "amazon-corretto-11.0.17.8.1-linux-x64.tar.gz",
          "key": "jdk"
        },
        {
          "name": "coursier.jar"
        }
      ],
      "boot": {
        "commands": {
          "": {
            "exe": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin/java",
            "args": [
              "-jar",
              "{coursier.jar}"
            ],
            "env": {
               "=JAVA_HOME": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64",
               "=PATH": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin:{scie.env.PATH}"
            }
          }
        }
      }
    }
  }
}
```

The basic structure is a JSON object with a top-level "scie" field that the lift manifest lives
under. The document can have any other keys inserted at the top level if your application or any of
its components need extra metadata (More on how to access that metadata below).

### Required fields
In the "lift" manifest, there are 3 required fields:

1. The "name" will be the name of the binary produced when the scie is packaged by the `scie-jump`
   boot-pack.
2. The "files" are a list of the files your scie boot commands need to function. Each file is a JSON
   object that just requires a "name" field whose value is the path of the file relative to the lift
   manifest. Here we've downloaded our files as siblings to the manifest `lift.json` file.
3. Finally, the "boot.commands" are the commands that your scie executable will be able to run and
   these are stored in an object keyed by the command name. You make a command the default one
   executed when the scie binary is run by giving it an empty name (""). Here we have just one
   command and have done that so that it runs by default (More on adding more commands and selecting
   them at runtime below). Each command object requires an "exe" that is the path of the executable
   to run. Since your scie will be packaged as a single file executable, it will need to unpack the
   files you have added to it on first boot. By default, it will do this in the `nce` cache
   directory of the user running the scie, but your command should insulate itself from the details
   of exactly where things are unpacked by using placeholders. Placeholders come in a few varieties,
   but the most common is a file placeholder. This is just the name of a file in the "files" section
   surrounded by brackets, e.g.: `{amazon-corretto-11.0.17.8.1-linux-x64.tar.gz}`. That placeholder
   will be expanded to the full path of the unpacked tarball on the local system when the command
   runs. If the name is a bit unwieldy, as it is in this case, you can add a "key" field to the file
   object and reference that key value instead. This is what we do in the example above, shortening
   the JDK placeholder to just `{jdk}`.

### Optional fields

Files and commands can have additional configuration metadata described.

For files, you can supply a "size" and sha256 "hash". Without these the boot-pack will calculate
them, but you may want to set them in advance as a security precaution. The `scie-jump` will refuse
to operate on any file whose size or hash do not match those specified. You can also manually
specify a file "type". By default, the boot-pack detects the file type based on the file extension.
If the file is a directory, it gets zipped up and later re-extracted at boot time. If it's a zip,
tar or any of the various forms of compressed tarballs (`tar.gz`, `tar.zst`, etc.), the archive will
be extracted and unpacked at boot time. Any other file is treated as a blob and is only extracted at
boot time; no unpacking is performed. In the example above we accept the defaults; so the JDK
tarball is extracted and unpacked at runtime and the jar, although unpackable since jars are zips,
is treated as a blob and extracted as a single file at runtime. You can also set a "source" field to
have a file be materialized by a binding command (see below for more details on binding commands)
instead of being stored and materialized from within the scie directly. When a "source" is specified
it should take the value of a binding command name and the corresponding binding command should
accept a file "name" as an argument and produce the corresponding file's bytes on stdout. Any file
with a source field set like this will not be packed by the boot pack; so it should have all fields
specified including "size", "hash" and "type". It will be materialized just in time when 1st needed
at runtime by executing the source binding command.

For commands, you can specify additional command line "args" to always pass to the "exe" as well as
environment variables to set in the ambient runtime environment via the "env" object. An environment
variable name that begins with "=" will have the "=" stripped and will overwrite any ambient
environment variable of the same name. Without the leading "=" the environment variable will be set
only if not already present in the ambient runtime environment.

You can also supply a list of commands under "scie.lift.boot.bindings". These commands are objects
with the same format as the "scie.lift.boot.commands" but they are not directly runnable by the end
user of the scie. Instead, they serve the role of performing 1-time installation actions that can be
requested by other commands that rely upon them via a `{scie.bindings.<binding command name>}`
placeholder. The named binding command will be run (successfully) exactly once as tracked by a lock
file maintained by the scie jump. The binding command will generally want to use the
`{scie.bindings}` to request the path of a directory (housed in the `nce` cache and namespaced by
the lift manifest hash) set aside for that scie alone. The binding command is guaranteed it will be
the only command operating against that directory when it is invoked.

N.B.: Since the scie-jump only maintains cooperative control over the contents of the `nce` cache,
care should be taken when designing boot binding commands. If the scie is run in a Docker container
build step, you have a wider guaranty of non-interference. If the scie is run in an open environment
though, you may need to account for conflicting processes running in parallel to your binding
command and invalidating its work or assumptions about the state of the wider filesystem.

### Executing the boot pack

With a `scie-jump` in hand, your application files downloaded and the lift manifest written,
building a scie is as simple as:
```
$ ./scie-jump
/home/jsirois/dev/a-scie/jump/docs/base/lift.json: /home/jsirois/dev/a-scie/jump/docs/base/coursier
```

Here the `scie-jump` is a sibling of the downloaded files and the lift manifest shown above named
`lift.json`, which is the default lift manifest name. If the lift manifest name is different, or its
in a different directory, just specify its path; e.g.: `./scie-jump apps/foo-lift.json`. The files
the lift manifest lists will still be searched for relative to the lift manifest's location
regardless of where you execute the `scie-jump` from.

### Using the scie

You now have a single file native executable:
```
$ file coursier
coursier: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, BuildID[sha1]=bf278dd7a23344e4f6912975827bc3ded31ddcbe, for GNU/Linux 3.2.0, stripped
```

You can run it without a JVM installed:
```
$ java
Command 'java' not found, but can be installed with:
sudo apt install openjdk-11-jre-headless  # version 11.0.16+8-0ubuntu1~22.04, or
sudo apt install default-jre              # version 2:1.11-72build2
sudo apt install openjdk-18-jre-headless  # version 18~36ea-1
sudo apt install openjdk-8-jre-headless   # version 8u312-b07-0ubuntu1
sudo apt install openjdk-17-jre-headless  # version 17.0.3+7-0ubuntu0.22.04.1

$ ./coursier version
2.1.0-M7-39-gb8f3d7532
```

You can inspect its contents. Since the last file was a zip (coursier.jar), we can treat it like a
zip and inspect the class file portion of its contents:
```
$ zipinfo coursier | tail
warning [coursier]:  196557299 extra bytes at beginning or within zipfile
  (attempting to process anyway)
-rw----     2.0 fat     3304 bl defN 16-Nov-07 16:28 catalysts/macros/TypeTagMacros$$typecreator1$1.class
-rw----     2.0 fat      827 bl defN 16-Nov-07 16:28 catalysts/macros/TypeTagM$.class
-rw----     2.0 fat     1019 bl defN 16-Nov-07 16:28 catalysts/macros/ClassInfo.class
-rw----     2.0 fat     2930 bl defN 16-Nov-07 16:28 catalysts/macros/ClassInfoMacros$$typecreator1$1.class
-rw----     2.0 fat      481 bl defN 16-Nov-07 16:28 catalysts/macros/ClassInfo$.class
-rw----     2.0 fat     1849 bl defN 16-Nov-07 16:28 catalysts/macros/TypeTagM.class
-rw----     1.0 fat        0 b- stor 22-Sep-28 10:14 META-INF/services/
-rw----     2.0 fat       34 bl defN 22-Feb-21 13:36 META-INF/services/coursier.jniutils.NativeApi
-rw----     2.0 fat       32 bl defN 22-Sep-28 10:14 META-INF/services/org.slf4j.spi.SLF4JServiceProvider
16465 files, 124279609 bytes uncompressed, 39282108 bytes compressed:  68.4%
```

And you can also inspect the lift manifest with basic tools since the boot-pack uses a
`--single-lift-line` by default:
```
$ tail -1 coursier
{"scie":{"lift":{"name":"coursier","files":[{"name":"amazon-corretto-11.0.17.8.1-linux-x64.tar.gz","key":"jdk","size":194998805,"hash":"9628b1c1ec298a6e0f277afe383b342580086cfd7eee2be567b8d00529ca9449","type":"tar.gz"},{"name":"coursier.jar","size":42284054,"hash":"a1799d6418fbcbad47ac9e388affc751b4fc2d8678f89c332df9592d2dd3a202","type":"blob"}],"boot":{"commands":{"":{"exe":"{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin/java","args":["-jar","{coursier.jar}"],"env":{"=JAVA_HOME":"{jdk}/amazon-corretto-11.0.17.8.1-linux-x64","=PATH":"{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin:{scie.env.PATH}"}}}}},"jump":{"size":1557952,"version":"0.1.10"}}}
```

You can also inspect the lift manifest with the built in `inspect` tool by setting the `SCIE`
environment variable, e.g.: `SCIE=inspect ./coursier`
```json
{
  "scie": {
    "lift": {
      "name": "coursier",
      "files": [
        {
          "name": "amazon-corretto-11.0.17.8.1-linux-x64.tar.gz",
          "key": "jdk",
          "size": 194998805,
          "hash": "9628b1c1ec298a6e0f277afe383b342580086cfd7eee2be567b8d00529ca9449",
          "type": "tar.gz"
        },
        {
          "name": "coursier.jar",
          "size": 42284054,
          "hash": "a1799d6418fbcbad47ac9e388affc751b4fc2d8678f89c332df9592d2dd3a202",
          "type": "blob"
        }
      ],
      "boot": {
        "commands": {
          "": {
            "exe": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin/java",
            "args": [
              "-jar",
              "{coursier.jar}"
            ],
            "env": {
              "=JAVA_HOME": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64",
              "=PATH": "{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin:{scie.env.PATH}"
            }
          }
        }
      }
    },
    "jump": {
      "size": 1557952,
      "version": "0.1.10"
    }
  }
}
```

If you've added non-default commands you can invoke them by name using the `SCIE_BOOT` environment
variable, e.g.: `SCIE_BOOT=some_other_command ./coursier`. If there is no default command defined
and the `SCIE_BOOT` environment variable is not set, a help screen will be printed listing all the
commands you defined in the lift manifest. You can add a "lift.description" to provide overall help
in this help page as well as a "description" for each command to provide help displayed after the
command name.

This style of multi-command scie with no default command is called a [BusyBox](
https://busybox.net/), and it functions like one. Instead of using `SCIE_BOOT` to address a command,
you can also pass the command name as the 1st argument; e.g: `./cousier some_other_command`.
Finally, you can re-name the binary (or make a hard link to it) and if the name of the binary
matches a contained BusyBox command name, that command will be run.

## Scie `cat` assembly

As an alternative to using the boot pack, you can use the `cat` utility to build the scie we built
above as well. The big differences are:

1. The lift manifest needs to be fully specified like the one shown above via
   `SCIE=inspect coursier`. In particular file size and hashes must be present as well as the
   information describing the scie-jump you're using in the "scie.jump" field.
2. The last file in the "files" list must be a zip[^2]. This is a requirement of the scie format.

Having written a fully specified lift manifest like the one above by hand though, and having ensured
the last file is a zip, scie cat assembly is just:
```
cat \
   amazon-corretto-11.0.17.8.1-linux-x64.tar.gz \
   coursier.jar \
   lift.json > coursier
chmod +x coursier
```

That scie will have the lift manifest in exactly the form you wrote it as its tail. That's generally
not very friendly for command line inspection assuming your JSON is written in a multi-line style
for readability. To package the scie for easier inspection, you can modify the `cat`command above
like so:
```
cat \
   amazon-corretto-11.0.17.8.1-linux-x64.tar.gz \
   coursier.jar \
   <(echo) \
   <(jq -c . lift.json) > coursier
chmod +x coursier
```

That extra bit of typing adds the lift manifest as a single line JSON document on its own line which
gains the ability to blindly issue `tail -1 coursier | jq .` to inspect the lift manifest of the
scie. Either way though, since the scie is powered by a `scie-jump` in its tip, you can also issue
`SCIE=inspect coursier` as in the boot-pack example.

## Advanced placeholders

Further placeholders you can use in command "exe", "args" and "env" values include:

+ `{scie.env.<env var name>[=<default env var value>]}`: This expands to the value of the env var
  named. If the env var is not in the ambient runtime environment and no default env var value is
  specified it expands to the empty string (""). If a default env var value is specified, it is
  used. The default env var value specified can itself be a placeholder, in which case that is
  expanded (recursively) to obtain the default value. For example,
  `{scie.env.FOO={scie.env.BAR=42}}` would evaluate to "bar" if the "FOO" env var was not set but
  the "BAR" env var was set to "bar" and it would evaluate to "42" if neither the "FOO" nor "BAR"
  env vars were set.
+ `{scie.lift}`: This expands to the path to the lift manifest, which is extracted to disk when you
  use this placeholder. This can be used to read custom metadata stored in the lift manifest.

[^1]: The binaries that Coursier releases are single-file true native binaries that do not require a
JVM at all. As such they are ~1/3 the size of the scie we build here, which contains a full JDK
along with the Coursier executable jar. Those binaries are also much faster, ~100x for
`./coursier version`. Although the scie-jump only takes ~200 microseconds to launch the JVM that
runs Coursier, the JVM startup and warmup overheads are high. You pay that cost in painfully obvious
ways in a command line app that runs quicly and exits! This is all just to point out you should
analyze and measure your use case for applicability when considering making a scie of it.

[^2]: The `scie-jump` has some smarts when it comes to file lists that do not end in a zip. It
creates an extra file called the `scie-tote` that is a zip that stores all the files above it inside
as STORED (uncompressed) entries. You need not be aware of this, the scie still functions like you'd
expect. Its only when using a tool like `zipinfo` to inspect your scie executable that you'll notice
a zip file entry for each of the files you specified.