use co3::FfiType;

#[derive(Clone, Copy, PartialEq, Eq, FfiType)]
#[repr(C)]
pub struct FfiStruct1(*mut u32);

#[derive(Clone, Copy, PartialEq, Eq, FfiType)]
pub enum FfiEnum1 {
    A,
    B(*mut u32),
}

fn main() {}
