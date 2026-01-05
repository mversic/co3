use co3::{ExternC, transmute::CheckedTransmute};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Clone, Copy, ExternC)]
#[repr(transparent)]
pub struct ZstTransparentStruct<T: core::fmt::Debug + Clone>(T);

#[derive(Clone, Copy, ExternC)]
#[repr(C)]
pub struct ZstReprCStruct<T>(T);

#[derive(Debug, Clone, ExternC)]
#[repr(transparent)]
pub enum ZstTransparentEnum<T> {
    A(Box<T>),
}

#[derive(Debug, Clone, Copy, ExternC)]
#[repr(u8)]
pub enum ZstReprCEnum<T> {
    A(T),
}

fn main() {
    assert_impl_all!(ZstTransparentStruct<ZstReprCEnum<u8>>: co3::ExternC, CheckedTransmute<Target = ZstReprCEnum<u8>>);
    assert_impl_all!(ZstTransparentEnum<ZstReprCStruct<u8>>: co3::ExternC, CheckedTransmute<Target = ZstReprCStruct<u8>>);

    assert_not_impl_any!(ZstTransparentStruct<ZstTransparentEnum<()>>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstReprCStruct<ZstTransparentEnum<()>>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstTransparentStruct<()>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstTransparentEnum<()>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstReprCStruct<()>: co3::ReprC, co3::ExternC);

    assert_impl_all!(ZstReprCEnum<u8>: co3::ExternC, CheckedTransmute);
    assert_impl_all!(ZstReprCStruct<ZstReprCEnum<u8>>: co3::ExternC, CheckedTransmute);
}
