use co3::{ExternC, carbonate};

#[derive(Clone, ExternC)]
pub struct NoReprStruct(String);

#[derive(Clone, ExternC)]
pub enum NoReprEnum {
    A(String),
}

#[carbonate(extern "C")]
pub extern "C" fn return_no_repr_struct() -> Vec<NoReprStruct> {
    unimplemented!()
}

#[carbonate(extern "C")]
pub extern "C" fn return_no_repr_enum() -> Vec<NoReprEnum> {
    unimplemented!()
}

fn main() {}
