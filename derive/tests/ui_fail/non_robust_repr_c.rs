use co3::FfiType;

#[derive(Clone, Copy, PartialEq, Eq, FfiType)]
#[repr(C)]
pub struct NonRobustReprCStruct<T> {
    a: bool,
    b: T,
}

fn main() {}
