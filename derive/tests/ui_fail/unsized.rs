use co3::ExternC;

#[derive(Clone, Copy, ExternC)]
#[repr(transparent)]
pub struct TransparentStruct<T: ?Sized>(T);

#[derive(Clone, Copy, ExternC)]
#[repr(C)]
pub struct ReprCStruct<T: ?Sized>(T);

#[derive(Clone, Copy, ExternC)]
pub struct NoReprCStruct<T: ?Sized>(T);

fn main() {}
