//! Guest coredump generation.
//!
//! Informations about the stack is recorded at offset 0 in memory with the
//! following structure:
//!
//! | number of frames (u32) | next frame offset (u32) | frame* |
//!
//! Where a `frame` is:
//!
//! | code offset (u32) | count local (u32) | local* (u32) |

use crate::traverse::{locals_flatten, traverse, Visitor, VisitorContext, WasmModule};
use crate::{ast, BoxError};
use log::debug;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use wasm_edit::{parser, printer, traverse, update_value};

struct CoredumpTransform {
    is_unwinding: u32,
    unreachable_shim: u32,
    write_coredump: u32,
    /// Mapping between target func local count and set_frame function
    set_frame_funcs: HashMap<u32, u32>,
}

impl CoredumpTransform {}

impl Visitor for CoredumpTransform {
    fn visit_instr<'a>(&self, ctx: &mut VisitorContext<'a, ast::Value<ast::Instr>>) {
        let curr_funcidx = ctx.curr_funcidx.unwrap_or_default();
        let curr_func_type = ctx.module.get_func_type(curr_funcidx);

        // Don't transform our own runtime functions
        for (_, funcidx) in &self.set_frame_funcs {
            if curr_funcidx == *funcidx {
                return;
            }
        }
        if curr_funcidx == self.unreachable_shim {
            return;
        }

        // Replace the `unreachable` instruction with our runtime, for all
        // instructions except in our runtime.
        if matches!(ctx.node.value, ast::Instr::unreachable) {
            // call unreachable_shim
            {
                let unreachable_shim = Arc::new(Mutex::new(ast::Value::new(self.unreachable_shim)));
                ctx.insert_node_before(ast::Instr::call(unreachable_shim));
            }

            // call set_frame
            {
                // In Wasm DWARF the offset is relative to the start of the
                // code section.
                // https://yurydelendik.github.io/webassembly-dwarf/#pc
                // let code_offset = ctx.node.start_offset as i64
                //     - ctx.module.get_code_section_start_offset().unwrap() as i64;
                // body.push(ast::Value::new(ast::Instr::i32_const(code_offset as i64)));
                // FIXME: we use the funcidx because the code offset isn't accurate
                // or buggy.
                ctx.insert_node_before(ast::Instr::i32_const(curr_funcidx as i64));

                let func_locals = ctx.module.func_locals(curr_funcidx);

                // TODO: for now we don't care about function arguments
                // because seems that Rust doesn't really use them anyway.
                for i in 0..curr_func_type.params.len() {
                    ctx.insert_node_before(ast::Instr::i32_const(669 + i as i64));
                }

                let locals = locals_flatten(func_locals);

                // Collect the base/stack pointer, usually Rust stores it in
                // the first few locals (so after the function params).
                let mut local_count = curr_func_type.params.len() as u32;

                for local in locals {
                    ctx.insert_node_before(ast::Instr::local_get(local_count));
                    if local.value_type == ast::ValueType::NumType(ast::NumType::I64) {
                        ctx.insert_node_before(ast::Instr::i32_wrap_i64);
                    }
                    if local.value_type == ast::ValueType::NumType(ast::NumType::F64) {
                        ctx.insert_node_before(ast::Instr::i32_trunc_f64_u);
                    }
                    if local.value_type == ast::ValueType::NumType(ast::NumType::F32) {
                        ctx.insert_node_before(ast::Instr::i32_trunc_f32_u);
                    }
                    local_count += 1;

                    // Only collect up to 10 locals after the function args
                    // because Rust usually stores the base addr there.
                    if local_count >= curr_func_type.params.len() as u32 + 15 {
                        break;
                    }
                }

                let set_frame = *self
                    .set_frame_funcs
                    .get(&local_count)
                    .expect(&format!("no set_frame for local count: {}", local_count));

                let set_frame = Arc::new(Mutex::new(ast::Value::new(set_frame)));
                ctx.insert_node_before(ast::Instr::call(set_frame));
            }

            // Return from the current function
            // Add values on the stack to satisfy the current function result
            // type. Values don't need to be meaningful.
            {
                for result in &curr_func_type.results {
                    match result {
                        ast::ValueType::NumType(ast::NumType::I32) => {
                            ctx.insert_node_before(ast::Instr::i32_const(666));
                        }
                        ast::ValueType::NumType(ast::NumType::I64) => {
                            ctx.insert_node_before(ast::Instr::i64_const(666));
                        }
                        ast::ValueType::NumType(ast::NumType::F32) => {
                            ctx.insert_node_before(ast::Instr::f32_const(666.0));
                        }
                        ast::ValueType::NumType(ast::NumType::F64) => {
                            ctx.insert_node_before(ast::Instr::f64_const(666.0));
                        }
                    }
                }
            }

            ctx.replace_node(ast::Instr::Return);

            // We don't need to continue in the func, it's unreachable.
            ctx.stop_traversal();
            return;
        }

        // After each call, check if we are unwinding the stack and need to continue
        // to do so. Unless we are in a function that is exported, ie the edge
        // of the module, in that case throw.
        if matches!(
            ctx.node.value,
            ast::Instr::call(_) | ast::Instr::call_indirect(_, _)
        ) {
            ctx.insert_node_after(ast::Instr::global_get(self.is_unwinding));

            // Insert if is_unwinding branch
            {
                let mut body = vec![];

                // call set_frame
                {
                    // In Wasm DWARF the offset is relative to the start of the
                    // code section.
                    // https://yurydelendik.github.io/webassembly-dwarf/#pc
                    // let code_offset = ctx.node.start_offset as i64
                    //     - ctx.module.get_code_section_start_offset().unwrap() as i64;
                    // body.push(ast::Value::new(ast::Instr::i32_const(code_offset as i64)));
                    // FIXME: we use the funcidx because the code offset isn't accurate
                    // or buggy.
                    body.push(ast::Value::new(ast::Instr::i32_const(curr_funcidx as i64)));

                    let func_locals = ctx.module.func_locals(curr_funcidx);

                    // TODO: for now we don't care about function arguments
                    // because seems that Rust doesn't really use them anyway.
                    for i in 0..curr_func_type.params.len() {
                        body.push(ast::Value::new(ast::Instr::i32_const(669 + i as i64)));
                    }

                    let locals = locals_flatten(func_locals);

                    // Collect the base/stack pointer, usually Rust stores it in
                    // the first few locals (so after the function params).
                    let mut local_count = curr_func_type.params.len() as u32;

                    for local in locals {
                        body.push(ast::Value::new(ast::Instr::local_get(local_count)));
                        if local.value_type == ast::ValueType::NumType(ast::NumType::I64) {
                            body.push(ast::Value::new(ast::Instr::i32_wrap_i64));
                        }
                        if local.value_type == ast::ValueType::NumType(ast::NumType::F64) {
                            body.push(ast::Value::new(ast::Instr::i32_trunc_f64_u));
                        }
                        if local.value_type == ast::ValueType::NumType(ast::NumType::F32) {
                            body.push(ast::Value::new(ast::Instr::i32_trunc_f32_u));
                        }
                        local_count += 1;

                        // Only collect up to 10 locals after the function args
                        // because Rust usually stores the base addr there.
                        if local_count >= curr_func_type.params.len() as u32 + 15 {
                            break;
                        }
                    }

                    let set_frame = *self
                        .set_frame_funcs
                        .get(&local_count)
                        .expect(&format!("no set_frame for local count: {}", local_count));

                    let set_frame = Arc::new(Mutex::new(ast::Value::new(set_frame)));
                    body.push(ast::Value::new(ast::Instr::call(set_frame)));
                }

                if ctx.module.is_func_exported(curr_funcidx) {
                    // We are at the edge of the module, stop unwinding the
                    // stack and trap.
                    let write_coredump = Arc::new(Mutex::new(ast::Value::new(self.write_coredump)));
                    body.push(ast::Value::new(ast::Instr::call(write_coredump)));
                    body.push(ast::Value::new(ast::Instr::unreachable));
                } else {
                    // Add values on the stack to satisfy the current function result
                    // type. Values don't need to be meaningful.
                    {
                        for result in &curr_func_type.results {
                            let instr = match result {
                                ast::ValueType::NumType(ast::NumType::I32) => {
                                    ast::Instr::i32_const(667)
                                }
                                ast::ValueType::NumType(ast::NumType::I64) => {
                                    ast::Instr::i64_const(667)
                                }
                                ast::ValueType::NumType(ast::NumType::F32) => {
                                    ast::Instr::f32_const(667.0)
                                }
                                ast::ValueType::NumType(ast::NumType::F64) => {
                                    ast::Instr::f64_const(667.0)
                                }
                            };
                            body.push(ast::Value::new(instr));
                        }
                    }

                    body.push(ast::Value::new(ast::Instr::Return));
                }
                body.push(ast::Value::new(ast::Instr::end));

                let body = ast::Value::new(body);
                let if_node = ast::Instr::If(ast::BlockType::Empty, Arc::new(Mutex::new(body)));
                ctx.insert_node_after(if_node);
            }
            return;
        }
    }
}

