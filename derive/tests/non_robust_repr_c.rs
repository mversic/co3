use co3::{Encode, ExternC};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Debug, Clone, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct NonRobustReprCStruct<T> {
    a: bool,
    b: Box<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct MaybeNonRobustReprCStruct<T> {
    a: u8,
    b: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(transparent)]
pub struct MaybeNonRobustTransparentStruct<T>(T);

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(transparent)]
pub enum MaybeNonRobustTransparentEnum<T> {
    A(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C, u8)]
pub enum MaybeNonRobustReprCEnum<T> {
    A(T),
}

fn main() {
    assert_not_impl_any!(NonRobustReprCStruct<MaybeNonRobustTransparentEnum<()>>: co3::ReprC, co3::ExternC);

    assert_not_impl_any!(NonRobustReprCStruct<&str>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(NonRobustReprCStruct<()>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(NonRobustReprCStruct<u8>: co3::ReprC, co3::ExternC);

    assert_impl_all!(MaybeNonRobustTransparentStruct<&str>: co3::ExternC);
    assert_not_impl_any!(MaybeNonRobustTransparentStruct<()>: co3::ReprC, co3::ExternC);
    assert_impl_all!(MaybeNonRobustTransparentStruct<u8>: co3::ReprC, co3::ExternC);

    assert_not_impl_any!(MaybeNonRobustReprCStruct<&str>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(MaybeNonRobustReprCStruct<()>: co3::ReprC, co3::ExternC);
    assert_impl_all!(MaybeNonRobustReprCStruct<u8>: co3::transmute::CheckedTransmute, co3::ExternC);

    assert_impl_all!(MaybeNonRobustTransparentEnum<&str>: co3::ExternC);
    assert_not_impl_any!(MaybeNonRobustTransparentEnum<()>: co3::ReprC, co3::ExternC);
    assert_impl_all!(MaybeNonRobustTransparentEnum<u8>: co3::ReprC, co3::ExternC);

    let transparent_struct = MaybeNonRobustTransparentStruct(100u8);
    let repr_c_struct = MaybeNonRobustReprCStruct { a: 0, b: 42 };
    let transparent_enum = MaybeNonRobustTransparentEnum::A(255u8);
    let repr_c_enum = MaybeNonRobustReprCEnum::A(123u8);

    assert_eq!(transparent_struct.encode(&mut ()), 100u8);
    //assert_eq!(repr_c_struct.encode(&mut ()), repr_c_struct);
    assert_eq!(transparent_enum.encode(&mut ()), 255u8);
    //assert_eq!(repr_c_enum.encode(&mut ()), repr_c_enum);
}
