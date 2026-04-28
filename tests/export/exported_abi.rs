use std::{mem::MaybeUninit, ptr::NonNull};

use co3::{
    DecodeWithStore, EncodeWithStore, FfiReturn, ReprC, export, export_, export_C, external::Extern,
    out_ptr::OutPtrRead as _,
};

trait AmbiguousX<T, const N: usize> {
    type U;

    fn ambiguous(a: &[Self::U; N]) -> Ambiguous;
}

trait AmbiguousY {
    extern "C" fn ambiguous() -> Ambiguous;
}

trait CustomExports {
    fn xor(&self, by: u8) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ReprC)]
#[repr(u8)]
pub enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Inherent,
    Fn,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructU8(u8);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructU32(u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructU64(u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructBool(bool);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpaqueStructI32(i32);

#[derive(Debug, Clone, Copy, PartialEq, ReprC)]
#[repr(transparent)]
pub enum NonOpaqueStruct<T> {
    A(T),
}

export_C! {
    type OpaqueStructU8;
    type OpaqueStructU32;
    type OpaqueStructU64;
    type OpaqueStructBool;
    type OpaqueStructI32;

    #[unsafe(no_mangle)]
    unsafe fn ambiguous1() -> Ambiguous;

    impl OpaqueStructU32 {
        fn re_exported() -> Self;
    }

    impl CustomExports for OpaqueStructU8 {
        #[unsafe(export_name = "xor_u8")]
        fn xor(&self, by: u8) -> Self;
    }

    impl Clone for OpaqueStructBool {
        #[unsafe(export_name = "cclone")]
        fn clone(&self) -> Self;
    }

    impl Clone for crate::exported_abi::OpaqueStructI32 {
        #[unsafe(export_name = "lclone")]
        fn clone(&self) -> Self;
    }

    impl Clone for self::NonOpaqueStruct<bool> {
        #[unsafe(export_name = "nclone")]
        fn clone(&self) -> Self;
    }

    impl Clone for NonOpaqueStruct<i32> {
        fn clone(&self) -> Self;
    }
}

export_! {
    #![abi = "Rust"]

    impl Clone for OpaqueStructU8 {
        #[unsafe(no_mangle)]
        fn clone(&self) -> Self;
    }

    impl Clone for NonOpaqueStruct<u8> {
        fn clone(&self) -> Self;
    }

    impl Clone for NonOpaqueStruct<i8> {
        fn clone(&self) -> Self;
    }
}

#[export("C")]
impl AmbiguousX<u64, 3> for OpaqueStructU64 {
    type U = u8;

    fn ambiguous(_a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[export("Rust")]
impl AmbiguousX<u32, 4> for OpaqueStructU32 {
    type U = i8;

    #[unsafe(export_name = "kita")]
    fn ambiguous(_a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[export("C")]
impl AmbiguousY for OpaqueStructU64 {
    #[unsafe(no_mangle)]
    extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

impl CustomExports for OpaqueStructU8 {
    fn xor(&self, by: u8) -> Self {
        OpaqueStructU8(self.0 ^ by)
    }
}

impl OpaqueStructU32 {
    fn re_exported() -> Self {
        OpaqueStructU32(42)
    }
}

#[export("Rust")]
impl OpaqueStructU64 {
    #[unsafe(export_name = "kita1")]
    pub const unsafe extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

#[export("C")]
impl OpaqueStructU32 {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

pub const unsafe fn ambiguous1() -> Ambiguous {
    Ambiguous::Fn
}

#[export("Rust")]
#[unsafe(export_name = "kita2")]
pub const unsafe extern "Rust" fn ambiguous2() -> Ambiguous {
    Ambiguous::Fn
}

#[test]
fn exported_abi() {
    let mut output = MaybeUninit::new(Ambiguous::None as _);

    unsafe extern "C" {
        fn export__OpaqueStructU32__ambiguous(output: *mut u8) -> FfiReturn;

        fn ambiguous(output: *mut u8) -> FfiReturn;
        fn ambiguous1(output: *mut u8) -> FfiReturn;

        #[link_name = "cclone"]
        fn export_opaque_clone_bool(
            handle_ptr: *const Extern,
            out_ptr: *mut NonNull<Extern>,
        ) -> FfiReturn;
        #[link_name = "nclone"]
        fn export_non_opaque_clone_bool(handle_ptr: *const u8, out_ptr: *mut u8) -> FfiReturn;
        #[link_name = "xor_u8"]
        fn export_opaque_xor_u8(
            handle_ptr: *const Extern,
            by: u8,
            out_ptr: *mut NonNull<Extern>,
        ) -> FfiReturn;

        fn export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous(
            a: &[u8; 3],
            output: *mut u8,
        ) -> FfiReturn;

        #[link_name = "export__OpaqueStructU32__re_exported"]
        fn re_exported(out_ptr: *mut NonNull<Extern>) -> FfiReturn;
    }

    unsafe extern "Rust" {
        fn kita(a: *const [i8; 4], out_ptr: *mut u8) -> FfiReturn;
        fn kita1(out_ptr: *mut u8) -> FfiReturn;
        fn kita2(out_ptr: *mut u8) -> FfiReturn;

        #[link_name = "export__Clone__NonOpaqueStruct_u8__clone"]
        fn export_non_opaque_clone_u8(handle: *const u8, out_ptr: *mut u8) -> FfiReturn;
        #[link_name = "clone"]
        fn export_opaque_clone_u8(
            handle: *const NonNull<Extern>,
            out_ptr: *mut NonNull<Extern>,
        ) -> FfiReturn;
    }

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            export__OpaqueStructU32__ambiguous(output.as_mut_ptr())
        );
        let inherent = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        assert_eq!(
            FfiReturn::Ok,
            export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous(&[12; 3], output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(
            FfiReturn::Ok,
            kita((&[13_i8; 4]).encode(&mut ()), output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(FfiReturn::Ok, kita1(output.as_mut_ptr()));
        let inherent = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        assert_eq!(FfiReturn::Ok, kita2(output.as_mut_ptr()));
        let custom_fn = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        assert_eq!(FfiReturn::Ok, ambiguous1(output.as_mut_ptr()));
        let custom_fn = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        assert_eq!(FfiReturn::Ok, ambiguous(output.as_mut_ptr()));
        let custom_fn = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, custom_fn);
    }

    unsafe {
        let mut output = MaybeUninit::new(NonNull::dangling());

        assert_eq!(FfiReturn::Ok, re_exported(output.as_mut_ptr()));
        let custom_fn = Box::from_raw(output.assume_init().as_ptr().cast::<OpaqueStructU32>());
        assert_eq!(OpaqueStructU32(42_u32), *custom_fn);
    }

    unsafe {
        let opaque_bool = OpaqueStructBool(true);
        let opaque_u8 = OpaqueStructU8(11_u8);

        let opaque_bool_ptr = &opaque_bool as *const OpaqueStructBool;
        let opaque_u8_ptr = &opaque_u8 as *const OpaqueStructU8;

        let mut opaque_bool_clone_out = MaybeUninit::new(NonNull::dangling());

        assert_eq!(
            FfiReturn::Ok,
            export_opaque_clone_bool(opaque_bool_ptr.cast(), opaque_bool_clone_out.as_mut_ptr())
        );
        let mut opaque_u8_clone_out = MaybeUninit::new(NonNull::dangling());
        assert_eq!(
            FfiReturn::Ok,
            export_opaque_clone_u8(opaque_u8_ptr.cast(), opaque_u8_clone_out.as_mut_ptr(),)
        );
        let opaque_u8_clone = Box::from_raw(opaque_u8_clone_out.assume_init().as_ptr().cast());
        assert_eq!(OpaqueStructU8(11_u8), *opaque_u8_clone);
        let mut opaque_xor_out = MaybeUninit::new(NonNull::dangling());
        assert_eq!(
            FfiReturn::Ok,
            export_opaque_xor_u8(opaque_u8_ptr.cast(), 7, opaque_xor_out.as_mut_ptr())
        );
        let opaque_xor = Box::from_raw(opaque_xor_out.assume_init().as_ptr().cast());
        assert_eq!(OpaqueStructU8(12_u8), *opaque_xor);
        let opaque_bool_clone = Box::from_raw(opaque_bool_clone_out.assume_init().as_ptr().cast());

        assert_eq!(OpaqueStructBool(true), *opaque_bool_clone);
    }

    unsafe {
        let non_opaque_bool = NonOpaqueStruct::A(true);
        let non_opaque_u8 = NonOpaqueStruct::A(11_u8);

        let mut non_opaque_bool_clone_out = MaybeUninit::new(171);

        assert_eq!(
            FfiReturn::Ok,
            export_non_opaque_clone_bool(
                (&non_opaque_bool).encode(&mut ()),
                non_opaque_bool_clone_out.as_mut_ptr(),
            )
        );
        let mut non_opaque_u8_clone_out = MaybeUninit::new(171);
        assert_eq!(
            FfiReturn::Ok,
            export_non_opaque_clone_u8(
                (&non_opaque_u8).encode(&mut ()),
                non_opaque_u8_clone_out.as_mut_ptr(),
            )
        );
        let non_opaque_u8_clone =
            NonOpaqueStruct::decode(non_opaque_u8_clone_out.assume_init(), &mut ()).unwrap();
        assert_eq!(NonOpaqueStruct::A(11_u8), non_opaque_u8_clone);

        let non_opaque_bool_clone =
            NonOpaqueStruct::decode(non_opaque_bool_clone_out.assume_init(), &mut ()).unwrap();

        assert_eq!(NonOpaqueStruct::A(true), non_opaque_bool_clone);
    }
}
