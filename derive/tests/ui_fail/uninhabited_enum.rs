use co3::{ReprC, export};

#[export("C")]
pub enum FfiStruct1 {}

#[derive(ReprC)]
pub enum FfiStruct2 {}

fn main() {}
