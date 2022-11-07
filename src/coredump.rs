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

use crate::ast;
use crate::traverse::{locals_flatten, traverse, Visitor, VisitorContext, WasmModule};
use log::debug;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

fn create_set_frame(module: &WasmModule, local_count: usize) -> u32 {
    let mut t = ast::Type {
        params: vec![
            ast::ValueType::NumType(ast::NumType::I32), // code offset,
        ],
        results: vec![],
    };
    for _ in 0..local_count {
        t.params.push(ast::ValueType::NumType(ast::NumType::I32));
    }
    let typeidx = module.add_type(&t);

    let zero = Arc::new(Mutex::new(ast::Value::new(0)));
    let base_addr = t.params.len() as u32;

    let mut set_locals = vec![];
    for i in 0..local_count {
        set_locals.push(ast::Value::new(ast::Instr::local_get(base_addr)));
        // Skip the first local as it's the code offset argument
        set_locals.push(ast::Value::new(ast::Instr::local_get((i + 1) as u32)));
        set_locals.push(ast::Value::new(ast::Instr::i32_store(
            ast::make_value!(0),
            8 + (i * 4) as u32,
        )));
    }

    // The first frame base addr is after number of frame and next frame itself.
    let initial_base_addr = ast::body![[
        ast::Value::new(ast::Instr::i32_const(4 * 2)),
        ast::Value::new(ast::Instr::local_set(base_addr)),
    ]];

    let body = ast::body![
        // Load next frame base addr
        [
            ast::Value::new(ast::Instr::i32_const(4)),
            ast::Value::new(ast::Instr::i32_load(zero.clone(), 0)),
            ast::Value::new(ast::Instr::local_tee(base_addr)),
            ast::Value::new(ast::Instr::i32_eqz),
            ast::Value::new(ast::Instr::If(
                ast::BlockType::Empty,
                Arc::new(Mutex::new(initial_base_addr))
            )),
        ],
        // Create frame struct
        [
            // set code offset param
            ast::Value::new(ast::Instr::local_get(base_addr)),
            ast::Value::new(ast::Instr::local_get(0)),
            ast::Value::new(ast::Instr::i32_store(zero.clone(), 0)),
            // set count local
            ast::Value::new(ast::Instr::local_get(base_addr)),
            ast::Value::new(ast::Instr::i32_const(local_count as i64)),
            ast::Value::new(ast::Instr::i32_store(ast::make_value!(0), 4)),
        ],
        set_locals,
        // Update frame counter
        [
            ast::Value::new(ast::Instr::i32_const(0)),
            ast::Value::new(ast::Instr::i32_const(0)),
            ast::Value::new(ast::Instr::i32_load(zero.clone(), 0)), // load number of frames
            ast::Value::new(ast::Instr::i32_const(1)),
            ast::Value::new(ast::Instr::i32_add),
            ast::Value::new(ast::Instr::i32_store(zero.clone(), 0)), // update number of frames
        ],
        // Update next frame addr
        [
            ast::Value::new(ast::Instr::i32_const(4)),
            ast::Value::new(ast::Instr::i32_const(8 + (local_count * 4) as i64)),
            ast::Value::new(ast::Instr::local_get(base_addr)),
            ast::Value::new(ast::Instr::i32_add),
            ast::Value::new(ast::Instr::i32_store(zero, 0)),
        ]
    ];
    let func = ast::Code {
        locals: vec![
            // Holds the base addr for the frame struct
            ast::CodeLocal {
                count: 1,
                value_type: ast::ValueType::NumType(ast::NumType::I32),
            },
        ],
        size: ast::Value::new(0), // printer calculates based on the body
        body: Arc::new(Mutex::new(body)),
    };
    module.add_function(&func, typeidx)
}

struct CoredumpTransform {
    is_unwinding: u32,
    unreachable_shim: u32,
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

            // TODO: call set_frame

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
                    // let fd_write = ctx.module.find_import("fd_write");
                    // body.extend_from_slice(&wasi::print(fd_write, 64));
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
        }
    }
}

pub fn transform(module_ast: Arc<ast::Module>) {
    let module = WasmModule::new(Arc::clone(&module_ast));

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

    // Add debugging text
    // let (text1, _text1_end) = module.add_data(36, &wasi::str(36, "unreachable_shim\n"));
    // let (text2, _text2_end) = module.add_data(64, &wasi::str(64, "edge\n"));

    // Add `unreachable_shim`
    let unreachable_shim = {
        let t = ast::make_type! {};
        let typeidx = module.add_type(&t);

        // let fd_write = module.find_import("fd_write");
        let body = ast::body![
            // wasi::print(fd_write, text1),
            [
                ast::Value::new(ast::Instr::i32_const(1)),
                ast::Value::new(ast::Instr::global_set(is_unwinding))
            ]
        ];
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
        let funcidx = create_set_frame(&module, i);
        debug!("set_frame{} func at {}", i, funcidx);
        set_frame_funcs.insert(i as u32, funcidx);
    }

    let visitor = CoredumpTransform {
        is_unwinding,
        unreachable_shim,
        set_frame_funcs,
    };
    traverse(Arc::clone(&module_ast), Arc::new(visitor));
}
