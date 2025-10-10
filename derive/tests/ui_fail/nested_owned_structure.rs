use std::mem::MaybeUninit;

use co3::ExternC;

#[derive(Clone, ExternC)]
pub struct FfiStruct;

#[co3::carbonate]
pub fn return_nested() -> Vec<Vec<FfiStruct>> {
    vec![vec![FfiStruct, FfiStruct], vec![FfiStruct, FfiStruct]]
}

fn main() {
    let mut nested = MaybeUninit::uninit();
    unsafe { __return_nested(nested.as_mut_ptr()) };
}
