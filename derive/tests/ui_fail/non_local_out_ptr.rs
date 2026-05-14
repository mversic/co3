use co3::{ReprC, extern_C};

#[derive(Clone, ReprC)]
pub struct NoReprStruct(String);

#[derive(Clone, ReprC)]
pub enum NoReprEnum {
    A(String),
}

extern_C! {
    pub fn return_no_repr_struct() -> Vec<NoReprStruct>;
    pub extern "C" fn return_no_repr_enum() -> Vec<NoReprEnum>;
}

mod provider {
    use co3::export;

    use super::*;

    #[export("C")]
    pub fn return_no_repr_struct() -> Vec<NoReprStruct> {
        unimplemented!()
    }

    #[export("C")]
    pub extern "C" fn return_no_repr_enum() -> Vec<NoReprEnum> {
        unimplemented!()
    }
}

fn main() {}
