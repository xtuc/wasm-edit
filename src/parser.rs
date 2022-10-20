use crate::ast;
use nom::bytes::complete::take;

type BoxError = Box<dyn std::error::Error>;

#[derive(Debug)]
pub(crate) struct ParserError(String);

impl<I> nom::error::ParseError<I> for ParserError {
    fn from_error_kind(_input: I, kind: nom::error::ErrorKind) -> Self {
        ParserError(format!("error {:?}", kind))
    }
    fn append(_input: I, _kind: nom::error::ErrorKind, _other: Self) -> Self {
        todo!()
    }
    fn from_char(_input: I, _: char) -> Self {
        todo!()
    }
    fn or(self, _other: Self) -> Self {
        todo!()
    }
}

pub(crate) type IResult<I, O, E = ParserError> = Result<(I, O), nom::Err<E>>;

#[derive(Debug)]
pub(crate) struct InputContext<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> InputContext<'a> {
    fn read_u32(self) -> IResult<InputContext<'a>, u32> {
        let (input, bytes) = self.read_bytes(4usize)?;
        let value = u32::from_le_bytes(bytes.try_into().unwrap());
        Ok((input, value))
    }

    fn read_u8(self) -> IResult<InputContext<'a>, u8> {
        let (input, bytes) = self.read_bytes(1usize)?;
        let value = u8::from_le_bytes(bytes.try_into().unwrap());
        Ok((input, value))
    }

    fn read_leb128(self) -> IResult<InputContext<'a>, u32> {
        let mut input = self.input;
        let v = leb128::read::unsigned(&mut input).unwrap();

        // FIXME: find a better way to known how many bytes we just read
        let size = {
            let mut buffer = Vec::new();
            leb128::write::unsigned(&mut buffer, v).unwrap();
            buffer.len()
        };

        Ok((
            Self {
                input,
                offset: self.offset + size,
            },
            v as u32,
        ))
    }

    fn read_f32(self) -> IResult<InputContext<'a>, f32> {
        let (ctx, bytes) = self.read_bytes(4)?;
        let v = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
        Ok((ctx, v))
    }

    fn read_f64(self) -> IResult<InputContext<'a>, f64> {
        let (ctx, bytes) = self.read_bytes(8)?;
        let v = f64::from_le_bytes(bytes[0..8].try_into().unwrap());
        Ok((ctx, v))
    }

    fn read_leb128_signed(self) -> IResult<InputContext<'a>, i64> {
        let mut input = self.input;
        let v = leb128::read::signed(&mut input).unwrap();

        // FIXME: find a better way to known how many bytes we just read
        let size = {
            let mut buffer = Vec::new();
            leb128::write::signed(&mut buffer, v).unwrap();
            buffer.len()
        };

        Ok((
            Self {
                input,
                offset: self.offset + size,
            },
            v,
        ))
    }

    fn read_bytes(self, n: usize) -> IResult<InputContext<'a>, &'a [u8]> {
        let (input, bytes) = take(n)(self.input)?;

        Ok((
            Self {
                input,
                offset: self.offset + n,
            },
            bytes,
        ))
    }
}

pub fn decode<'a>(input: &'a [u8]) -> Result<ast::Module, BoxError> {
    let input = InputContext { input, offset: 0 };
    match decode_module(input) {
        Ok((_, module)) => Ok(module),
        Err(err) => Err(format!("failed to decode: {}", err).into()),
    }
}

fn decode_module<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::Module> {
    let (ctx, magic) = ctx.read_bytes(4)?;
    if magic != b"\0asm" {
        panic!("unsupported header: {:?}", magic)
    }
    assert_eq!(ctx.offset, 4);
    let (ctx, version) = ctx.read_u32()?;
    if version != 1 {
        panic!("unsupported version: {:?}", version)
    }
    assert_eq!(ctx.offset, 8);

    let mut ctx = ctx;

    let mut module = ast::Module {
        sections: Vec::new(),
    };

    loop {
        let res = decode_section(ctx)?;
        ctx = res.0;
        module.sections.push(res.1);

        if ctx.input.is_empty() {
            break;
        }
    }
    Ok((ctx, module))
}

