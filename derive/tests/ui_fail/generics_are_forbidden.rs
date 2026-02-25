use co3::{ReprC, export};

#[derive(ReprC)]
#[reprC(opaque)]
pub struct GenericHandle<'a, T, const N: usize>(&'a [T; N]);

#[export("C")]
impl GenericHandle<'static, u32, 12> {
    pub fn handle0<'a>(self) {}
}

#[export("C")]
impl<'a> GenericHandle<'a, u32, 12> {
    pub fn handle1(self) {}
}

#[export("C")]
impl<T> GenericHandle<'static, T, 12> {
    pub fn handle2(self) {}
}

#[export("C")]
impl<const N: usize> GenericHandle<'static, u32, N> {
    pub fn handle3(self) {}
}

#[export("C")]
pub extern "C" fn freestanding1<'a>(v: &'a u32) -> &'a u32 {
    v
}

#[export("C")]
pub extern "C" fn freestanding2<T>(v: T) -> T {
    v
}

#[export("C")]
pub extern "C" fn freestanding3<const N: usize>(v: [u32; N]) -> [u32; N] {
    v
}

fn main() {}
