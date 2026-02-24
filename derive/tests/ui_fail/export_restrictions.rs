use co3::{ExternC, export};

trait Kita {
    extern "system" fn kita0(self);
    extern "C" fn kita1(self);
    fn kita2(self);
}

#[derive(Clone, ExternC)]
struct FfiStruct(u8);

#[export(extern "C")]
impl Kita for FfiStruct {
    #[export]
    fn kita0(self) {}
    #[unsafe(no_mangle)]
    #[export(skip)]
    fn kita1(self) {}
    #[export(skip)]
    #[unsafe(export_name = "kita")]
    extern "C" fn kita2(self) {}
}

#[export(extern "C")]
impl FfiStruct {
    #[unsafe(no_mangle)]
    #[export(skip)]
    extern "C" fn kita1(self) {}
    #[export(skip)]
    #[unsafe(export_name = "kita")]
    pub extern "C" fn kita2(self) {}
}

#[export]
impl FfiStruct {}

#[export(extern)]
impl FfiStruct {}

#[export(extern "C")]
#[export(skip)]
extern "C" fn kita1(_a: u32) {}

#[export(extern)]
extern "C" fn kita3(_a: u32) {}

#[export]
extern "C" fn kita4(_a: u32) {}

fn main() {}
