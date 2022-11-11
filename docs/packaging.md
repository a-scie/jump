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

As for the files needed to compose your application, that varies! We'll package [Coursier](
https://get-coursier.io/), a popular JVM application for resolving JVM project dependencies for this
example. Although it ships native binaries using [Graal](
https://www.graalvm.org/22.2/reference-manual/java/compiler/), we'll provide a more basic scie as
the native launcher[^1]. Coursier just requires its executable fat jar, found as `coursier.jar` [here](
https://github.com/coursier/launchers/). Additionally, a JVM will be needed. We grab one from Amazon
[here](https://docs.aws.amazon.com/corretto/latest/corretto-11-ug/downloads-list.html).

## Using the `scie-jump` boot-pack to build a scie

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

The basic structure is JSON object with a top-level "scie" field that the lift manifest lives under.
The document can have any other keys inserted at the top level if your application or any of its
components need extra metadata. More on how to access that metadata below.

### Required fields
In the "lift" manifest, there are 3 required fields:

1. The "name" will be the name of the binary produced when the scie is packaged by the `scie-jump`
   boot-pack.
2. The "files" are a list of the files your scie boot commands need to function. Each file is a JSON
   object that just requires a "name" field whose value is the relative path of the file. Here we've
   downloaded our files as siblings to the manifest `lift.json` file.
3. Finally, the "boot.commands" are the commands that your scie executable will be able to run and
   these are stored in an object keyed by the command name. You make a command the default one
   executed when the scie binary is run by giving it an empty name (""). Here we have just one
   command and have done that so that it runs by default. More on adding more commands and selecting
   them at runtime below. Each command object requires an "exe" that is the path of the executable
   to run. Since your scie will be packaged as a single file executable, it will need to unpack the
   files you have added to it on first boot. By default, it will do this in the `~/.nce` directory
   of the user running the scie, but your command should insulate itself from the details of exactly
   where things are unpacked by using placeholders. Placeholders come in a few varieties, but the
   most common is a file placeholder. This is just the name of a file in the "files" section
   surrounded by brackets, e.g.: `{amazon-corretto-11.0.17.8.1-linux-x64.tar.gz}`. That will be
   replaced with the full path of the unpacked tarball on the local system when the command runs. If
   the name is a bit unwieldy as in this case, you can add a "key" field to the file object and
   reference that key value instead. This is what we do in the example above, shortening the JDK
   placeholder to just `{jdk}`.

### Optional fields
Files and commands can have additional configuration metadata described.

For files, you can supply a "size" and sha256 "hash". Without these the boot-pack will calculate
them, but you may want to set them in advance as a security precaution. The `scie-jump` will refuse
to operate on any file whose size or hash do not match those specified. You can also manually
specify a file "type". By default, the boot-pack detects the file type base on extension. If the
file is a directory, it gets zipped up and later re-extracted at boot time. If it's a zip, tar or
any  of the various forms of compressed tarballs (`tar.gz`, `tar.zst`, etc.), the archive will be
extracted and unpacked at boot time. Any other file is treated as a blob and is only extracted at
boot time, no unpacking is performed. In the example above we accept the defaults; so the JDK
tarball is extracted and unpacked at runtime and the jar, although unpackable sine jars a zips, is
treated as a blob and extracted as a single file at runtime.

For commands, you can specify additional command line "args" to always pass to the "exe" as well as
environment variables to set in the ambient environment in the "env" object. An environment variable
name that begins with "=" will have the "=" stripped and will overwrite the ambient environment
variable if one is already set with the same name. Without the leading "=" the environment variable
will be set only if not present in the ambient environment.

### Executing the boot pack

With a `scie-jump` in hand, your application files downloaded and the lift manifest written,
building a scie from all this is as simple as:
```
$ ./scie-jump
/home/jsirois/dev/a-scie/jump/docs/base/lift.json: /home/jsirois/dev/a-scie/jump/docs/base/coursier
```

Here the `scie-jump` is a sibling of the downloaded files and the lift manifest shown above named
`lift.json`, which is the default lift manifest name. If the lift manifest name is different, or its
in a different directory, just specify its path. The files it lists will still be searched for
relative to it regardless of where you execute the `scie-jump` from.

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
jar:
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

And you can inspect the lift manifest with basic tools:
```
$ tail -1 coursier
{"scie":{"lift":{"name":"coursier","base":"~/.nce","files":[{"name":"amazon-corretto-11.0.17.8.1-linux-x64.tar.gz","key":"jdk","size":194998805,"hash":"9628b1c1ec298a6e0f277afe383b342580086cfd7eee2be567b8d00529ca9449","type":"tar.gz"},{"name":"coursier.jar","size":42284054,"hash":"a1799d6418fbcbad47ac9e388affc751b4fc2d8678f89c332df9592d2dd3a202","type":"blob"}],"boot":{"commands":{"":{"exe":"{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin/java","args":["-jar","{coursier.jar}"],"env":{"=JAVA_HOME":"{jdk}/amazon-corretto-11.0.17.8.1-linux-x64","=PATH":"{jdk}/amazon-corretto-11.0.17.8.1-linux-x64/bin:{scie.env.PATH}"}}}}},"jump":{"size":1557952,"version":"0.1.10"}}}
```

You can also inspect the lift manifest with the built in `inspect` tool by setting the `SCIE`
environment variable, e.g.: `SCIE=inspect ./coursier`
```json
{
  "scie": {
    "lift": {
      "name": "coursier",
      "base": "~/.nce",
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
commands. You can add a "lift.description" to provide overall help to go along with your binary in
this help page as well as a "description" for each command to provide help displayed after the
command name.

This style of multi-command scie with no default command is called a [BusyBox](
https://busybox.net/), and it functions like one. Instead of using `SCIE_BOOT` to address a command,
you can also pass the command name as the 1st argument; e.g: `./cousier some_other_command`.
Finally, you can re-name the binary (or make a hard link to it) and if the name of the binary
matches a contained BusyBox command name, that command will be run.

## Using `cat` to build a scie

You can use the `cat` utility to build the scie we built above as well. The big difference is that
the lift manifest needs to be fully specified like the one shown above via `SCIE-inspect coursier`.
Having written a fully specified lift manifest like that by hand though, scie assembly is just:
```
cat \
   amazon-corretto-11.0.17.8.1-linux-x64.tar.gz \
   coursier.jar \
   lift.json > coursier
chmod +x coursier
```
That scie will have the lift manifest in pretty-printed form as its tail. That's not very friendly
for command line inspection unless you know how many lines it takes up, so you can
`tail -<N> coursier | jq .`. To package the scie for easier inspection, you could  modify the `cat`
command above like so:
```
cat \
   amazon-corretto-11.0.17.8.1-linux-x64.tar.gz \
   coursier.jar \
   <(echo) \
   <(jq -c . lift.json) > coursier
chmod +x coursier
```
That extra bit of typing adds the lift manifest as a single line JSON document on its own line and
gains the ability to blindly issue `tail -1 coursier | jq .` to inspect the lift manifest of the
scie. Either way though, since the scie is powered by a `scie-jump` in its tip, you can also issue
`SCIE-inspect coursier` as before as well.

## Advanced placeholders

+ `{scie.env.<env var name>}`: This expands to the value if the env var named. If not set it expands
  to the empty string ("").
+ `{scie.lift}`: This expands to the path to the lift manifest of the current scie extracted to
  disk. This can be used to read custom metadata stored in the lift manifest.

[^1]: The binaries that Coursier releases are single-file true native binaries that do not require a
JVM at all. As such they are ~1/3 the size of the scie we build here, which contains a full JDK
along with the Coursier executable jar. Those binaries are also much faster, ~100x for
`./coursier version`. Although the scie-jump only takes ~200 microseconds to launch the JVM that
runs Coursier, the JVM startup and warmup overheads are high. You pay that cost in painfully obvious
ways in a command line app that runs quicly and exits! This is all just to point out you should
analyze and measure your use case for applicability when considering making a scie of it.
