use co3::ExternC;

#[derive(Clone, Copy, ExternC)]
#[repr(transparent)]
pub struct ZstTransparentStruct<T: ?Sized>(T);

#[derive(Clone, Copy, ExternC)]
#[repr(transparent)]
pub enum ZstTransparentEnum<T: ?Sized> {
    A(T),
}

#[derive(Clone, Copy, ExternC)]
#[repr(C)]
pub struct ZstReprCStruct<T: ?Sized>(T);

#[derive(Clone, Copy, ExternC)]
#[repr(C)]
pub enum ZstReprCEnum<T: ?Sized> {
    A(T),
}

fn main() {}
