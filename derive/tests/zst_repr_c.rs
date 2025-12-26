use std::ffi::c_int;

use co3::{ExternC, transmute::CheckedTransmute};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Clone, Copy, ExternC)]
#[repr(transparent)]
pub struct ZstTransparentStruct<T: core::fmt::Debug + Clone>(T);

#[derive(Clone, Copy, ExternC)]
#[repr(C)]
pub struct ZstReprCStruct<T: ?Sized>(T);

#[derive(Debug, Clone, ExternC)]
#[repr(transparent)]
pub enum ZstTransparentEnum {
    A,
}

#[derive(Debug, Clone, Copy, ExternC)]
#[repr(C)]
pub enum ZstReprCEnum {
    A,
}

fn main() {
    assert_not_impl_any!(ZstTransparentStruct<ZstTransparentEnum>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstTransparentStruct<()>: co3::ReprC, co3::ExternC);

    assert_not_impl_any!(ZstReprCStruct<ZstTransparentEnum>: co3::ReprC, co3::ExternC);

    assert_not_impl_any!(ZstReprCStruct<[u8]>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstReprCStruct<()>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstTransparentEnum: co3::ReprC, co3::ExternC);

    assert_impl_all!(ZstTransparentStruct<ZstReprCEnum>: co3::ExternC, CheckedTransmute<Target = ZstReprCEnum>);
    assert_impl_all!(ZstReprCEnum: co3::ExternC, CheckedTransmute<Target = c_int>);
    //assert_impl_all!(ZstReprCStruct<ZstReprCEnum>: co3::ReprC, co3::ExternC);
}
