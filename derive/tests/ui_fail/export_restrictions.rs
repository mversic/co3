use co3::{ExternC, carbonate};

trait Kita {
    extern "system" fn kita0(self);
    extern "C" fn kita1(self);
    fn kita2(self);
}

#[derive(Clone, ExternC)]
struct FfiStruct(u8);

#[carbonate(extern "C")]
impl Kita for FfiStruct {
    #[carbonate]
    fn kita0(self) {}
    #[unsafe(no_mangle)]
    #[carbonate(skip)]
    fn kita1(self) {}
    #[carbonate(skip)]
    #[unsafe(export_name = "kita")]
    extern "C" fn kita2(self) {}
}

#[carbonate(extern "C")]
impl FfiStruct {
    #[unsafe(no_mangle)]
    #[carbonate(skip)]
    extern "C" fn kita1(self) {}
    #[carbonate(skip)]
    #[unsafe(export_name = "kita")]
    pub extern "C" fn kita2(self) {}
}

#[carbonate]
impl FfiStruct {}

#[carbonate(extern)]
impl FfiStruct {}

#[carbonate(extern "C")]
#[carbonate(skip)]
extern "C" fn kita1(_a: u32) {}

#[carbonate(extern)]
extern "C" fn kita3(_a: u32) {}

#[carbonate]
extern "C" fn kita4(_a: u32) {}

fn main() {}
