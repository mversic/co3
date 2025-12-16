use co3::{Encode, ExternC};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct NonRobustReprCStruct<T: ?Sized> {
    a: bool,
    b: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct MaybeNonRobustReprCStruct<T: ?Sized> {
    a: u8,
    b: T,
}

//#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
//#[repr(C)]
//pub enum MaybeNonRobustReprCEnum<T: ?Sized> {
//    A,
//}

fn main() {
    let maybe_non_robust_repr_c = MaybeNonRobustReprCStruct { a: 0, b: 42 };

    assert_not_impl_any!(NonRobustReprCStruct<&str>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(NonRobustReprCStruct<str>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(NonRobustReprCStruct<()>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(NonRobustReprCStruct<u8>: co3::ReprC, co3::ExternC);

    assert_not_impl_any!(MaybeNonRobustReprCStruct<&str>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(MaybeNonRobustReprCStruct<str>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(MaybeNonRobustReprCStruct<()>: co3::ReprC, co3::ExternC);
    assert_impl_all!(MaybeNonRobustReprCStruct<u8>: co3::ReprC, co3::ExternC);

    assert_eq!(
        maybe_non_robust_repr_c.encode(&mut ()),
        maybe_non_robust_repr_c
    );
}
