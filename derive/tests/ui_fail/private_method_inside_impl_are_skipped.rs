use co3::{FfiConvert, FfiType};

#[derive(Clone, FfiType)]
pub struct FfiStruct;

#[co3::carbonate]
impl FfiStruct {
    fn private(self) {}
}

fn main() {
    let s = FfiStruct;
    unsafe {
        FfiStruct__private(FfiConvert::into_ffi(s, &mut ()));
    }
}
