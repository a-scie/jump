# package

A psuedo-crate that just serves to implement a post-processing build step against the scie-jump
binary.

## Usage

```
cargo run [--release] -p package <output directory>
```

## Role

The packaging step adds a 64 bit magic footer to the scie-jump binary that allows a scie-jump to
determine if it is bare (as opposed to being a scie tip). This allows it to switch modes of
operation automatically for ergonomic use of the `boot-pack` in bare mode.

See the [jump README.md](../jump/README.md) for more information on scie structure.
