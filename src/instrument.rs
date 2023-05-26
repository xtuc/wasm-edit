use crate::wasi;
use core_wasm_ast as ast;
use core_wasm_ast::traverse::{traverse, Visitor, VisitorContext, WasmModule};
use std::sync::Arc;
use std::sync::Mutex;

pub struct MemoryInstrument {
    pub instrument_func: u32,
}

impl Visitor for MemoryInstrument {
    fn visit_instr<'a>(&self, ctx: &mut VisitorContext<'a, ast::Value<ast::Instr>>) {
        if matches!(ctx.node.value, ast::Instr::memory_grow(_)) {
            let instrument_func = self.instrument_func as u32;
            assert!(instrument_func > 0);

            if ctx.curr_funcidx.unwrap_or_default() != instrument_func {
                // Replace memory_grow instruction with a call to our runtime
                // for all callsites except our runtime function.
                let instrument_func = Arc::new(Mutex::new(ast::Value::new(instrument_func)));
                ctx.replace_node(ast::Instr::call(instrument_func));
            }
        }
    }
}

pub fn transform(module_ast: Arc<ast::Module>) {
    let module = WasmModule::new(Arc::clone(&module_ast));

    let (text_offset, _text_end) = module.add_data(36, &wasi::str(36, "called memory.grow\n"));

    let instrument_func = {
        let t = ast::make_type! {(I32) -> I32};
        let typeidx = module.add_type(&t);

        let fd_write = module.find_import("fd_write");
        let fd_write = Arc::new(Mutex::new(ast::Value::new(fd_write)));

        let body = ast::body![[
            ast::Value::new(ast::Instr::i32_const(1)), // fd
            ast::Value::new(ast::Instr::i32_const(text_offset as i64)), // *iovs
            ast::Value::new(ast::Instr::i32_const(1)), // iovs_len
            ast::Value::new(ast::Instr::i32_const(0)), // nwritten ptr
            ast::Value::new(ast::Instr::call(fd_write)),
            ast::Value::new(ast::Instr::drop),
            ast::Value::new(ast::Instr::local_get(0)),
            ast::Value::new(ast::Instr::memory_grow(0)),
            ast::Value::new(ast::Instr::end),
        ]];
        let func = ast::Code {
            locals: vec![],
            size: ast::Value::new(0), // printer calculates based on the body
            body: Arc::new(Mutex::new(body)),
        };

        module.add_function(&func, typeidx)
    };

    let visitor = MemoryInstrument { instrument_func };
    traverse(Arc::clone(&module_ast), Arc::new(visitor));
}
