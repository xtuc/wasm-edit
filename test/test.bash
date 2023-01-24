set -xe

wat2wasm --debug-names ./test/initial.wast -o /tmp/initial.wasm
./target/debug/wasm-edit coredump < /tmp/initial.wasm > /tmp/expected.wasm
wasm2wat /tmp/expected.wasm > ./test/expected.wast
