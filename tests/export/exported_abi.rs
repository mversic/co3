use std::mem::MaybeUninit;

use co3::{ExternC, FfiReturn, export, out_ptr::OutPtrRead as _};
use webassembly_test::webassembly_test;

trait AmbiguousX<T, const N: usize> {
    type U;

    fn ambiguous(a: &[Self::U; N]) -> Ambiguous;
}

trait AmbiguousY {
    extern "C" fn ambiguous() -> Ambiguous;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(u8)]
pub enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Inherent,
    Fn,
    None,
}

#[derive(Clone, Copy, ExternC)]
#[mineral(opaque)]
pub(crate) struct OpaqueStruct<T>(T);

#[export(extern "C")]
impl AmbiguousX<u64, 3> for OpaqueStruct<u64> {
    type U = u8;

    fn ambiguous(_a: &[Self::U; 3]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[export(extern "Rust")]
impl AmbiguousX<u32, 4> for OpaqueStruct<u32> {
    type U = i8;

    #[unsafe(export_name = "kita")]
    fn ambiguous(_a: &[Self::U; 4]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[export(extern "C")]
impl AmbiguousY for OpaqueStruct<u64> {
    #[unsafe(no_mangle)]
    extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

#[export(extern "C")]
impl AmbiguousY for OpaqueStruct<u32> {
    #[export(skip)]
    extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

#[export(extern "Rust")]
impl OpaqueStruct<u64> {
    #[unsafe(export_name = "kita1")]
    pub const unsafe extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

#[export(extern "C")]
impl OpaqueStruct<u32> {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

#[export(extern "C")]
#[unsafe(no_mangle)]
pub const unsafe fn ambiguous1() -> Ambiguous {
    Ambiguous::Fn
}

#[export(extern "Rust")]
#[unsafe(export_name = "kita2")]
pub const unsafe extern "Rust" fn ambiguous2() -> Ambiguous {
    Ambiguous::Fn
}

#[test]
#[webassembly_test]
fn exported_abi() {
    let mut output = MaybeUninit::new(Ambiguous::None as _);

    unsafe extern "C" {
        fn export_OpaqueStruct_u32_ambiguous(output: *mut u8) -> FfiReturn;

        fn ambiguous(output: *mut u8) -> FfiReturn;
        fn ambiguous1(output: *mut u8) -> FfiReturn;

        fn export_AmbiguousX_u64_3_OpaqueStruct_u64_ambiguous(
            a: &[u8; 3],
            output: *mut u8,
        ) -> FfiReturn;
    }

    unsafe extern "Rust" {
        fn kita(a: &[i8; 4]) -> Ambiguous;
        fn kita1() -> Ambiguous;
        fn kita2() -> Ambiguous;
    }

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            export_OpaqueStruct_u32_ambiguous(output.as_mut_ptr())
        );
        let inherent = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        assert_eq!(
            FfiReturn::Ok,
            export_AmbiguousX_u64_3_OpaqueStruct_u64_ambiguous(&[12; 3], output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(Ambiguous::AmbiguousX, kita(&[13; 4]));
        assert_eq!(Ambiguous::Inherent, kita1());
        assert_eq!(Ambiguous::Fn, kita2());

        assert_eq!(FfiReturn::Ok, ambiguous1(output.as_mut_ptr()));
        let custom_fn = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        assert_eq!(FfiReturn::Ok, ambiguous(output.as_mut_ptr()));
        let custom_fn = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, custom_fn);
    }
}
