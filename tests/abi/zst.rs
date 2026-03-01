use co3::{ExternC, ReprC, transmute::CheckedTransmute};
use static_assertions::assert_not_impl_any;

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
pub struct NoReprStruct<T: ?Sized> {
    b: Box<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
pub enum NoReprEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[repr(C)]
pub struct ReprCStruct<T: ?Sized> {
    b: Box<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[repr(u8)]
#[allow(dead_code)]
pub enum ReprCEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[repr(C, u8)]
#[allow(dead_code)]
pub enum ReprCDataEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[repr(transparent)]
pub struct TransparentStruct<T: ?Sized>(Box<T>);

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[repr(transparent)]
#[allow(dead_code)]
pub enum TransparentEnum<T: ?Sized> {
    A(Box<T>),
}

#[test]
fn zst_no_impl() {
    assert_not_impl_any!(NoReprStruct<()>:
        CheckedTransmute,
        ExternC,
        ReprC,
    );

    assert_not_impl_any!(NoReprEnum<()>:
        CheckedTransmute,
        ExternC,
        ReprC,
    );

    assert_not_impl_any!(ReprCStruct<()>:
        CheckedTransmute,
        ExternC,
        ReprC,
    );

    assert_not_impl_any!(ReprCEnum<()>:
        CheckedTransmute,
        ExternC,
        ReprC,
    );

    assert_not_impl_any!(ReprCDataEnum<()>:
        CheckedTransmute,
        ExternC,
        ReprC,
    );

    assert_not_impl_any!(TransparentStruct<()>:
        ExternC,
        ReprC,
    );

    assert_not_impl_any!(TransparentEnum<()>:
        ExternC,
        ReprC,
    );
}
