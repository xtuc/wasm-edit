use crate::ast;
use crate::traverse::{Visitor, VisitorContext};
use log::debug;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

pub struct MemoryInstrumentAddData {
    pub text: &'static str,
    pub text_offset: RefCell<u32>,
}

impl Visitor for MemoryInstrumentAddData {
    fn visit_data_section<'a>(&self, ctx: &mut VisitorContext<'a, Vec<ast::DataSegment>>) {
        let text = self.text;

        let mut bytes = vec![];
        // iov.iov_base - This is a pointer to the start of the 'hello world\n' string
        bytes.write_all(&((4 * 2) as u32).to_le_bytes()).unwrap();
        // iov.iov_len - The length of the 'hello world\n' string
        bytes.write_all(&(text.len() as u32).to_le_bytes()).unwrap();
        // text
        bytes.write_all(text.as_bytes()).unwrap();

        // The response for the fd_write call is after the iov struct

        let segment = ast::DataSegment {
            offset: ast::Value::new(vec![
                ast::Value::new(ast::Instr::i32_const(0 as i64)),
                ast::Value::new(ast::Instr::end),
            ]),
            bytes,
        };
        ctx.insert_node_after(segment);

        *self.text_offset.borrow_mut() = 0;
    }
}

pub struct MemoryInstrumentAddRuntime {
    pub typeidx: RefCell<usize>,
    pub funcidx: RefCell<usize>,
    pub fd_write: RefCell<u32>,
    pub text_offset: u32,
}

impl Visitor for MemoryInstrumentAddRuntime {
    fn visit_code_section<'a>(&self, ctx: &'_ mut VisitorContext<'a, Vec<ast::Code>>) {
        let fd_write = Rc::new(RefCell::new(ast::Value::new(
            self.fd_write.borrow().clone(),
        )));

        let body = ast::Value::new(vec![
            ast::Value::new(ast::Instr::i32_const(1)), // fd
            ast::Value::new(ast::Instr::i32_const(self.text_offset as i64)), // *iovs
            ast::Value::new(ast::Instr::i32_const(1)), // iovs_len
            ast::Value::new(ast::Instr::i32_const(40)), // nwritten ptr
            ast::Value::new(ast::Instr::call(fd_write)),
            ast::Value::new(ast::Instr::drop),
            ast::Value::new(ast::Instr::local_get(0)),
            ast::Value::new(ast::Instr::memory_grow(0)),
            ast::Value::new(ast::Instr::end),
        ]);
        let func = ast::Code {
            locals: vec![],
            size: ast::Value::new(0), // printer calculates based on the body
            body: Rc::new(RefCell::new(body)),
        };
        let funcidx = ctx.node.len();
        *self.funcidx.borrow_mut() += funcidx;
        ctx.insert_node_after(func);
    }

    fn visit_import_section<'a>(&self, ctx: &'_ mut VisitorContext<'a, Vec<ast::Import>>) {
        let mut found = false;
        for import in ctx.node {
            if import.name == "fd_write" {
                found = true;
                break;
            }
            *self.fd_write.borrow_mut() += 1;
        }
        assert!(found, "no WASI fd_write function was found");
        debug!("fd_write at index {}", self.fd_write.borrow());

        *self.funcidx.borrow_mut() += ctx.node.len();
    }

    fn visit_type_section<'a>(&self, ctx: &'_ mut VisitorContext<'a, Vec<ast::Type>>) {
        let t = ast::Type {
            params: vec![ast::ValueType::NumType(ast::NumType::I32)],
            results: vec![ast::ValueType::NumType(ast::NumType::I32)],
        };
        let typeidx = ctx.node.len();
        *self.typeidx.borrow_mut() = typeidx;
        ctx.insert_node_after(t);
    }

    fn visit_func_section<'a>(&self, ctx: &'_ mut VisitorContext<'a, Vec<u32>>) {
        let typeidx = *self.typeidx.borrow() as u32;
        assert!(typeidx > 0, "type has not been added");
        ctx.insert_node_after(typeidx);
    }
}

pub struct MemoryInstrument {
    pub funcidx: usize,
}

impl Visitor for MemoryInstrument {
    fn visit_instr<'a>(&self, ctx: &mut VisitorContext<'a, ast::Value<ast::Instr>>) {
        if matches!(ctx.node.value, ast::Instr::memory_grow(_)) {
            let funcidx = self.funcidx as u32;
            assert!(funcidx > 0);

            if ctx.curr_funcidx.unwrap_or_default() != funcidx {
                // Replace memory_grow instruction with a call to our runtime
                // for all callsites except our runtime function.
                let funcidx = Rc::new(RefCell::new(ast::Value::new(funcidx)));
                ctx.replace_node(ast::Instr::call(funcidx));
            }
        }
    }
}
