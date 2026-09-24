use co3::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, ReprC, rust_spec::RustSpec,
    transmute::CheckedTransmute,
};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
pub struct NoReprStruct<T: ?Sized> {
    b: Box<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
pub enum NoReprEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C)]
pub struct ReprCStruct<T: ?Sized> {
    b: Box<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
pub enum ReprCEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C, u8)]
pub enum ReprCDataEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(transparent)]
pub struct TransparentStruct<T: ?Sized>(Box<T>);

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(transparent)]
pub enum TransparentEnum<T: ?Sized> {
    A(Box<T>),
}

type CNoReprStructZst = <NoReprStruct<()> as ExternC>::CType;
type CNoReprEnumZst = <NoReprEnum<()> as ExternC>::CType;
type CReprCStructZst = <ReprCStruct<()> as ExternC>::CType;
type CReprCEnumZst = <ReprCEnum<()> as ExternC>::CType;
type CReprCDataEnumZst = <ReprCDataEnum<()> as ExternC>::CType;
type CTransparentStructZst = <TransparentStruct<()> as ExternC>::CType;
type CTransparentEnumZst = <TransparentEnum<()> as ExternC>::CType;

#[test]
fn boxed_zst_traits() {
    macro_rules! assert_traits {
        ($ty:ty, $ctype:ty) => {
            assert_impl_all!($ty:
                Decode<'static>,
                Encode,
            );

            assert_impl_all!($ctype:
                ReprC,
                CFnArg,
                CFnReturn,
            );

            assert_not_impl_any!($ty: ReprC, CFnArg, CFnReturn);
        };
    }

    assert_traits!(NoReprStruct<()>, CNoReprStructZst);
    assert_traits!(NoReprEnum<()>, CNoReprEnumZst);
    assert_traits!(ReprCStruct<()>, CReprCStructZst);
    assert_traits!(ReprCEnum<()>, CReprCEnumZst);
    assert_traits!(ReprCDataEnum<()>, CReprCDataEnumZst);
    assert_traits!(TransparentStruct<()>, CTransparentStructZst);
    assert_traits!(TransparentEnum<()>, CTransparentEnumZst);

    assert_impl_all!(ReprCStruct<()>: CheckedTransmute);
    assert_impl_all!(ReprCEnum<()>: CheckedTransmute);
    assert_impl_all!(ReprCDataEnum<()>: CheckedTransmute);
    assert_impl_all!(TransparentStruct<()>: CheckedTransmute);
    assert_impl_all!(TransparentEnum<()>: CheckedTransmute);

    assert_not_impl_any!(NoReprStruct<()>: CheckedTransmute);
    assert_not_impl_any!(NoReprEnum<()>: CheckedTransmute);
}
