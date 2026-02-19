use co3::ExternC;

#[derive(Clone, ExternC)]
pub struct NoReprStruct(String);

#[derive(Clone, ExternC)]
pub enum NoReprEnum {
    A(String),
}

#[co3::carbonate]
pub fn return_no_repr_struct() -> Vec<NoReprStruct> {
    unimplemented!()
}

#[co3::carbonate]
pub fn return_no_repr_enum() -> Vec<NoReprEnum> {
    unimplemented!()
}

fn main() {}
