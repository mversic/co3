use co3::ExternC;

#[derive(Clone, ExternC)]
pub struct FfiStruct(u8);

#[co3::carbonate]
impl FfiStruct {
    pub fn public(self) {}
    fn private(self) {}
}

unsafe extern "C" {
    fn FfiStruct__public(arg: <FfiStruct as ExternC>::CType) -> co3::FfiReturn;
    fn FfiStruct__private(arg: <FfiStruct as ExternC>::CType) -> co3::FfiReturn;
}

fn main() {
    let source = CFfiStruct(42);

    unsafe {
        FfiStruct__public(source);
        FfiStruct__private(source);
    }
}
