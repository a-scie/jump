# scie-jump

[![GitHub](https://img.shields.io/github/license/a-scie/jump)](LICENSE)
[![Github Actions CI (x86_64 Linux / MacOS / Windows)](https://github.com/a-scie/jump/actions/workflows/ci.yml/badge.svg)](https://github.com/a-scie/jump/actions/workflows/ci.yml)
[![CircleCI (Linux aarch64)](https://circleci.com/gh/a-scie/jump.svg?style=svg)](https://circleci.com/gh/a-scie/jump)

A Self Contained Interpreted Executable Launcher.

The scie-jump is rooted in science, but loose pronunciation is encouraged. The pieces all fit
together that way. More about that nce bit below.

# What is a scie-jump?

A scie-jump is a dual-purpose native binary that can either create a scie (itself a native binary of
a sort) or launch one. Best to start with two observations:

1. Executable binary formats for all major computer operating systems today accept arbitrary 
   trailing content. [ELF](https://en.wikipedia.org/wiki/Executable_and_Linkable_Format) (Linux), 
   [Mach-O](https://en.wikipedia.org/wiki/Mach-O) (MacOS) and [PE32](
   https://en.wikipedia.org/wiki/Portable_Executable) (Windows) binaries all allow you to tack
   on extra data and the binaries still run. Try it!
2. Zip files allow arbitrary header content to be added and the zip still works. Try it!

So if you write a binary that knows how zip works, you can concatenate a zip to that binary and do
magic. This general idea is almost as old as the zip file format at least and has been put to use
in one form or another in various ways.

One prominent way that brought all this to my attention is in [setuptools implementation of Python
console scripts for Windows](https://github.com/pypa/setuptools/blob/main/launcher.c). In that case
the console script executable is made up from the following sandwich:

```
[ launcher stub binary ]
#!/path/to/python/venv/bin/python
[ zip file containing a __main__.py ]
```

Since the sandwich is a Windows PE32 executable by dint of its launcher stub header, the launcher
stub executes when the sandwich is run. It immediately searches for the shebang line in the middle
of the sandwich. It then calculates the Python interpreter to use with the information in the
shebang and then re-executes using that Python interpreter as the executable and itself inserted as
the first argument. Now it turns out that Python interpreters know nothing about PE32 binaries, but
[they do know about zips](
https://docs.python.org/3/library/zipapp.html#the-python-zip-application-archive-format) and can run
zips that contain a `__main__.py` file and import any other Python modules inside the zip as well.
This is all a bit crazy, but definitely ingenious.

A scie-jump binary in its launcher role works quite a bit like the setuptools console script
launcher stub binary. Its sandwich is constructed a bit differently though:

```
[ scie-jump ]
[ file1 ]
[ file2 ]
...
[ fileN ]
[ lift.json ]
```

Just like in the setuptools console script case, the scie-jump head of this sandwich is a native
executable; so it executes. It searches for the lift manifest at the end of the file and reads it
to determine the list of files contained within it as well as any commands configured to run that
use those files. It then selects the desired command and extracts the files it requires and then
re-executes itself using that command. In general, the command will run an interpreter binary
contained in one of the files it extracts (say a CPython distribution) against another set of
interpreted files it extracts (say `.py` files). As such, a `scie-jump` is the launcher stub for a
self-contained interpreted executable. It extracts the needed files into a base directory that is
traditionally located at `~/.nce`. This is where the self-contained interpreted executable is
transformed by the scie-jump into a non-compact executable.

## Format

The format was driven by the properties of executable binaries and zip files as discussed above with
a few design goals guiding the rest:

1. I wanted to be able to assemble a scie with just a scie-jump binary, `curl`, `vi` and `cat` or
   similar foundational / ever-present command line tools.
2. I wanted an assembled scie to be inspectable, again using standard tools.

An unstated constraint here so far is that the scie-jump needs to be able to quickly and
unambiguously find the 1st byte of the lift manifest so that it can read it.

This all leads to the only real choice made, which is that the last file in a scie is always a zip.
The zip format can accept arbitrary header content because it has its central directory at its end.
This allows for a quick search backwards from the end of the file of no more than ~65KB (a zip end
of central directory record is 22 bytes plus an optional zip comment of up to 65535 bytes) to
definitively identify the zip and calculate the position of its last byte. We know the lift manifest
starts at the next byte and runs to the end of the file.

This means assembly of the scie just involves:

1. Write a lift manifest json file.
2. `cat scie-jump file1 file2 ... fileN lift.json > my-scie-binary`

The zip trailer also gives transparency. Generally, the interpreter code will live in that zip; so
tools like `zipinfo` and `unzip` can be used against the scie directly to inspect / extract the
application code.

If the ever more ubiquitous `jq` tool is included in the list of ever-present command line tools,
then the lift manifest also becomes inspectable. You change assembly to:
```
cat scie-jump file1 file2 ... fileN <(echo) <(jq -c . lift.json) > my-scie-binary
```

That gets you the lift manifest on its own single line at the end of the scie. You can then inspect
the manifest with:
```
tail -1 my-scie-binary | jq .
```

Despite scies admitting to assembly by hand like this, tools are not always available (Windows) and
there are fiddly bits here to get an easily inspectable lift manifest not to mention file sizes and
hashes, which are required by the scie-jump to find the internal files and then verify them on
extraction. As such, the scie-jump launcher will act in a boot-pack role when its bare (not in a
scie sandwich) and accept one or more lift manifest files as input from which it will build scies
for you with `--single-lift-line` manifests for easy inspectability.

## Performance

The process described above for locating the lift manifest and the subsequent parsing, checking for
file extractions and finally dispatching the selected command is fast. Generally sub-millisecond.
Since the primary use case for a scie is packaging a self-contained executable for interpreter code,
the latency overhead introduced by adding a scie-jump launcher is likely very far in the noise for
most projects that might consider packaging their applications this way. This certainly applies for
the three current [examples](examples) which are Node.js, Java and Python scies, the fastest of
which is roughly 50ms.

## Building

To build an executable scie-jump you'll need the [Rust suite of tools](https://rustup.rs/) 
installed. With that done, simply:
```
cargo run --release -p package .
```

That will deposit a scie-jump binary in the current directory after building it and packaging it.
The binary will have an `-<os>-<arch>` suffix that you are free to remove with a rename.

## Learn More

The project is at an early stage with more documentation to be fleshed out. Right now it's probably
best to inspect the [examples](examples/README.md) first and then dive into the [jump crate](
jump/README.md) for more details.