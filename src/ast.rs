#![allow(non_camel_case_types)]
// Allow non camel case types because it's easier to copy paste from the
// Wasm reference interpreter.

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, PartialEq, Clone)]
pub struct Value<T> {
    pub value: T,
    pub start_offset: usize,
    pub end_offset: usize,
}
impl<T> Value<T> {
    pub fn new(value: T) -> Self {
        Self {
            start_offset: 0,
            end_offset: 0,
            value,
        }
    }
}

pub type MutableValue<T> = Rc<RefCell<Value<T>>>;

#[derive(Debug, Clone)]
pub struct Memory {
    pub initial_memory: Value<u32>,
}

#[derive(Debug, Clone)]
pub struct Code {
    pub size: Value<u32>,
    pub locals: Vec<CodeLocal>,
    pub body: MutableValue<Vec<Value<Instr>>>,
}

#[derive(Debug, Clone)]
pub struct CodeLocal {
    pub count: u32,
    pub value_type: ValueType,
}

#[derive(Debug, Clone)]
pub enum ValueType {
    NumType(NumType),
    // VectorType(),
    // RefType(),
}

#[derive(Debug, Clone)]
pub enum NumType {
    I32,
    I64,
    F32,
    F64,
}

#[derive(Debug, Clone)]
pub enum Reftype {
    Func,
    Extern,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub reftype: Reftype,
    pub limits: Limits,
}

#[derive(Debug, Clone)]
pub struct DataSegment {
    pub offset: Value<Vec<Value<Instr>>>,
    pub bytes: Vec<u8>,
}