fn get_runtime() -> Result<WasmModule, BoxError> {
    let contents = include_bytes!("../runtime/build/runtime.wasm");
    let module_ast = parser::decode(contents)
        .map_err(|err| format!("failed to parse runtime Wasm module: {}", err))?;
    let module = WasmModule::new(Arc::new(module_ast));

    return Ok(module);
}

pub fn transform(module_ast: Arc<ast::Module>) -> Result<(), BoxError> {
    let module = WasmModule::new(Arc::clone(&module_ast));
    let runtime = get_runtime()?;

    debug!(
        "code section starts at {}",
        module.get_code_section_start_offset().unwrap()
    );

    // Add `is_unwinding` global
    let is_unwinding = {
        let expr = ast::Value::new(vec![
            ast::Value::new(ast::Instr::i32_const(0)),
            ast::Value::new(ast::Instr::end),
        ]);
        let global = ast::Global {
            global_type: ast::GlobalType {
                valtype: ast::ValueType::NumType(ast::NumType::I32),
                mutable: true,
            },
            expr,
        };
        module.add_global(&global).unwrap()
    };
    debug!("is_unwinding global at {}", is_unwinding);

    // Add `unreachable_shim`
    let unreachable_shim = {
        let t = ast::make_type! {};
        let typeidx = module.add_type(&t);

        let body = ast::body![[
            ast::Value::new(ast::Instr::i32_const(1)),
            ast::Value::new(ast::Instr::global_set(is_unwinding))
        ]];
        let func = ast::Code {
            locals: vec![],
            size: ast::Value::new(0), // printer calculates based on the body
            body: Arc::new(Mutex::new(body)),
        };
        module.add_function(&func, typeidx)
    };
    debug!("unreachable_shim func at {}", unreachable_shim);

    // Add `set_frame{}` functions
    let mut set_frame_funcs = HashMap::new();
    for i in 0..30 {
        let func = runtime
            .get_export_func(&format!("set_frame{}", i))
            .expect("failed to get set_frame1");

        // add type
        let typeidx = {
            let mut t = ast::Type {
                params: vec![
                    ast::ValueType::NumType(ast::NumType::I32), // code offset,
                ],
                results: vec![],
            };
            for _ in 0..i {
                t.params.push(ast::ValueType::NumType(ast::NumType::I32));
            }
            module.add_type(&t)
        };

        let funcidx = module.add_function(&func, typeidx);
        debug!("set_frame{} func at {}", i, funcidx);
        set_frame_funcs.insert(i as u32, funcidx);
    }

    let write_coredump = {
        let typeidx = module.add_type(&ast::make_type!());
        let func = runtime
            .get_export_func("write_coredump")
            .expect("failed to get write_coredump");
        module.add_function(&func, typeidx)
    };
    debug!("write_coredump func at {}", write_coredump);

    let visitor = CoredumpTransform {
        is_unwinding,
        unreachable_shim,
        set_frame_funcs,
        write_coredump,
    };
    traverse(Arc::clone(&module_ast), Arc::new(visitor));

    Ok(())
}
