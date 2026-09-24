use co3::rust_spec::RustSpec;
use core::ffi::c_void;

use co3::{ReprC, ffi};

trait AmbiguousX<T, const N: usize> {
    type U;

    fn ambiguous(a: &[Self::U; N]) -> Ambiguous;
}

trait AmbiguousY {
    extern "C" fn ambiguous() -> Ambiguous;
}

trait CustomExports {
    fn xor(&self, by: u8) -> Box<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
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

#[derive(Debug, Clone, Copy, PartialEq, RustSpec, ReprC)]
#[repr(transparent)]
pub enum NonOpaqueStruct<T> {
    A(T),
}

ffi! {
    #![unsafe(export("C"))]

    type OpaqueStructU8;
    type OpaqueStructU32;
    type OpaqueStructU64;
    type OpaqueStructBool;
    type OpaqueStructI32;

    #[symbol_name = "ambiguous1"]
    unsafe fn ambiguous1() -> Ambiguous;

    impl OpaqueStructU32 {
        fn re_exported() -> move Box<Self>;
    }

    impl CustomExports for OpaqueStructU8 {
        #[symbol_name = "xor_u8"]
        fn xor(&self, by: u8) -> move Box<Self>;
    }

    impl Clone for Box<OpaqueStructBool> {
        #[symbol_name = "cclone"]
        fn clone(&self) -> move Self;
    }

    impl Clone for Box<OpaqueStructI32> {
        #[symbol_name = "lclone"]
        fn clone(&self) -> move Self;
    }

    impl Clone for self::NonOpaqueStruct<bool> {
        #[symbol_name = "nclone"]
        fn clone(&self) -> move Self;
    }

    impl Clone for NonOpaqueStruct<i32> {
        fn clone(&self) -> move Self;
    }

    impl AmbiguousX<u64, 3> for OpaqueStructU64 {
        type U = u8;
        fn ambiguous(a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous;
    }

    impl AmbiguousY for OpaqueStructU64 {
        #[symbol_name = "ambiguous"]
        extern "C" fn ambiguous() -> Ambiguous;
    }

    impl OpaqueStructU32 {
        fn ambiguous() -> Ambiguous;
    }
}

ffi! {
    #![unsafe(export("Rust"))]

    impl Clone for Box<OpaqueStructU8> {
        #[symbol_name = "clone"]
        fn clone(&self) -> move Self;
    }

    impl Clone for NonOpaqueStruct<u8> {
        fn clone(&self) -> move Self;
    }

    impl Clone for NonOpaqueStruct<i8> {
        fn clone(&self) -> move Self;
    }

    impl AmbiguousX<u32, 4> for OpaqueStructU32 {
        type U = i8;
        #[symbol_name = "kita"]
        fn ambiguous(a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous;
    }

    impl OpaqueStructU64 {
        #[symbol_name = "kita1"]
        unsafe extern "C" fn ambiguous() -> Ambiguous;
    }

    #[symbol_name = "kita2"]
    unsafe fn ambiguous2() -> Ambiguous;
}

impl AmbiguousX<u64, 3> for OpaqueStructU64 {
    type U = u8;

    fn ambiguous(_a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

impl AmbiguousX<u32, 4> for OpaqueStructU32 {
    type U = i8;

    fn ambiguous(_a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

impl AmbiguousY for OpaqueStructU64 {
    extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

impl CustomExports for OpaqueStructU8 {
    fn xor(&self, by: u8) -> Box<Self> {
        Box::new(OpaqueStructU8(self.0 ^ by))
    }
}

impl OpaqueStructU32 {
    fn re_exported() -> Box<Self> {
        Box::new(OpaqueStructU32(42))
    }
}

impl OpaqueStructU64 {
    pub const unsafe extern "C" fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

impl OpaqueStructU32 {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

pub const unsafe fn ambiguous1() -> Ambiguous {
    Ambiguous::Fn
}

pub const unsafe extern "Rust" fn ambiguous2() -> Ambiguous {
    Ambiguous::Fn
}

#[test]
fn exported_abi() {
    unsafe extern "C" {
        fn export__OpaqueStructU32__ambiguous() -> <Ambiguous as co3::ExternC>::CType;

        fn ambiguous() -> <Ambiguous as co3::ExternC>::CType;
        fn ambiguous1() -> <Ambiguous as co3::ExternC>::CType;

        #[link_name = "cclone"]
        fn export_opaque_clone_bool(handle: *const c_void) -> *mut c_void;
        #[link_name = "nclone"]
        fn export_non_opaque_clone_bool(
            handle_ptr: *const <NonOpaqueStruct<bool> as co3::ExternC>::CType,
        ) -> <NonOpaqueStruct<bool> as co3::ExternC>::CType;
        #[link_name = "xor_u8"]
        fn export_opaque_xor_u8(handle_ptr: *const c_void, by: u8) -> *mut c_void;

        fn export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous(
            a: &[u8; 3],
        ) -> <Ambiguous as co3::ExternC>::CType;

        #[link_name = "export__OpaqueStructU32__re_exported"]
        fn re_exported() -> *mut c_void;
    }

    unsafe extern "Rust" {
        fn kita(a: *const [i8; 4]) -> <Ambiguous as co3::ExternC>::CType;
        fn kita1() -> <Ambiguous as co3::ExternC>::CType;
        fn kita2() -> <Ambiguous as co3::ExternC>::CType;

        #[link_name = "export__Clone__NonOpaqueStruct_u8__clone"]
        fn export_non_opaque_clone_u8(
            handle: *const <NonOpaqueStruct<u8> as co3::ExternC>::CType,
        ) -> <NonOpaqueStruct<u8> as co3::ExternC>::CType;
        #[link_name = "clone"]
        fn export_opaque_clone_u8(handle: *const c_void) -> *mut c_void;
    }

    unsafe {
        let inherent: Ambiguous = co3::decode(export__OpaqueStructU32__ambiguous()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        let ambiguous_x: Ambiguous = co3::decode(
            export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous(&[12; 3]),
        )
        .unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        let ambiguous_x: Ambiguous = co3::decode(kita(co3::encode(&[13_i8; 4]))).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        let inherent = co3::decode(kita1()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        let custom_fn = co3::decode(kita2()).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        let custom_fn = co3::decode(ambiguous1()).unwrap();
        assert_eq!(Ambiguous::Fn, custom_fn);

        let custom_fn = co3::decode(ambiguous()).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, custom_fn);
    }

    unsafe {
        let custom_fn = Box::from_raw(re_exported().cast::<OpaqueStructU32>());
        assert_eq!(OpaqueStructU32(42_u32), *custom_fn);
    }

    unsafe {
        let opaque_bool = Box::new(OpaqueStructBool(true));
        let opaque_u8 = Box::new(OpaqueStructU8(11_u8));

        let opaque_u8_ptr = (&*opaque_u8 as *const OpaqueStructU8).cast::<c_void>();

        let opaque_bool_handle =
            co3::encode::<&Box<OpaqueStructBool>>(&opaque_bool).cast::<c_void>();
        let opaque_bool_clone =
            Box::from_raw(export_opaque_clone_bool(opaque_bool_handle).cast::<OpaqueStructBool>());
        let opaque_u8_handle = co3::encode::<&Box<OpaqueStructU8>>(&opaque_u8).cast::<c_void>();
        let opaque_u8_clone =
            Box::from_raw(export_opaque_clone_u8(opaque_u8_handle).cast::<OpaqueStructU8>());
        assert_eq!(OpaqueStructU8(11_u8), *opaque_u8_clone);
        let opaque_xor =
            Box::from_raw(export_opaque_xor_u8(opaque_u8_ptr, 7).cast::<OpaqueStructU8>());
        assert_eq!(OpaqueStructU8(12_u8), *opaque_xor);
        assert_eq!(OpaqueStructBool(true), *opaque_bool_clone);
    }

    unsafe {
        let non_opaque_bool = NonOpaqueStruct::A(true);
        let non_opaque_u8 = NonOpaqueStruct::A(11_u8);

        let non_opaque_bool_clone = co3::decode::<NonOpaqueStruct<bool>>(
            export_non_opaque_clone_bool(co3::encode(&non_opaque_bool)),
        )
        .unwrap();
        let non_opaque_u8_clone = co3::decode::<NonOpaqueStruct<u8>>(export_non_opaque_clone_u8(
            co3::encode(&non_opaque_u8),
        ))
        .unwrap();
        assert_eq!(NonOpaqueStruct::A(11_u8), non_opaque_u8_clone);

        assert_eq!(NonOpaqueStruct::A(true), non_opaque_bool_clone);
    }
}
