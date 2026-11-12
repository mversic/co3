use co3::{ReprC, ffi, rust_spec::RustSpec};

trait AmbiguousX<T, const N: usize> {
    type U;

    fn ambiguous(a: &[Self::U; N]) -> Ambiguous;
}

trait AmbiguousY {
    extern "C" fn ambiguous() -> Ambiguous;
}

trait CustomExports {
    fn xor(&self, by: u8) -> OwnedOpaqueStructU8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
pub enum Ambiguous {
    #[repr_c(allow_zero_tag)]
    AmbiguousX,
    AmbiguousY,
    Inherent,
    Fn,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, RustSpec)]
#[rust_spec(opaque)]
pub(crate) struct OpaqueStructU8(u8);

#[derive(Debug, Clone, Copy, PartialEq, RustSpec)]
#[rust_spec(opaque)]
pub(crate) struct OpaqueStructU32(u32);

#[derive(Debug, Clone, Copy, PartialEq, RustSpec)]
#[rust_spec(opaque)]
pub(crate) struct OpaqueStructU64(u64);

#[derive(Debug, Clone, Copy, PartialEq, RustSpec)]
#[rust_spec(opaque)]
pub(crate) struct OpaqueStructBool(bool);

#[derive(Debug, Clone, Copy, PartialEq, RustSpec)]
#[rust_spec(opaque)]
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
        fn re_exported() -> move OwnedOpaqueStructU32;
    }

    impl CustomExports for OpaqueStructU8 {
        #[symbol_name = "xor_u8"]
        fn xor(&self, by: u8) -> move OwnedOpaqueStructU8;
    }

    impl Clone for OwnedOpaqueStructBool {
        #[symbol_name = "cclone"]
        fn clone(&self) -> move Self;
    }

    impl Clone for OwnedOpaqueStructI32 {
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
    #![unsafe(export("C"))]

    impl Clone for OwnedOpaqueStructU8 {
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
    fn xor(&self, by: u8) -> OwnedOpaqueStructU8 {
        Box::new(OpaqueStructU8(self.0 ^ by))
    }
}

impl OpaqueStructU32 {
    fn re_exported() -> OwnedOpaqueStructU32 {
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

mod imported {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "export__OpaqueStructU32__ambiguous"]
        pub fn inherent() -> Ambiguous;
        #[symbol_name = "ambiguous"]
        pub fn trait_method() -> Ambiguous;
        #[symbol_name = "ambiguous1"]
        pub fn custom() -> Ambiguous;
        #[symbol_name = "cclone"]
        pub fn clone_bool(handle: &OwnedOpaqueStructBool) -> move OwnedOpaqueStructBool;
        #[symbol_name = "nclone"]
        pub fn clone_non_opaque_bool(handle: &NonOpaqueStruct<bool>) -> move NonOpaqueStruct<bool>;
        #[symbol_name = "xor_u8"]
        pub fn xor_u8(handle: &OpaqueStructU8, by: u8) -> move OwnedOpaqueStructU8;
        #[symbol_name = "export__AmbiguousX_u64_3__OpaqueStructU64__ambiguous"]
        pub fn ambiguous_u64(a: &[u8; 3]) -> Ambiguous;
        #[symbol_name = "export__OpaqueStructU32__re_exported"]
        pub fn re_exported() -> move OwnedOpaqueStructU32;
        #[symbol_name = "kita"]
        pub fn ambiguous_u32(a: &[i8; 4]) -> Ambiguous;
        #[symbol_name = "kita1"]
        pub fn custom_inherent() -> Ambiguous;
        #[symbol_name = "kita2"]
        pub fn custom_second() -> Ambiguous;
        #[symbol_name = "export__Clone__NonOpaqueStruct_u8__clone"]
        pub fn clone_non_opaque_u8(handle: &NonOpaqueStruct<u8>) -> move NonOpaqueStruct<u8>;
        #[symbol_name = "clone"]
        pub fn clone_u8(handle: &OwnedOpaqueStructU8) -> move OwnedOpaqueStructU8;
    }
}

#[test]
fn exported_abi() {
    assert_eq!(Ambiguous::Inherent, imported::inherent());
    assert_eq!(Ambiguous::AmbiguousX, imported::ambiguous_u64(&[12; 3]));
    assert_eq!(Ambiguous::AmbiguousX, imported::ambiguous_u32(&[13; 4]));
    assert_eq!(Ambiguous::Inherent, imported::custom_inherent());
    assert_eq!(Ambiguous::Fn, imported::custom_second());
    assert_eq!(Ambiguous::Fn, imported::custom());
    assert_eq!(Ambiguous::AmbiguousY, imported::trait_method());
    assert_eq!(OpaqueStructU32(42), *imported::re_exported());

    let opaque_bool = Box::new(OpaqueStructBool(true));
    let opaque_u8 = Box::new(OpaqueStructU8(11));
    assert_eq!(OpaqueStructBool(true), *imported::clone_bool(&opaque_bool));
    assert_eq!(OpaqueStructU8(11), *imported::clone_u8(&opaque_u8));
    assert_eq!(OpaqueStructU8(12), *imported::xor_u8(&opaque_u8, 7));

    let non_opaque_bool = NonOpaqueStruct::A(true);
    let non_opaque_u8 = NonOpaqueStruct::A(11);
    assert_eq!(
        non_opaque_bool,
        imported::clone_non_opaque_bool(&non_opaque_bool)
    );
    assert_eq!(non_opaque_u8, imported::clone_non_opaque_u8(&non_opaque_u8));
}
