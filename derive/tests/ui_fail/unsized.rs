use co3::ReprC;

#[derive(Clone, Copy, ReprC)]
#[repr(transparent)]
pub struct TransparentStruct<T: ?Sized>(T);

#[derive(Clone, Copy, ReprC)]
#[repr(C)]
pub struct ReprCStruct<T: ?Sized>(T);

#[derive(Clone, Copy, ReprC)]
pub struct NoReprCStruct<T: ?Sized>(T);

fn main() {}
