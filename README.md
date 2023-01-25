# wasm-edit

> Edit and instrument already compiled Wasm binaries

## Coredump generation

Moved to https://github.com/xtuc/wasm-coredump.

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

## Running into stack overflow

Some Wasm binaries have very recursive flow of control, increase the maximum stack size:
```
$ ulimit -s 160000
```

[wasmgdb]: https://github.com/xtuc/wasmgdb
