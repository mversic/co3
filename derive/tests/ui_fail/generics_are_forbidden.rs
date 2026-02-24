use co3::{ExternC, export};

#[derive(ExternC)]
#[mineral(opaque)]
pub struct GenericHandle<'a, T, const N: usize>(&'a [T; N]);

#[export(extern "C")]
impl GenericHandle<'static, u32, 12> {
    pub fn handle0<'a>(self) {}
}

#[export(extern "C")]
impl<'a> GenericHandle<'a, u32, 12> {
    pub fn handle1(self) {}
}

#[export(extern "C")]
impl<T> GenericHandle<'static, T, 12> {
    pub fn handle2(self) {}
}

#[export(extern "C")]
impl<const N: usize> GenericHandle<'static, u32, N> {
    pub fn handle3(self) {}
}

#[export(extern "C")]
pub extern "C" fn freestanding1<'a>(v: &'a u32) -> &'a u32 {
    v
}

#[export(extern "C")]
pub extern "C" fn freestanding2<T>(v: T) -> T {
    v
}

#[export(extern "C")]
pub extern "C" fn freestanding3<const N: usize>(v: [u32; N]) -> [u32; N] {
    v
}

fn main() {}
