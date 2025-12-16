use co3::ExternC;
use static_assertions::assert_not_impl_any;

#[derive(Clone, Copy, ExternC)]
#[repr(transparent)]
pub struct ZstTransparent<T>(T);

#[derive(Clone, Copy, ExternC)]
#[repr(C)]
pub struct ZstReprC<T: ?Sized>(T);

#[derive(ExternC)]
#[repr(C)]
pub enum UninhabitedReprC {
    A
}

fn main() {
    //assert_not_impl_any!(ZstTransparent<UninhabitedReprC>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstTransparent<()>: co3::ReprC, co3::ExternC);

    assert_not_impl_any!(ZstReprC<UninhabitedReprC>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstReprC<[u8]>: co3::ReprC, co3::ExternC);
    assert_not_impl_any!(ZstReprC<()>: co3::ReprC, co3::ExternC);

    //assert_not_impl_any!(UninhabitedReprC: co3::ReprC, co3::ExternC);
}
