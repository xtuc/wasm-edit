use core_wasm_ast as ast;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

// FIXME: remove unwraps
pub(crate) fn str(offset: u32, text: &str) -> Vec<u8> {
    let mut bytes = vec![];
    // iov.iov_base - This is a pointer to the start of the 'hello world\n' string
    bytes
        .write_all(&(offset + (4 * 2) as u32).to_le_bytes())
        .unwrap();
    // iov.iov_len - The length of the 'hello world\n' string
    bytes.write_all(&(text.len() as u32).to_le_bytes()).unwrap();
    // text
    bytes.write_all(text.as_bytes()).unwrap();

    // FIXME: some padding

    bytes
}

pub(crate) fn print(fd_write: u32, ptr: u32) -> Vec<ast::Value<ast::Instr>> {
    let fd_write = Arc::new(Mutex::new(ast::Value::new(fd_write)));
    vec![
        ast::Value::new(ast::Instr::i32_const(1)),          // fd
        ast::Value::new(ast::Instr::i32_const(ptr as i64)), // *iovs
        ast::Value::new(ast::Instr::i32_const(1)),          // iovs_len
        ast::Value::new(ast::Instr::i32_const(1020)),       // nwritten ptr
        ast::Value::new(ast::Instr::call(fd_write)),
        ast::Value::new(ast::Instr::drop),
    ]
}
