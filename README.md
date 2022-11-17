# wasm-edit

> Edit and instrument already compiled Wasm binaries

## Install

```
cargo install wasm-edit
```

## Edit the main memory

Change the initial memory amount (in pages):
```
wasm-edit edit-memory --initial-memory=1000 < input.wasm > output.wasm
```

## Trace calls to `memory.grow`

Trace calls to the `memory.grow` instruction:
```
wasm-edit instrument-memory < input.wasm > output.wasm
```

Requires Wasi, but doesn't require any change on the host.
Tested with Rust and theoretically working with C/C++ (clang).

## Coredump generation

Add the coredump generation:

```
wasm-edit coredump < input.wasm > output.wasm
```

When WebAssembly encounters a `unreachable` instruction it will unwind
the stack, collect informations and generate a coredump.

The coredump struct is stored at a fixed location, this might conflict with other transformations like asyncify.

Collect the entire WebAssembly memory and use [wasmgdb] to analyze.

## Running into stack overflow

Some Wasm binaries have very recursive flow of control, increase the maximum stack size:
```
$ ulimit -s 160000
```

[wasmgdb]: https://github.com/xtuc/wasmgdb
