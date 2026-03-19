use co3::{ReprC, export};

#[derive(Clone, ReprC)]
pub struct NoReprStruct(String);

#[derive(Clone, ReprC)]
pub enum NoReprEnum {
    A(String),
}

#[export("C")]
pub fn return_no_repr_struct() -> Vec<NoReprStruct> {
    unimplemented!()
}

#[export("C")]
pub extern "C" fn return_no_repr_enum() -> Vec<NoReprEnum> {
    unimplemented!()
}

fn main() {}
