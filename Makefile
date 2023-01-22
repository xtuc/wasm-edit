target/debug/wasm-edit: runtime/build/runtime.wasm
	cargo build

runtime/build/runtime.wasm:
	make -C ./runtime build/runtime.wasm
