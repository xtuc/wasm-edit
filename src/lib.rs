use core_wasm_ast as ast;
pub mod wasi;

pub fn update_value<T>(input: &mut Vec<u8>, old_value: &ast::Value<T>, new_value: T) -> isize
where
    T: ToBytes + std::fmt::Debug,
{
    // FIXME: use vec splice

    let old_value_len = old_value.end_offset - old_value.start_offset;
    let head = &input[..old_value.start_offset];
    let tail = &input[old_value.end_offset..];

    let mut new_input = Vec::new();
    new_input.extend_from_slice(&head);

    let new_bytes = new_value.to_bytes();
    new_input.extend_from_slice(&new_bytes);

    new_input.extend_from_slice(&tail);

    *input = new_input;

    let diff = new_bytes.len() as isize - old_value_len as isize;

    eprintln!(
        "replace {:?} with {:?} ({:?}); diff {:?}",
        old_value, new_value, new_bytes, diff
    );

    diff
}

pub trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

impl ToBytes for u32 {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        leb128::write::unsigned(&mut buffer, *self as u64).unwrap();
        buffer
    }
}

impl ToBytes for ast::Instr {
    fn to_bytes(&self) -> Vec<u8> {
        match self {
            ast::Instr::call(idx) => {
                let mut buffer = Vec::new();
                buffer.push(0x10);
                leb128::write::unsigned(&mut buffer, idx.lock().unwrap().value as u64).unwrap();
                buffer
            }
            n => todo!("ToBytes not yet implemented for node: {:?}", n),
        }
    }
}