fn decode_section_memory<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, Vec<ast::Memory>> {
    decode_vec(ctx, decode_memory)
}

fn decode_section_code<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, Vec<ast::Code>> {
    decode_vec(ctx, decode_code)
}

fn decode_code<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::Code> {
    let start_offset = ctx.offset;
    let (ctx, size) = ctx.read_leb128()?;
    let end_offset = ctx.offset;

    let (ctx, code_bytes) = ctx.read_bytes(size as usize)?;

    let code = {
        let ctx = InputContext {
            input: code_bytes,
            offset: ctx.offset,
        };

        let size = ast::Value {
            start_offset,
            value: size,
            end_offset,
        };
        let (ctx, locals) = decode_vec(ctx, decode_code_local)?;
        let (_ctx, body) = decode_expr(ctx, ast::Instr::end)?;
        ast::Code { size, locals, body }
    };

    Ok((ctx, code))
}

fn decode_instr<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::Value<ast::Instr>> {
    let start_offset = ctx.offset;
    let (ctx, id) = ctx.read_u8()?;

    macro_rules! decode_instr {
        ($byte:expr, $instr:ident) => {
            if id == $byte {
                let end_offset = ctx.offset;
                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr,
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };
        ($byte:expr, $instr:ident(u8)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_u8()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(f32)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_f32()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(f64)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_f64()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(i32)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_leb128_signed()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(i64)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_leb128_signed()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(u32)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_leb128()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(u32, u32)) => {
            if id == $byte {
                let (ctx, arg0) = ctx.read_leb128()?;
                let (ctx, arg1) = ctx.read_leb128()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0, arg1),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };

        ($byte:expr, $instr:ident(Vec<u32>, u32)) => {
            if id == $byte {
                let (ctx, arg0) = decode_vec(ctx, |ctx| ctx.read_leb128())?;
                let (ctx, arg1) = ctx.read_leb128()?;
                let end_offset = ctx.offset;

                let value = ast::Value {
                    start_offset,
                    value: ast::Instr::$instr(arg0, arg1),
                    end_offset,
                };
                return Ok((ctx, value));
            }
        };
    }

    decode_instr!(0x00, unreachable);
    decode_instr!(0x01, nop);

    if id == 0x02 {
        let (ctx, ret_type) = ctx.read_u8()?;
        let (ctx, body) = decode_expr(ctx, ast::Instr::end)?;
        let end_offset = ctx.offset;

        let value = ast::Value {
            start_offset,
            value: ast::Instr::Block(ret_type, body),
            end_offset,
        };
        return Ok((ctx, value));
    }
    if id == 0x03 {
        let (ctx, ret_type) = ctx.read_u8()?;
        let (ctx, body) = decode_expr(ctx, ast::Instr::end)?;
        let end_offset = ctx.offset;

        let value = ast::Value {
            start_offset,
            value: ast::Instr::Loop(ret_type, body),
            end_offset,
        };
        return Ok((ctx, value));
    }
    if id == 0x04 {
        let (ctx, ret_type) = ctx.read_u8()?;
        // let (ctx, consequent) = decode_expr(ctx, ast::Instr::else_end)?;
        // FIXME: support IfElse, If will contain both
        let (ctx, body) = decode_expr(ctx, ast::Instr::end)?;
        let end_offset = ctx.offset;

        let value = ast::Value {
            start_offset,
            value: ast::Instr::If(ret_type, body),
            end_offset,
        };
        return Ok((ctx, value));
    }

    decode_instr!(0xc, br(u32));
    decode_instr!(0xd, br_if(u32));
    decode_instr!(0x0b, end);
    decode_instr!(0x05, else_end);
    decode_instr!(0x0e, br_table(Vec<u32>, u32));
    decode_instr!(0x0f, Return);
    decode_instr!(0x10, call(u32));
    decode_instr!(0x11, call_indirect(u32, u32));

    decode_instr!(0x1a, drop);
    decode_instr!(0x1b, select);
    // decode_instr!(0x1c, select);

    decode_instr!(0x20, local_get(u32));
    decode_instr!(0x21, local_set(u32));
    decode_instr!(0x22, local_tee(u32));
    decode_instr!(0x23, global_get(u32));
    decode_instr!(0x24, global_set(u32));
    decode_instr!(0x25, table_get(u32));
    decode_instr!(0x26, table_set(u32));

    decode_instr!(0x28, i32_load(u32, u32));
    decode_instr!(0x29, i64_load(u32, u32));
    decode_instr!(0x2a, f32_load(u32, u32));
    decode_instr!(0x2b, f64_load(u32, u32));
    decode_instr!(0x2c, i32_load8_s(u32, u32));
    decode_instr!(0x2d, i32_load8_u(u32, u32));
    decode_instr!(0x2e, i32_load16_s(u32, u32));
    decode_instr!(0x2f, i32_load16_u(u32, u32));
    decode_instr!(0x30, i64_load8_s(u32, u32));
    decode_instr!(0x31, i64_load8_u(u32, u32));
    decode_instr!(0x32, i64_load16_s(u32, u32));
    decode_instr!(0x33, i64_load16_u(u32, u32));
    decode_instr!(0x34, i64_load32_s(u32, u32));
    decode_instr!(0x35, i64_load32_u(u32, u32));

    decode_instr!(0x36, i32_store(u32, u32));
    decode_instr!(0x37, i64_store(u32, u32));
    decode_instr!(0x38, f32_store(u32, u32));
    decode_instr!(0x39, f64_store(u32, u32));
    decode_instr!(0x3a, i32_store8(u32, u32));
    decode_instr!(0x3b, i32_store16(u32, u32));
    decode_instr!(0x3c, i64_store8(u32, u32));
    decode_instr!(0x3d, i64_store16(u32, u32));
    decode_instr!(0x3e, i64_store32(u32, u32));

    decode_instr!(0x3f, memory_size(u8));
    decode_instr!(0x40, memory_grow(u8));

    decode_instr!(0x41, i32_const(i32));
    decode_instr!(0x42, i64_const(i64));
    decode_instr!(0x43, f32_const(f32));
    decode_instr!(0x44, f64_const(f64));

    decode_instr!(0x45, i32_eqz);
    decode_instr!(0x46, i32_eq);
    decode_instr!(0x47, i32_ne);
    decode_instr!(0x48, i32_lt_s);
    decode_instr!(0x49, i32_lt_u);
    decode_instr!(0x4a, i32_gt_s);
    decode_instr!(0x4b, i32_gt_u);
    decode_instr!(0x4c, i32_le_s);
    decode_instr!(0x4d, i32_le_u);
    decode_instr!(0x4e, i32_ge_s);
    decode_instr!(0x4f, i32_ge_u);

    decode_instr!(0x50, i64_eqz);
    decode_instr!(0x51, i64_eq);
    decode_instr!(0x52, i64_ne);
    decode_instr!(0x53, i64_lt_s);
    decode_instr!(0x54, i64_lt_u);
    decode_instr!(0x55, i64_gt_s);
    decode_instr!(0x56, i64_gt_u);
    decode_instr!(0x57, i64_le_s);
    decode_instr!(0x58, i64_le_u);
    decode_instr!(0x59, i64_ge_s);
    decode_instr!(0x5a, i64_ge_u);

    decode_instr!(0x5b, f32_eq);
    decode_instr!(0x5c, f32_ne);
    decode_instr!(0x5d, f32_lt);
    decode_instr!(0x5e, f32_gt);
    decode_instr!(0x5f, f32_le);
    decode_instr!(0x60, f32_ge);

    decode_instr!(0x61, f64_eq);
    decode_instr!(0x62, f64_ne);
    decode_instr!(0x63, f64_lt);
    decode_instr!(0x64, f64_gt);
    decode_instr!(0x65, f64_le);
    decode_instr!(0x66, f64_ge);

    decode_instr!(0x67, i32_clz);
    decode_instr!(0x68, i32_ctz);
    decode_instr!(0x69, i32_popcnt);
    decode_instr!(0x6a, i32_add);
    decode_instr!(0x6b, i32_sub);
    decode_instr!(0x6c, i32_mul);
    decode_instr!(0x6d, i32_div_s);
    decode_instr!(0x6e, i32_div_u);
    decode_instr!(0x6f, i32_rem_s);
    decode_instr!(0x70, i32_rem_u);
    decode_instr!(0x71, i32_and);
    decode_instr!(0x72, i32_or);
    decode_instr!(0x73, i32_xor);
    decode_instr!(0x74, i32_shl);
    decode_instr!(0x75, i32_shr_s);
    decode_instr!(0x76, i32_shr_u);
    decode_instr!(0x77, i32_rotl);
    decode_instr!(0x78, i32_rotr);

    decode_instr!(0x79, i64_clz);
    decode_instr!(0x7a, i64_ctz);
    decode_instr!(0x7b, i64_popcnt);
    decode_instr!(0x7c, i64_add);
    decode_instr!(0x7d, i64_sub);
    decode_instr!(0x7e, i64_mul);
    decode_instr!(0x7f, i64_div_s);
    decode_instr!(0x80, i64_div_u);
    decode_instr!(0x81, i64_rem_s);
    decode_instr!(0x82, i64_rem_u);
    decode_instr!(0x83, i64_and);
    decode_instr!(0x84, i64_or);
    decode_instr!(0x85, i64_xor);
    decode_instr!(0x86, i64_shl);
    decode_instr!(0x87, i64_shr_s);
    decode_instr!(0x88, i64_shr_u);
    decode_instr!(0x89, i64_rotl);
    decode_instr!(0x8a, i64_rotr);

    decode_instr!(0x8b, f32_abs);
    decode_instr!(0x8c, f32_neg);
    decode_instr!(0x8d, f32_ceil);
    decode_instr!(0x8e, f32_floor);
    decode_instr!(0x8f, f32_trunc);
    decode_instr!(0x90, f32_nearest);
    decode_instr!(0x91, f32_sqrt);
    decode_instr!(0x92, f32_add);
    decode_instr!(0x93, f32_sub);
    decode_instr!(0x94, f32_mul);
    decode_instr!(0x95, f32_div);
    decode_instr!(0x96, f32_min);
    decode_instr!(0x97, f32_max);
    decode_instr!(0x98, f32_copysign);

    decode_instr!(0x99, f64_abs);
    decode_instr!(0x9a, f64_neg);
    decode_instr!(0x9b, f64_ceil);
    decode_instr!(0x9c, f64_floor);
    decode_instr!(0x9d, f64_trunc);
    decode_instr!(0x9e, f64_nearest);
    decode_instr!(0x9f, f64_sqrt);
    decode_instr!(0xa0, f64_add);
    decode_instr!(0xa1, f64_sub);
    decode_instr!(0xa2, f64_mul);
    decode_instr!(0xa3, f64_div);
    decode_instr!(0xa4, f64_min);
    decode_instr!(0xa5, f64_max);
    decode_instr!(0xa6, f64_copysign);

    decode_instr!(0xa7, i32_wrap_i64);
    decode_instr!(0xa8, i32_trunc_f32_s);
    decode_instr!(0xa9, i32_trunc_f32_u);
    decode_instr!(0xaa, i32_trunc_f64_s);
    decode_instr!(0xab, i32_trunc_f64_u);
    decode_instr!(0xac, i64_extend_i32_s);
    decode_instr!(0xad, i64_extend_i32_u);
    decode_instr!(0xae, i64_trunc_f32_s);
    decode_instr!(0xaf, i64_trunc_f32_u);
    decode_instr!(0xb0, i64_trunc_f64_s);
    decode_instr!(0xb1, i64_trunc_f64_u);
    decode_instr!(0xb2, f32_convert_i32_s);
    decode_instr!(0xb3, f32_convert_i32_u);
    decode_instr!(0xb4, f32_convert_i64_s);
    decode_instr!(0xb5, f32_convert_i64_u);
    decode_instr!(0xb6, f32_demote_f64);
    decode_instr!(0xb7, f64_convert_i32_s);
    decode_instr!(0xb8, f64_convert_i32_u);
    decode_instr!(0xb9, f64_convert_i64_s);
    decode_instr!(0xba, f64_convert_i64_u);
    decode_instr!(0xbb, f64_promote_f32);

    decode_instr!(0xbc, i32_reinterpret_f32);
    decode_instr!(0xbd, i64_reinterpret_f64);
    decode_instr!(0xbe, f32_reinterpret_i32);
    decode_instr!(0xbf, f64_reinterpret_i64);

    decode_instr!(0xc0, i32_extend8_s);
    decode_instr!(0xc1, i32_extend16_s);
    decode_instr!(0xc2, i64_extend8_s);
    decode_instr!(0xc3, i64_extend16_s);
    decode_instr!(0xc4, i64_extend32_s);

    unimplemented!("unknown instruction: {:#x}", id);
}

fn decode_expr<'a>(
    ctx: InputContext<'a>,
    end_instr: ast::Instr,
) -> IResult<InputContext<'a>, Vec<ast::Value<ast::Instr>>> {
    let mut ctx = ctx;
    let mut vec = Vec::new();
    loop {
        let ret = decode_instr(ctx)?;
        ctx = ret.0;
        if ret.1.value == end_instr {
            break;
        }
        vec.push(ret.1);
    }

    Ok((ctx, vec))
}

