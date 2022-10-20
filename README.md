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

## Running into stack overflow

Some Wasm binaries have very recursive flow of control, increase the maximum stack size:
```
$ ulimit -s 160000
```
