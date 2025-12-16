use co3::ExternC;

#[derive(ExternC)]
#[mineral(opaque)]
pub enum FfiStruct1 {}

#[derive(ExternC)]
pub enum FfiStruct2 {}

#[derive(ExternC)]
#[repr(transparent)]
pub enum FfiStruct3 {}

#[derive(ExternC)]
#[repr(C)]
pub enum FfiStruct4 {}

fn main() {}
