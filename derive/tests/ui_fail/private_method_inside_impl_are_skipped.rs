use co3::{carbonate, FfiConvert, FfiType};

#[derive(Clone, FfiType)]
pub struct FfiStruct;

#[carbonate]
impl FfiStruct {
    fn private(self) {}
}

fn main() {
    let s = FfiStruct;
    unsafe {
        FfiStruct__private(FfiConvert::into_ffi(s, &mut ()));
    }
}
