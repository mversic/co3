use std::mem::MaybeUninit;

use co3::{carbonate, FfiType};

#[derive(Clone, FfiType)]
pub struct FfiStruct;

#[carbonate]
pub fn return_nested() -> Vec<Vec<FfiStruct>> {
    vec![vec![FfiStruct, FfiStruct], vec![FfiStruct, FfiStruct]]
}

fn main() {
    let mut nested = MaybeUninit::uninit();
    unsafe { __return_nested(nested.as_mut_ptr()) };
}
