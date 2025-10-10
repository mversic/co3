use co3::ExternC;

#[derive(ExternC)]
#[mineral(opaque)]
pub enum FfiStruct1 {}

#[derive(ExternC)]
pub enum FfiStruct2 {}

fn main() {}
