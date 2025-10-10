use co3::ExternC;

#[derive(Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct FfiStruct1(*mut u32);

#[derive(Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub enum FfiEnum1 {
    A,
    B(*mut u32),
}

fn main() {}
