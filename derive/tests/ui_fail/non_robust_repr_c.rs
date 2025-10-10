use co3::ExternC;

#[derive(Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct NonRobustReprCStruct<T> {
    a: bool,
    b: T,
}

fn main() {}
