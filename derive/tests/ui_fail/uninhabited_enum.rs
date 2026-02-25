use co3::ReprC;

#[derive(ReprC)]
#[reprC(opaque)]
pub enum FfiStruct1 {}

#[derive(ReprC)]
pub enum FfiStruct2 {}

#[derive(ReprC)]
#[repr(transparent)]
pub enum FfiStruct3 {}

#[derive(ReprC)]
#[repr(C)]
pub enum FfiStruct4 {}

fn main() {}
