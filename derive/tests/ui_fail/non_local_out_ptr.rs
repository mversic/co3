use co3::{ExternC, export};

#[derive(Clone, ExternC)]
pub struct NoReprStruct(String);

#[derive(Clone, ExternC)]
pub enum NoReprEnum {
    A(String),
}

#[export(extern "C")]
pub extern "C" fn return_no_repr_struct() -> Vec<NoReprStruct> {
    unimplemented!()
}

#[export(extern "C")]
pub extern "C" fn return_no_repr_enum() -> Vec<NoReprEnum> {
    unimplemented!()
}

fn main() {}