fn decode_code_local<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::CodeLocal> {
    let (ctx, count) = ctx.read_leb128()?;
    let (ctx, value_type) = decode_valtype(ctx)?;
    let code_local = ast::CodeLocal { count, value_type };
    Ok((ctx, code_local))
}

fn decode_valtype<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::ValueType> {
    let (ctx, id) = ctx.read_u8()?;
    Ok((
        ctx,
        match id {
            0x7F => ast::ValueType::NumType(ast::NumType::I32),
            0x7E => ast::ValueType::NumType(ast::NumType::I64),
            0x7D => ast::ValueType::NumType(ast::NumType::F32),
            0x7C => ast::ValueType::NumType(ast::NumType::F64),
            e => unimplemented!("unsupported type: {:x}", e),
        },
    ))
}

fn decode_memory<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::Memory> {
    let (ctx, t) = ctx.read_u8()?;

    let start_offset = ctx.offset;
    let (ctx, min) = ctx.read_leb128()?;
    let end_offset = ctx.offset;
    let initial_memory = ast::Value {
        start_offset,
        value: min,
        end_offset,
    };

    if t != 0 {
        todo!();
    }

    let mem = ast::Memory { initial_memory };
    Ok((ctx, mem))
}

fn decode_section<'a>(ctx: InputContext<'a>) -> IResult<InputContext<'a>, ast::Section> {
    let (ctx, id) = ctx.read_u8()?;

    let start_offset = ctx.offset;
    let (ctx, size) = ctx.read_leb128()?;
    let end_offset = ctx.offset;

    let section_size = ast::Value {
        start_offset,
        value: size,
        end_offset,
    };

    let offset = ctx.offset;
    let (ctx, section_bytes) = ctx.read_bytes(size as usize)?;

    let section_bytes = InputContext {
        input: section_bytes,
        offset,
    };

    let section = match id {
        5 => {
            let (_, res) = decode_section_memory(section_bytes)?;
            ast::Section::Memory((section_size, res))
        }
        10 => {
            let (_, res) = decode_section_code(section_bytes)?;
            ast::Section::Code((section_size, res))
        }
        _ => ast::Section::Unknown,
    };
    Ok((ctx, section))
}

type DecodeItem<'a, T> = fn(InputContext<'a>) -> IResult<InputContext<'a>, T>;

pub(crate) fn decode_vec<'a, T>(
    ctx: InputContext<'a>,
    decode_item_fn: DecodeItem<'a, T>,
) -> IResult<InputContext<'a>, Vec<T>> {
    let (ctx, n) = ctx.read_leb128()?;

    let mut items: Vec<T> = Vec::new();
    let mut ctx = ctx;
    for _ in 0..n {
        let res = decode_item_fn(ctx)?;
        ctx = res.0;
        items.push(res.1);
    }

    Ok((ctx, items))
}