impl DataSegment {
    pub fn compute_offset(&self) -> i64 {
        let expr = &self.offset.value;
        for instr in expr {
            if let Instr::i32_const(v) = instr.value {
                return v;
            }
        }

        panic!("malformed data expression: {:?}", expr)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Instr {
    unreachable,
    nop,

    call(MutableValue<u32>),
    call_indirect(u32, u32),

    drop,
    select,

    local_get(u32),
    local_set(u32),
    local_tee(u32),
    global_get(u32),
    global_set(u32),
    table_get(u32),
    table_set(u32),

    i32_load(MutableValue<u32>, u32),
    i64_load(MutableValue<u32>, u32),
    f32_load(MutableValue<u32>, u32),
    f64_load(MutableValue<u32>, u32),
    i32_load8_s(MutableValue<u32>, u32),
    i32_load8_u(MutableValue<u32>, u32),
    i32_load16_s(MutableValue<u32>, u32),
    i32_load16_u(MutableValue<u32>, u32),
    i64_load8_s(MutableValue<u32>, u32),
    i64_load8_u(MutableValue<u32>, u32),
    i64_load16_s(MutableValue<u32>, u32),
    i64_load16_u(MutableValue<u32>, u32),
    i64_load32_s(MutableValue<u32>, u32),
    i64_load32_u(MutableValue<u32>, u32),

    i32_store(MutableValue<u32>, u32),
    i64_store(MutableValue<u32>, u32),
    f32_store(MutableValue<u32>, u32),
    f64_store(MutableValue<u32>, u32),
    i32_store8(MutableValue<u32>, u32),
    i32_store16(MutableValue<u32>, u32),
    i64_store8(MutableValue<u32>, u32),
    i64_store16(MutableValue<u32>, u32),
    i64_store32(MutableValue<u32>, u32),

    memory_size(u8),
    memory_grow(u8),

    br(u32),
    br_if(u32),
    br_table(Vec<u32>, u32),
    else_end,
    end,
    Return,
    Block(u8, MutableValue<Vec<Value<Instr>>>),
    Loop(u8, MutableValue<Vec<Value<Instr>>>),
    If(u8, MutableValue<Vec<Value<Instr>>>),

    i32_const(i64),
    i64_const(i64),
    f32_const(f32),
    f64_const(f64),
    i32_eqz,
    i32_eq,
    i32_ne,
    i32_lt_s,
    i32_lt_u,
    i32_gt_s,
    i32_gt_u,
    i32_le_s,
    i32_le_u,
    i32_ge_s,
    i32_ge_u,

    i64_eqz,
    i64_eq,
    i64_ne,
    i64_lt_s,
    i64_lt_u,
    i64_gt_s,
    i64_gt_u,
    i64_le_s,
    i64_le_u,
    i64_ge_s,
    i64_ge_u,

    f32_eq,
    f32_ne,
    f32_lt,
    f32_gt,
    f32_le,
    f32_ge,

    f64_eq,
    f64_ne,
    f64_lt,
    f64_gt,
    f64_le,
    f64_ge,

    i32_clz,
    i32_ctz,
    i32_popcnt,
    i32_add,
    i32_sub,
    i32_mul,
    i32_div_s,
    i32_div_u,
    i32_rem_s,
    i32_rem_u,
    i32_and,
    i32_or,
    i32_xor,
    i32_shl,
    i32_shr_s,
    i32_shr_u,
    i32_rotl,
    i32_rotr,

    i64_clz,
    i64_ctz,
    i64_popcnt,
    i64_add,
    i64_sub,
    i64_mul,
    i64_div_s,
    i64_div_u,
    i64_rem_s,
    i64_rem_u,
    i64_and,
    i64_or,
    i64_xor,
    i64_shl,
    i64_shr_s,
    i64_shr_u,
    i64_rotl,
    i64_rotr,

    f32_abs,
    f32_neg,
    f32_ceil,
    f32_floor,
    f32_trunc,
    f32_nearest,
    f32_sqrt,
    f32_add,
    f32_sub,
    f32_mul,
    f32_div,
    f32_min,
    f32_max,
    f32_copysign,

    f64_abs,
    f64_neg,
    f64_ceil,
    f64_floor,
    f64_trunc,
    f64_nearest,
    f64_sqrt,
    f64_add,
    f64_sub,
    f64_mul,
    f64_div,
    f64_min,
    f64_max,
    f64_copysign,

    i32_wrap_i64,
    i32_trunc_f32_s,
    i32_trunc_f32_u,
    i32_trunc_f64_s,
    i32_trunc_f64_u,
    i64_extend_i32_s,
    i64_extend_i32_u,
    i64_trunc_f32_s,
    i64_trunc_f32_u,
    i64_trunc_f64_s,
    i64_trunc_f64_u,
    f32_convert_i32_s,
    f32_convert_i32_u,
    f32_convert_i64_s,
    f32_convert_i64_u,
    f32_demote_f64,
    f64_convert_i32_s,
    f64_convert_i32_u,
    f64_convert_i64_s,
    f64_convert_i64_u,
    f64_promote_f32,

    i32_reinterpret_f32,
    i64_reinterpret_f64,
    f32_reinterpret_i32,
    f64_reinterpret_i64,

    i32_extend8_s,
    i32_extend16_s,
    i64_extend8_s,
    i64_extend16_s,
    i64_extend32_s,
}

#[derive(Debug)]
pub enum Section {
    /// (Size, Section)
    Memory((Value<u32>, Vec<Memory>)),
    Data((Value<u32>, Rc<RefCell<Vec<DataSegment>>>)),
    Code((Value<u32>, MutableValue<Vec<Code>>)),
    Type((Value<u32>, Rc<RefCell<Vec<Type>>>)),
    Func((Value<u32>, Rc<RefCell<Vec<u32>>>)),
    Import((Value<u32>, Rc<RefCell<Vec<Import>>>)),
    Table((Value<u32>, Rc<RefCell<Vec<Table>>>)),
    /// (Id, Size, Section)
    Unknown((u8, u32, Vec<u8>)),
}

#[derive(Debug)]
pub struct Module {
    pub sections: Rc<RefCell<Vec<Section>>>,
}

#[derive(Debug, Clone)]
pub struct Type {
    pub params: Vec<ValueType>,
    pub results: Vec<ValueType>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub typeidx: u32,
}
