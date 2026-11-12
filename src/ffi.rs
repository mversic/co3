//! Conversions for types in [`core::ffi`].

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, ffi::CString};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    ffi::{CStr as RustCStr, c_void},
    mem::ManuallyDrop,
};

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    primitives::primitive_derive,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
};
#[cfg(feature = "alloc")]
use crate::{boxed::CBox, niche::Niche};

macro_rules! c_alias_carrier {
    ($name:ident, $alias:ty) => {
        #[repr(transparent)]
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, RustSpec)]
        pub struct $name(
            // NOTE: The wrapped type must remain private to prevent Rust literals to coerce to it.
            // Privacy helps keep the use of the type always portable to different architectures.
            $alias,
        );

        impl $name {
            /// Creates a carrier from the target's C type. An unsuffixed numeric literal is
            /// inferred as that type.
            pub const fn new(value: $alias) -> Self {
                Self(value)
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(&self.0, f)
            }
        }

        impl core::str::FromStr for $name {
            type Err = <$alias as core::str::FromStr>::Err;

            fn from_str(source: &str) -> Result<Self, Self::Err> {
                source.parse::<$alias>().map(Self)
            }
        }

        primitive_derive! { $name }
    };
}

macro_rules! c_binary_ops {
    ($name:ident; $( $op:ident::$method:ident, $assign:ident::$assign_method:ident ),* $(,)?) => {
        $(
            impl core::ops::$op for $name {
                type Output = Self;

                fn $method(self, rhs: Self) -> Self::Output {
                    Self(core::ops::$op::$method(self.0, rhs.0))
                }
            }

            impl core::ops::$assign for $name {
                fn $assign_method(&mut self, rhs: Self) {
                    core::ops::$assign::$assign_method(&mut self.0, rhs.0);
                }
            }

        )*
    };
}

macro_rules! c_integer_ops {
    ($name:ident) => {
        c_binary_ops! {
            $name;
            Add::add, AddAssign::add_assign,
            Sub::sub, SubAssign::sub_assign,
            Mul::mul, MulAssign::mul_assign,
            Div::div, DivAssign::div_assign,
            Rem::rem, RemAssign::rem_assign,
            BitAnd::bitand, BitAndAssign::bitand_assign,
            BitOr::bitor, BitOrAssign::bitor_assign,
            BitXor::bitxor, BitXorAssign::bitxor_assign,
            Shl::shl, ShlAssign::shl_assign,
            Shr::shr, ShrAssign::shr_assign
        }

        impl core::ops::Not for $name {
            type Output = Self;

            fn not(self) -> Self::Output {
                Self(!self.0)
            }
        }
    };
}

macro_rules! c_signed_ops {
    ($name:ident) => {
        impl core::ops::Neg for $name {
            type Output = Self;

            fn neg(self) -> Self::Output {
                Self(-self.0)
            }
        }
    };
}

macro_rules! c_integer_conversions {
    (
        $name:ident, $alias:ty;
        from: [$($from:ty),* $(,)?];
        try_from: [$($try_from:ty),* $(,)?];
        into: [$($into:ty),* $(,)?];
        try_into: [$($try_into:ty),* $(,)?]
    ) => {
        impl $name {
            pub const MIN: Self = Self(<$alias>::MIN);
            pub const MAX: Self = Self(<$alias>::MAX);
            pub const BITS: u32 = <$alias>::BITS;
        }

        impl Eq for $name {}

        #[allow(clippy::derive_ord_xor_partial_ord)]
        impl Ord for $name {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.0.cmp(&other.0)
            }
        }

        impl core::hash::Hash for $name {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                core::hash::Hash::hash(&self.0, state);
            }
        }

        impl core::fmt::Binary for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Binary::fmt(&self.0, f)
            }
        }

        impl core::fmt::Octal for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Octal::fmt(&self.0, f)
            }
        }

        impl core::fmt::LowerHex for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::LowerHex::fmt(&self.0, f)
            }
        }

        impl core::fmt::UpperHex for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::UpperHex::fmt(&self.0, f)
            }
        }

        $(
            impl From<$from> for $name {
                fn from(value: $from) -> Self {
                    Self(<$alias>::from(value))
                }
            }
        )*

        $(
            #[allow(clippy::infallible_try_from)]
            impl TryFrom<$try_from> for $name {
                type Error = <$alias as TryFrom<$try_from>>::Error;

                fn try_from(value: $try_from) -> Result<Self, Self::Error> {
                    <$alias>::try_from(value).map(Self)
                }
            }
        )*

        $(
            impl From<$name> for $into {
                fn from(value: $name) -> Self {
                    Self::from(value.0)
                }
            }
        )*

        $(
            #[allow(clippy::infallible_try_from)]
            impl TryFrom<$name> for $try_into {
                type Error = <$try_into as TryFrom<$alias>>::Error;

                fn try_from(value: $name) -> Result<Self, Self::Error> {
                    Self::try_from(value.0)
                }
            }
        )*

        c_integer_ops! { $name }
    };
}

c_alias_carrier! { c_char, core::ffi::c_char }
c_alias_carrier! { c_schar, core::ffi::c_schar }
c_alias_carrier! { c_uchar, core::ffi::c_uchar }
c_alias_carrier! { c_short, core::ffi::c_short }
c_alias_carrier! { c_ushort, core::ffi::c_ushort }
c_alias_carrier! { c_int, core::ffi::c_int }
c_alias_carrier! { c_uint, core::ffi::c_uint }
c_alias_carrier! { c_long, core::ffi::c_long }
c_alias_carrier! { c_ulong, core::ffi::c_ulong }
c_alias_carrier! { c_longlong, core::ffi::c_longlong }
c_alias_carrier! { c_ulonglong, core::ffi::c_ulonglong }
c_alias_carrier! { c_float, core::ffi::c_float }
c_alias_carrier! { c_double, core::ffi::c_double }

macro_rules! c_integer_conversions_i8 {
    ($name:ident, $alias:ty) => {
        c_integer_conversions! {
            $name, $alias;
            from: [i8, bool];
            try_from: [u8, i16, u16, i32, u32, i64, u64, i128, u128, isize, usize];
            into: [i8, i16, i32, i64, i128, isize, f32, f64];
            try_into: [u8, u16, u32, u64, u128, usize, bool]
        }
        c_signed_ops! { $name }
    };
}

macro_rules! c_integer_conversions_u8 {
    ($name:ident, $alias:ty) => {
        c_integer_conversions! {
            $name, $alias;
            from: [u8, bool];
            try_from: [i8, i16, u16, i32, u32, i64, u64, i128, u128, isize, usize];
            into: [u8, i16, u16, i32, u32, i64, u64, i128, u128, isize, usize, f32, f64];
            try_into: [i8, bool]
        }
    };
}

macro_rules! c_integer_conversions_i16 {
    ($name:ident, $alias:ty) => {
        c_integer_conversions! {
            $name, $alias;
            from: [i8, u8, i16, bool];
            try_from: [u16, i32, u32, i64, u64, i128, u128, isize, usize];
            into: [i16, i32, i64, i128, f32, f64];
            try_into: [i8, u8, u16, u32, u64, u128, usize, bool]
        }
        c_signed_ops! { $name }
    };
}

macro_rules! c_integer_conversions_u16 {
    ($name:ident, $alias:ty) => {
        c_integer_conversions! {
            $name, $alias;
            from: [u8, u16, bool];
            try_from: [i8, i16, i32, u32, i64, u64, i128, u128, isize, usize];
            into: [u16, i32, u32, i64, u64, i128, u128, usize, f32, f64];
            try_into: [i8, u8, i16, isize, bool]
        }
    };
}

macro_rules! c_integer_conversions_i64 {
    ($name:ident, $alias:ty) => {
        c_integer_conversions! {
            $name, $alias;
            from: [i8, u8, i16, u16, i32, u32, i64, bool];
            try_from: [u64, i128, u128, isize, usize];
            into: [i64, i128];
            try_into: [i8, u8, i16, u16, i32, u32, u64, u128, isize, usize, bool]
        }
        c_signed_ops! { $name }
    };
}

macro_rules! c_integer_conversions_u64 {
    ($name:ident, $alias:ty) => {
        c_integer_conversions! {
            $name, $alias;
            from: [u8, u16, u32, u64, bool];
            try_from: [i8, i16, i32, i64, i128, u128, isize, usize];
            into: [u64, i128, u128];
            try_into: [i8, u8, i16, u16, i32, u32, i64, isize, usize, bool]
        }
    };
}

c_integer_conversions_i8! { c_schar, core::ffi::c_schar }
c_integer_conversions_u8! { c_uchar, core::ffi::c_uchar }
c_integer_conversions_i16! { c_short, core::ffi::c_short }
c_integer_conversions_u16! { c_ushort, core::ffi::c_ushort }
c_integer_conversions_i64! { c_longlong, core::ffi::c_longlong }
c_integer_conversions_u64! { c_ulonglong, core::ffi::c_ulonglong }

// `c_int` and `c_uint` are 16 bits on AVR and MSP430, and 32 bits elsewhere.
c_integer_conversions! {
    c_int, core::ffi::c_int;
    from: [i8, u8, i16, bool];
    try_from: [u16, i32, u32, i64, u64, i128, u128, isize, usize];
    into: [i32, i64, i128, f64];
    try_into: [i8, u8, i16, u16, u32, u64, u128, isize, usize, bool]
}
c_signed_ops! { c_int }

c_integer_conversions! {
    c_uint, core::ffi::c_uint;
    from: [u8, u16, bool];
    try_from: [i8, i16, i32, u32, i64, u64, i128, u128, isize, usize];
    into: [u32, i64, u64, i128, u128, f64];
    try_into: [i8, u8, i16, u16, i32, isize, usize, bool]
}

// `c_long` may be i32 or i64, so expose the same conversions on both targets.
c_integer_conversions! {
    c_long, core::ffi::c_long;
    from: [i8, u8, i16, u16, i32, bool];
    try_from: [u32, i64, u64, i128, u128, isize, usize];
    into: [i64, i128];
    try_into: [i8, u8, i16, u16, i32, u32, u64, u128, isize, usize, bool]
}
c_signed_ops! { c_long }

// `c_ulong` may be u32 or u64.
c_integer_conversions! {
    c_ulong, core::ffi::c_ulong;
    from: [u8, u16, u32, bool];
    try_from: [i8, i16, i32, i64, u64, i128, u128, isize, usize];
    into: [u64, i128, u128];
    try_into: [i8, u8, i16, u16, i32, u32, i64, isize, usize, bool]
}

// `c_char` may be i8 or u8. Neither signedness is assumed by its API.
c_integer_conversions! {
    c_char, core::ffi::c_char;
    from: [bool];
    try_from: [i8, u8, i16, u16, i32, u32, i64, u64, i128, u128, isize, usize];
    into: [i16, i32, i64, i128, isize, f32, f64];
    try_into: [u8, u16, u32, u64, u128, usize, bool]
}

macro_rules! c_float_from {
    ($name:ident, $alias:ty; $($from:ty),* $(,)?) => {$ (
        impl From<$from> for $name {
            fn from(value: $from) -> Self {
                Self(<$alias>::from(value))
            }
        }
    )*};
}

c_float_from! { c_float, core::ffi::c_float; bool, i8, u8, i16, u16 }
c_float_from! { c_double, core::ffi::c_double; bool, i8, u8, i16, u16, f32 }

impl From<f32> for c_float {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<c_float> for f32 {
    fn from(value: c_float) -> Self {
        value.0
    }
}

impl From<c_double> for f64 {
    #[allow(clippy::useless_conversion)]
    fn from(value: c_double) -> Self {
        Self::from(value.0)
    }
}

c_binary_ops! {
    c_float;
    Add::add, AddAssign::add_assign,
    Sub::sub, SubAssign::sub_assign,
    Mul::mul, MulAssign::mul_assign,
    Div::div, DivAssign::div_assign,
    Rem::rem, RemAssign::rem_assign
}
c_binary_ops! {
    c_double;
    Add::add, AddAssign::add_assign,
    Sub::sub, SubAssign::sub_assign,
    Mul::mul, MulAssign::mul_assign,
    Div::div, DivAssign::div_assign,
    Rem::rem, RemAssign::rem_assign
}
c_signed_ops! { c_float }
c_signed_ops! { c_double }

impl core::fmt::LowerExp for c_float {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerExp::fmt(&self.0, f)
    }
}

impl core::fmt::UpperExp for c_float {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperExp::fmt(&self.0, f)
    }
}

impl core::fmt::LowerExp for c_double {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerExp::fmt(&self.0, f)
    }
}

impl core::fmt::UpperExp for c_double {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperExp::fmt(&self.0, f)
    }
}

impl From<c_float> for f64 {
    fn from(value: c_float) -> Self {
        Self::from(value.0)
    }
}

/// A nul-terminated value represented across the ABI by a pointer to its data.
///
/// # Safety
///
/// - Implementor must be a DST; a sized type, including a ZST, must not implement this trait.
/// - Implementations must identify the same valid nul-terminated data through every method.
pub unsafe trait NulTerminatedBuf {
    /// Header of a nul-terminated pointer.
    type Header;

    /// Returns a raw pointer to the underlying data.
    fn as_ptr(value: &Self) -> *const Self::Header;

    /// Forms a nul-terminated reference from a data pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null and properly aligned. The initialized data through the first nul
    /// must be readable within one allocation, fit in `isize::MAX` bytes, and remain unchanged for
    /// `'a`.
    unsafe fn from_raw<'a>(ptr: *const Self::Header) -> &'a Self;

    /// Consumes the `Box`, returning a wrapped `NonNull` pointer.
    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header>;

    /// Constructs a box from a `NonNull` pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must come from `Self::into_non_null`, retain its allocation provenance,
    /// and still own the complete allocation.
    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Header>) -> Box<Self>;
}

unsafe impl<R: NulTerminatedBuf + ?Sized> NulTerminatedBuf for ManuallyDrop<R> {
    type Header = R::Header;

    fn as_ptr(value: &Self) -> *const Self::Header {
        R::as_ptr(value)
    }

    unsafe fn from_raw<'a>(ptr: *const Self::Header) -> &'a Self {
        let inner = unsafe { R::from_raw(ptr) };
        unsafe { &*(inner as *const R as *const Self) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
        let inner = unsafe { Box::from_raw(Box::into_raw(self) as *mut R) };
        R::into_non_null(inner)
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Header>) -> Box<Self> {
        let inner = unsafe { R::from_non_null(ptr) };
        unsafe { Box::from_raw(Box::into_raw(inner) as *mut Self) }
    }
}

impl ReprC for RustCStr {
    type CType = c_char;
}

unsafe impl NulTerminatedBuf for RustCStr {
    type Header = c_char;

    fn as_ptr(value: &Self) -> *const Self::Header {
        value.as_ptr().cast()
    }

    unsafe fn from_raw<'a>(ptr: *const Self::Header) -> &'a Self {
        unsafe { Self::from_ptr(ptr.cast()) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
        let raw = self.into_c_string().into_raw();
        unsafe { NonNull::new_unchecked(raw.cast()) }
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Header>) -> Box<Self> {
        unsafe { CString::from_raw(ptr.cast().as_ptr()) }.into_boxed_c_str()
    }
}

#[cfg(feature = "alloc")]
impl ReprC for CString {
    type CType = CBox<RustCStr>;
}
#[cfg(feature = "alloc")]
unsafe impl EncodeMove for CString {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
#[cfg(feature = "alloc")]
unsafe impl EncodeMoveUnchecked for CString {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.into_boxed_c_str().soft_encode_unchecked(&mut ()) }
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeMove<'d> for CString {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        unsafe { Box::<RustCStr>::soft_decode(source, &mut ()) }.map(Into::into)
    }
}

#[cfg(feature = "alloc")]
impl Encode for CString {}
#[cfg(feature = "alloc")]
impl crate::EncodeUnchecked for CString {}
#[cfg(feature = "alloc")]
impl Decode<'_> for CString {}

#[cfg(feature = "alloc")]
impl Niche for CString {
    const NICHE: Self::CType = CBox::null();

    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}

crate::__impl_borrowed_identity!([] c_void);

impl ReprC for c_void {
    type CType = Self;
}
unsafe impl CType for c_void {}

#[cfg(test)]
mod tests {

    use core::cell::{Cell, UnsafeCell};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::Decode;

    #[test]
    fn c_int_checked_length_conversion() {
        assert_eq!(c_int::try_from(42_usize), Ok(c_int::new(42)));
        assert!(c_int::try_from(usize::MAX).is_err());
    }

    #[test]
    fn c_integer_carriers_follow_primitive_conversions() {
        let mut value = c_int::new(12);
        assert_eq!(i32::from(value), 12);
        assert_eq!(i64::from(value), 12);
        assert_eq!(c_int::try_from(12_usize), Ok(value));
        assert!(u8::try_from(c_int::new(-1)).is_err());
        assert_eq!(bool::try_from(c_int::new(1)), Ok(true));
        assert!(bool::try_from(c_int::new(2)).is_err());

        value += c_int::new(3);
        assert_eq!(i32::from(value + c_int::new(2)), 17);
        assert_eq!(i32::from(value).checked_add(1), Some(16));
        value = c_int::new(18);
        assert_eq!(i32::from(value), 18);
        assert_eq!(c_int::MIN.0, core::ffi::c_int::MIN);
        fn assert_lower_hex<T: core::fmt::LowerHex>() {}
        assert_lower_hex::<c_int>();

        let character = c_char::try_from(127_i16).unwrap();
        assert_eq!(i16::from(character), 127);
        assert!(c_char::try_from(usize::MAX).is_err());
        assert_eq!(c_long::try_from(7_usize), Ok(c_long::new(7)));
        assert_eq!(u128::from(c_ulonglong::new(9)), 9);
    }

    #[test]
    fn c_float_carriers_follow_primitive_comparisons() {
        let mut value = c_float::new(1.0);
        assert_eq!(f32::from(value), 1.0_f32);
        value += c_float::new(0.5);
        assert_eq!(f32::from(value), 1.5_f32);
        assert_eq!(f64::from(value), 1.5_f64);
        let nan = c_double::new(0.0) / c_double::new(0.0);
        assert_eq!(nan.partial_cmp(&nan), None);
        assert_eq!(f64::from(c_double::new(2.0)), 2.0_f64);
    }

    #[test]
    fn nul_terminated_buf_wrapper_impls() {
        assert_impl_all!(RustCStr: NulTerminatedBuf);
        assert_not_impl_any!(UnsafeCell<RustCStr>: NulTerminatedBuf);
        assert_not_impl_any!(Cell<RustCStr>: NulTerminatedBuf);
        assert_impl_all!(ManuallyDrop<RustCStr>: NulTerminatedBuf);
        assert_impl_all!(&ManuallyDrop<RustCStr>: Encode, Decode<'static>);
        assert_not_impl_any!(&mut RustCStr: ReprC);
        assert_not_impl_any!(&mut RustCStr: Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        {
            assert_impl_all!(Box<ManuallyDrop<RustCStr>>: EncodeMove, crate::stored::DecodeMove<'static>);
            assert_impl_all!(CString: Encode);
        }
    }

    #[test]
    fn cstr_reference_round_trip() {
        let original = c"hello";
        let encoded = crate::encode(original);
        assert_eq!(encoded.as_ptr().cast(), original.as_ptr());

        let decoded: &RustCStr = unsafe { crate::decode(encoded) }.unwrap();
        assert_eq!(decoded, original);

        let none: Option<&RustCStr> =
            unsafe { crate::decode(crate::encode(None::<&RustCStr>)) }.unwrap();
        assert!(none.is_none());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn cstr_box_round_trip() {
        let original = CString::new("owned").unwrap().into_boxed_c_str();
        let ptr = original.as_ptr();
        let encoded = crate::stored::encode_move(original);
        assert_eq!(encoded.as_ptr().cast_const().cast(), ptr);
        let decoded: Box<RustCStr> = unsafe { crate::stored::decode_move(encoded) }.unwrap();
        assert_eq!(decoded.as_ptr(), ptr);
        assert_eq!(&*decoded, c"owned");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn cstring_round_trip() {
        let original = CString::new("owned").unwrap();
        let encoded = crate::encode(original.clone());
        let decoded: CString = unsafe { crate::decode(encoded) }.unwrap();
        assert_eq!(decoded, original);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    use alloc::string::String;

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(Clone, Copy, RustSpec, ReprC)]
    #[repr(C)]
    struct AliasFields<T>(c_int, T, c_long);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn alias_fields_preserve_borrowed_types_and_values() {
        let view = AliasFields(c_int::new(3), 4_u32, c_long::new(5));
        let borrowed_lowered = co3::borrow::encode(view).0;
        let _: c_int = borrowed_lowered.0;
        let _: c_long = borrowed_lowered.2;
        let view = AliasFields(c_int::new(3), 4_u32, c_long::new(5));
        let recovered: AliasFields<u32> = view;
        assert_eq!(
            (recovered.0, recovered.1, recovered.2),
            (c_int::new(3), 4, c_long::new(5))
        );
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(Clone, Copy, RustSpec, ReprC)]
    #[repr_c(identity)]
    #[repr(C)]
    struct Value(u8);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn borrowed_return() -> co3::borrow::CBorrowed<Box<Value>> {
        co3::borrow::CBorrowed(co3::boxed::CBox::null())
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    co3::ffi! {
        #![unsafe(extern("C"))]
        type BorrowedReturn = raw extern "C" fn() -> Box<Value>;
        type InteriorUnpacked = extern "C" fn(#[unpack(*mut core::cell::Cell<u8>, usize)] &[core::cell::Cell<u8>]) -> usize;
        type RegularUnpacked = extern "C" fn(#[unpack(_, _)] &mut [u8]) -> usize;
        type Composite = (String, raw fn(String));

        fn interior_slice(#[unpack(*mut core::cell::Cell<u8>, usize)] values: &[core::cell::Cell<u8>]) -> usize;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn generated_callbacks_use_borrowed_carriers() {
        let _: BorrowedReturn = borrowed_return;
        static_assertions::assert_type_eq_all!(
            InteriorUnpacked,
            Option<unsafe extern "C" fn(*mut core::cell::Cell<u8>, usize) -> usize>
        );
        static_assertions::assert_type_eq_all!(
            RegularUnpacked,
            Option<unsafe extern "C" fn(*mut u8, usize) -> usize>
        );
        static_assertions::assert_type_eq_all!(
            Composite,
            <(String, unsafe extern "C" fn(co3::borrow::CBorrowed<String>)) as ReprC>::CType
        );
    }
    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[unsafe(export_name = "co3_unit_borrowed_import")]
    extern "C" fn borrowed_import_source(value: *const u8) -> *const u8 {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    co3::ffi! {
        #![unsafe(extern("C"))]
        #[symbol_name = "co3_unit_borrowed_import"]
        raw fn borrowed_import<'a>(value: &'a u8) -> &'a u8;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn raw_import_preserves_uniform_borrowed_carrier_signature() {
        use co3::{CBorrowed, reference::CRef};
        fn check<'a>() {
            let _: unsafe extern "C" fn(CBorrowed<&'a u8>) -> CBorrowed<&'a u8> = borrowed_import;
        }
        check();
        let input = 7;
        let output = unsafe { borrowed_import(CBorrowed::new(CRef::from_ref(&input))) };
        assert_eq!(
            CBorrowed::into_inner(output).as_ptr(),
            core::ptr::from_ref(&input)
        );
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    use crate::{
        CBorrowed, Error, ffi,
        ops::{CFn0, CFn1, CFn2, CFn12},
        option::COption,
        raw,
        reference::{CRef, CSoftRef, CSoftRefMut},
        tuple::Tuple3,
        utf8::Utf8String,
    };
    #[cfg(all(feature = "alloc", feature = "derive"))]
    use alloc::{vec, vec::Vec};
    #[cfg(all(feature = "alloc", feature = "derive"))]
    use core::{
        ffi::CStr,
        num::NonZeroU8,
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[cfg(all(feature = "alloc", feature = "derive"))]
    type Text = String;
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn string_len(input: &String) -> usize {
        input.len()
    }
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn alias_len(input: &Text) -> usize {
        input.len()
    }
    #[cfg(all(feature = "alloc", feature = "derive"))]
    co3::raw! {
        fn string_len(#[soft] input: &String) -> usize;
        fn alias_len(#[soft] input: &Text) -> usize;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn exported_shared_string_and_alias_use_the_decoder() {
        let source = Utf8String::from_string(String::from("héllo"));
        assert_eq!(
            unsafe { string_len_raw(CBorrowed::new(CSoftRef::from_ref(&source))) },
            "héllo".len()
        );
        assert_eq!(
            unsafe { alias_len_raw(CBorrowed::new(CSoftRef::from_ref(&source))) },
            "héllo".len()
        );
        assert_eq!(unsafe { source.0.into_vec() }.unwrap(), "héllo".as_bytes());
    }

    #[derive(Clone, Copy, RustSpec, ReprC)]
    #[repr_c(identity)]
    #[repr(C)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct StructWithCallback<'a>(Option<unsafe extern "C" fn(CRef<'a, u32>) -> u32>);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, RustSpec, ReprC)]
    #[repr_c(identity)]
    #[repr(C)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct CallbackValue(u8);

    #[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct CallbackRustValue(u8);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl CallbackRustValue {
        fn scaled(&self, factor: u8) -> Self {
            Self(self.0 * factor)
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    trait Echo: Sized + ReprC<CType: Sized> {
        fn echo(self) -> Self {
            self
        }

        unsafe extern "C" fn echo_raw(value: CBorrowed<Self>) -> CBorrowed<Self>;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    type CallbackPathExisting = CCallback;

    #[cfg(all(feature = "alloc", feature = "derive"))]
    type CompanionPathAlias = CallbackPathExisting;

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" fn increment_pointer(value: CRef<'_, u32>) -> u32 {
        (unsafe { *value.as_ptr() }) + 1
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" fn return_pointer(value: CRef<'_, u32>) -> CRef<'_, u32> {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" fn ignore_pointer(_: CRef<'_, u32>) {}

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" fn update_option(value: CSoftRefMut<Option<u8>, true>) -> u32 {
        let value = value.as_ptr();
        unsafe { *value = COption::Some(42) };
        7
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn return_forty_two() -> u32 {
        42
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "system" fn increment_system(value: CRef<'_, u32>) -> u32 {
        (unsafe { *value.as_ptr() }) + 2
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" fn add_from_pointer(value: CRef<'_, u32>, offset: u32) -> u32 {
        (unsafe { *value.as_ptr() }) + offset
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn sum_twelve(
        a0: u32,
        a1: u32,
        a2: u32,
        a3: u32,
        a4: u32,
        a5: u32,
        a6: u32,
        a7: u32,
        a8: u32,
        a9: u32,
        a10: u32,
        a11: u32,
    ) -> u32 {
        a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn double_rust(value: CallbackRustValue) -> CallbackRustValue {
        CallbackRustValue(value.0 * 2)
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn sum_pair(
        left: CBorrowed<CallbackValue>,
        right: CBorrowed<CallbackValue>,
    ) -> CBorrowed<CallbackValue> {
        CBorrowed::new(CallbackValue(
            CBorrowed::into_inner(left).0 + CBorrowed::into_inner(right).0,
        ))
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn apply_callback(
        callback: CCallback,
        left: CallbackValue,
        right: CallbackValue,
    ) -> CallbackValue {
        CBorrowed::into_inner(unsafe { callback(CBorrowed::new(left), CBorrowed::new(right)) })
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn apply_optional_callback(callback: Option<CCallback>, value: CallbackValue) -> CallbackValue {
        callback.map_or(value, |call| {
            CBorrowed::into_inner(unsafe {
                call(CBorrowed::new(value), CBorrowed::new(CallbackValue(1)))
            })
        })
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn sum_native(left: CallbackValue, right: CallbackValue) -> CallbackValue {
        CallbackValue(left.0 + right.0)
    }

    #[unsafe(export_name = "abi_c_callback_raw_method")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn add_values(left: CallbackValue, right: CallbackValue) -> CallbackValue {
        CallbackValue(left.0 + right.0)
    }

    #[unsafe(export_name = "abi_system_callback")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "system" fn add_system_values(
        left: CallbackValue,
        right: CallbackValue,
    ) -> CallbackValue {
        CallbackValue(left.0 + right.0)
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn return_owned(value: Box<CallbackValue>) -> Box<CallbackValue> {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn borrow_rust_value(value: CallbackRustValue) -> CallbackRustValue {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(export("C"))]
        #![symbol_prefix = "abi_c_callback"]

        type CCallback = raw extern "C" fn(CallbackValue, CallbackValue) -> CallbackValue;

        fn apply_callback(callback: CCallback, left: CallbackValue, right: CallbackValue) -> CallbackValue;
        fn apply_optional_callback(callback: Option<CCallback>, value: CallbackValue) -> CallbackValue;
        #[symbol_name = "abi_borrowed_value"]
        fn borrow_rust_value(value: CallbackRustValue) -> CallbackRustValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        #[symbol_name = "abi_borrowed_value"]
        pub fn imported_borrow_rust_value(value: CallbackRustValue) -> CallbackRustValue;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn borrowed_return_round_trips_through_custom_symbol() {
        let input = CallbackRustValue(7);
        assert_eq!(imported_borrow_rust_value(input), input);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback_exported_raw"]

        #[symbol_name = "abi_c_callback_exported_raw__sum_native"]

        pub fn imported_sum_native(left: CallbackValue, right: CallbackValue) -> CallbackValue;
        #[symbol_name = "abi_c_callback_exported_raw__sum_native"]
        pub raw fn imported_sum_native_raw(left: CallbackValue, right: CallbackValue) -> CallbackValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("system"))]
        #![symbol_prefix = "abi_system_callback"]

        #[symbol_name = "abi_system_callback"]
        pub raw fn imported_system_sum_native(left: CallbackValue, right: CallbackValue) -> CallbackValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(export("C"))]
        #![symbol_prefix = "abi_c_callback_exported_raw"]

        fn sum_native(left: CallbackValue, right: CallbackValue) -> CallbackValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        type ImportedCallback = raw extern "C" fn(CallbackValue, CallbackValue) -> CallbackValue;

        #[symbol_name = "abi_c_callback__apply_callback"]

        pub fn imported_apply_callback(callback: ImportedCallback, left: CallbackValue, right: CallbackValue) -> CallbackValue;
        #[symbol_name = "abi_c_callback__apply_callback"]
        pub raw fn imported_apply_callback_raw(callback: ImportedCallback, left: CallbackValue, right: CallbackValue) -> CallbackValue;
        #[symbol_name = "abi_c_callback__apply_optional_callback"]
        pub fn imported_apply_optional_callback(callback: Option<ImportedCallback>, value: CallbackValue) -> CallbackValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_c_callback"]

        #[symbol_name = "abi_c_callback__apply_callback"]

        pub raw fn raw_only_apply_callback(callback: CCallback, left: CallbackValue, right: CallbackValue) -> CallbackValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        impl CallbackValue {
            #[symbol_name = "abi_c_callback_raw_method"]
            pub raw fn add_raw(self, other: CallbackValue) -> CallbackValue;
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        type CallbackBorrowedReturn = raw extern "C" fn() -> Box<CallbackValue>;
        type BorrowedRustValue = raw extern "C" fn(CallbackRustValue) -> CallbackRustValue;
        type OwnedTransform = raw extern "C" fn(move Box<CallbackValue>) -> move Box<CallbackValue>;
        type CompanionCallback = raw extern "C" fn(CallbackValue, CallbackValue) -> CallbackValue;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn borrow_rust_value(value: CallbackRustValue) -> CallbackRustValue;
        fn return_owned(value: move Box<CallbackValue>) -> move Box<CallbackValue>;
        fn double_rust(value: CallbackRustValue) -> move CallbackRustValue;
        pub fn sum_native(left: CallbackValue, right: CallbackValue) -> CallbackValue;

        impl Echo for CallbackValue {
            fn echo(self) -> CallbackValue;
        }

        impl CallbackRustValue {
            pub fn scaled(#[soft] &self, factor: u8) -> move CallbackRustValue;
        }
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn zero_argument_callback_decodes_result() {
        let callback: unsafe extern "C" fn() -> _ = return_forty_two;
        let result: Option<u32> = unsafe { callback.call() };
        assert_eq!(result, Some(42));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn stored_c_callback_accepts_rust_reference() {
        fn assert_first_arg<'a, T: CFn1<Arg1 = CRef<'a, u32>>>()
        where
            Option<T>: CType,
        {
        }
        assert_first_arg::<unsafe extern "C" fn(CRef<'static, u32>) -> u32>();

        let callback = StructWithCallback(Some(increment_pointer));
        let value = 41_u32;
        let result: Option<u32> = unsafe { callback.0.unwrap().call(&value) };
        assert_eq!(result, Some(42));

        let void_callback: unsafe extern "C" fn(_) = ignore_pointer;
        let result: Option<()> = unsafe { void_callback.call(&value) };
        assert_eq!(result, Some(()));

        let system_callback: unsafe extern "system" fn(_) -> _ = increment_system;
        let result: Option<u32> = unsafe { system_callback.call(&value) };
        assert_eq!(result, Some(43));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn callback_returns_rust_reference() {
        let callback: unsafe extern "C" fn(_) -> _ = return_pointer;
        let value = 42_u32;
        let result: Option<&u32> = unsafe { callback.call(&value) };

        assert_eq!(result, Some(&value));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn soft_call_synchronizes_mutable_argument() {
        let callback: unsafe extern "C" fn(_) -> _ = update_option;
        let mut value = Some(1_u8);

        let result: Option<u32> = unsafe { callback.soft_call(&mut value) };

        assert_eq!(result, Some(7));
        assert_eq!(value, Some(42));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn multi_argument_callbacks_encode_each_argument() {
        fn assert_two_args<'a, T: CFn2<Arg1 = CRef<'a, u32>, Arg2 = u32>>()
        where
            Option<T>: CType,
        {
        }
        assert_two_args::<unsafe extern "C" fn(CRef<'static, u32>, u32) -> u32>();

        let value = 40_u32;
        let callback: unsafe extern "C" fn(_, _) -> _ = add_from_pointer;
        let result: Option<u32> = unsafe { callback.call(&value, 2_u32) };
        assert_eq!(result, Some(42));

        let callback: unsafe extern "C" fn(_, _, _, _, _, _, _, _, _, _, _, _) -> _ = sum_twelve;
        let result: Option<u32> = unsafe {
            callback.call(
                1_u32, 2_u32, 3_u32, 4_u32, 5_u32, 6_u32, 7_u32, 8_u32, 9_u32, 10_u32, 11_u32,
                12_u32,
            )
        };
        assert_eq!(result, Some(78));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn c_callback_crosses_export_and_import() {
        let _: CompanionCallback = sum_pair;
        let _: CompanionPathAlias = sum_pair;
        let _: Option<CallbackBorrowedReturn> = None;
        let _: BorrowedRustValue = borrow_rust_value_raw;
        let _: OwnedTransform = return_owned_raw;
        let result = unsafe { return_owned_raw(co3::encode(Box::new(CallbackValue(42)))) };
        assert_eq!(
            unsafe { co3::decode::<Box<CallbackValue>>(result) },
            Some(Box::new(CallbackValue(42)))
        );
        fn assert_existing_fn_pointer_impls<T>(_: T) {}
        assert_existing_fn_pointer_impls(sum_native_raw as CCallback);
        let raw_value = co3::encode(CallbackRustValue(21));
        let raw_result = unsafe { double_rust_raw(CBorrowed::new(raw_value)) };
        assert_eq!(
            unsafe { co3::decode::<CallbackRustValue>(raw_result) },
            Some(CallbackRustValue(42))
        );
        let encoded = co3::encode(CallbackRustValue(7));
        let raw_result = unsafe {
            CallbackRustValue::scaled_raw(CBorrowed::new(CSoftRef::from_ref(&encoded)), 6)
        };
        assert_eq!(
            unsafe { co3::decode::<CallbackRustValue>(raw_result) },
            Some(CallbackRustValue(42))
        );
        assert_eq!(
            CBorrowed::into_inner(unsafe {
                <CallbackValue as Echo>::echo_raw(CBorrowed::new(CallbackValue(42)))
            }),
            CallbackValue(42)
        );
        assert_eq!(
            imported_apply_callback(sum_pair, CallbackValue(19), CallbackValue(23)),
            CallbackValue(42)
        );
        assert_eq!(
            CBorrowed::into_inner(unsafe {
                imported_apply_callback_raw(
                    CBorrowed::new(Some(sum_pair as CCallback)),
                    CBorrowed::new(CallbackValue(19)),
                    CBorrowed::new(CallbackValue(23)),
                )
            }),
            CallbackValue(42)
        );
        assert_eq!(
            CBorrowed::into_inner(unsafe {
                raw_only_apply_callback(
                    CBorrowed::new(Some(sum_pair as CCallback)),
                    CBorrowed::new(CallbackValue(19)),
                    CBorrowed::new(CallbackValue(23)),
                )
            }),
            CallbackValue(42)
        );
        assert_eq!(
            CBorrowed::into_inner(unsafe {
                CallbackValue::add_raw(
                    CBorrowed::new(CallbackValue(19)),
                    CBorrowed::new(CallbackValue(23)),
                )
            }),
            CallbackValue(42)
        );
        let system_raw: unsafe extern "system" fn(
            CBorrowed<CallbackValue>,
            CBorrowed<CallbackValue>,
        ) -> CBorrowed<CallbackValue> = imported_system_sum_native;
        assert_eq!(
            CBorrowed::into_inner(unsafe {
                system_raw(
                    CBorrowed::new(CallbackValue(19)),
                    CBorrowed::new(CallbackValue(23)),
                )
            }),
            CallbackValue(42)
        );
        assert_eq!(
            imported_sum_native(CallbackValue(19), CallbackValue(23)),
            CallbackValue(42)
        );
        assert_eq!(
            CBorrowed::into_inner(unsafe {
                imported_sum_native_raw(
                    CBorrowed::new(CallbackValue(19)),
                    CBorrowed::new(CallbackValue(23)),
                )
            }),
            CallbackValue(42)
        );
        assert_eq!(
            imported_apply_callback(sum_native_raw, CallbackValue(19), CallbackValue(23)),
            CallbackValue(42)
        );
        assert_eq!(
            imported_apply_optional_callback(Some(sum_pair), CallbackValue(41)),
            CallbackValue(42)
        );
        assert_eq!(
            imported_apply_optional_callback(None, CallbackValue(41)),
            CallbackValue(41)
        );
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn cstr_len(value: &CStr) -> usize {
        value.to_bytes().len()
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn cstr_len(value: &CStr) -> usize;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn raw_cstr_uses_thin_char_pointer() {
        let callback: unsafe extern "C" fn(CBorrowed<&'static CStr>) -> usize = cstr_len_raw;
        assert_eq!(
            unsafe { callback(CBorrowed::new(co3::encode(c"hello"))) },
            5
        );
    }

    #[unsafe(export_name = "abi_cstr__borrowed")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn borrowed_source(value: *const c_char) -> *const c_char {
        value
    }

    #[unsafe(export_name = "abi_cstr__borrowed_native")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn borrowed_native_source(value: *const c_char) -> *const c_char {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "abi_cstr"]

        raw fn borrowed<'a>(value: &'a CStr) -> &'a CStr;
        fn borrowed_native<'a>(value: &'a CStr) -> &'a CStr;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn imported_cstr_uses_thin_char_pointer() {
        let callback: unsafe extern "C" fn(CBorrowed<&'static CStr>) -> CBorrowed<&'static CStr> =
            borrowed;
        let original = c"hello";
        assert_eq!(
            CBorrowed::into_inner(unsafe { callback(CBorrowed::new(co3::encode(original))) })
                .as_ptr()
                .cast::<u8>(),
            original.as_ptr().cast::<u8>()
        );
        assert_eq!(borrowed_native(original), original);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn flip(value: bool) -> bool {
        !value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn next_ascii(value: char) -> char {
        char::from_u32(value as u32 + 1).unwrap()
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn flip(value: bool) -> bool;
        fn next_ascii(value: char) -> char;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn copy_primitives_with_custom_carriers_use_value_conversion() {
        let raw_flip: unsafe extern "C" fn(_) -> _ = flip_raw;
        let raw_next_ascii: unsafe extern "C" fn(_) -> _ = next_ascii_raw;

        let flipped = unsafe { raw_flip(co3::encode(true)) };
        assert_eq!(
            unsafe { co3::decode::<bool>(CBorrowed::into_inner(flipped)) },
            Some(false)
        );
        assert_eq!(
            CBorrowed::into_inner(unsafe { raw_next_ascii(co3::encode('a')) }),
            co3::encode('b')
        );
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    static RAW_ORDER_EVENTS: AtomicUsize = AtomicUsize::new(0);
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn validate_raw_order(value: &u8) -> bool {
        RAW_ORDER_EVENTS
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |previous| {
                Some(previous * 10 + usize::from(*value))
            })
            .unwrap();
        true
    }

    #[derive(RustSpec, ReprC)]
    #[repr(transparent)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value| validate_raw_order(value))]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct RawOrderProbe(u8);

    #[derive(RustSpec, ReprC)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct RawOrderRecord(RawOrderProbe, RawOrderProbe, [u8; 1]);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn raw_order_tuple(values: (RawOrderProbe, RawOrderProbe, RawOrderProbe)) -> u8 {
        values.0.0 + values.1.0 + values.2.0
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn raw_order_record(value: RawOrderRecord) -> u8 {
        value.0.0 + value.1.0 + value.2[0]
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn raw_order_tuple(values: (RawOrderProbe, RawOrderProbe, RawOrderProbe)) -> u8;
        fn raw_order_record(value: RawOrderRecord) -> u8;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn raw_companions_validate_fields_in_declaration_order() {
        assert_eq!(
            unsafe {
                raw_order_tuple_raw(CBorrowed::new(Tuple3(
                    <RawOrderProbe as co3::ReprC>::CType::from_fields(1),
                    <RawOrderProbe as co3::ReprC>::CType::from_fields(2),
                    <RawOrderProbe as co3::ReprC>::CType::from_fields(3),
                )))
            },
            6
        );
        assert_eq!(RAW_ORDER_EVENTS.swap(0, Ordering::SeqCst), 123);

        assert_eq!(
            unsafe {
                raw_order_record_raw(CBorrowed::new(<RawOrderRecord as co3::ReprC>::CType::from_fields(
                    <RawOrderProbe as co3::ReprC>::CType::from_fields(1),
                    <RawOrderProbe as co3::ReprC>::CType::from_fields(2),
                    [3],
                )))
            },
            6
        );
        assert_eq!(RAW_ORDER_EVENTS.swap(0, Ordering::SeqCst), 12);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn generic_identity<T>(value: T) -> T {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn generic_cloned<T: Clone>(value: T) -> T {
        value.clone()
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn generic_const_value<const N: usize>() -> usize {
        N
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn generic_borrowed<'a>(value: &'a u8) -> &'a u8 {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct GenericCallbackHost<T>(core::marker::PhantomData<T>);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl<T: Clone> GenericCallbackHost<T> {
        fn echo(value: T) -> T {
            value
        }

        fn method<U: Clone>(value: U) -> U {
            value.clone()
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        pub fn generic_identity<T>(value: move T) -> move T;
        pub fn generic_cloned<T: Clone>(value: T) -> move T;
        pub fn generic_const_value<const N: usize>() -> usize;
        pub fn generic_borrowed<'a>(value: &'a u8) -> &'a u8;

        impl<T: Clone> GenericCallbackHost<T> {
            pub fn echo(value: move T) -> move T;
            pub fn method<U: Clone>(value: move U) -> move U;
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        type StringRaw = raw extern "C" fn(move String) -> move String;
        type BytesRaw = raw extern "C" fn(move Vec<u8>) -> move Vec<u8>;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn generic_companions_preserve_borrowing() {
        let generic_identity: StringRaw = generic_identity_raw::<String>;
        let value: String = unsafe { generic_identity.call(String::from("hello")) }.unwrap();
        assert_eq!(value, "hello");

        assert_eq!(unsafe { generic_cloned_raw::<u8>(CBorrowed::new(7)) }, 7);
        assert_eq!(unsafe { generic_const_value_raw::<4>() }, 4);
        let byte = 9_u8;
        assert_eq!(
            unsafe {
                *CBorrowed::into_inner(generic_borrowed_raw(CBorrowed::new(CRef::from_ref(&byte))))
                    .as_ptr()
            },
            9
        );

        let echo: BytesRaw = GenericCallbackHost::<Vec<u8>>::echo_raw;
        let value: Vec<u8> = unsafe { echo.call(vec![1_u8, 2, 3]) }.unwrap();
        assert_eq!(value, [1, 2, 3]);

        let method: StringRaw = GenericCallbackHost::<u8>::method_raw::<String>;
        let value: String = unsafe { method.call(String::from("method")) }.unwrap();
        assert_eq!(value, "method");
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn existing(value: u8) -> u8 {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn existing(value: u8) -> u8;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn signature_identity<T>(value: T) -> T {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn signature_identity<T>(value: move T) -> move T;
    }

    #[allow(improper_ctypes_definitions)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    type ArrayCompanion = unsafe extern "C" fn([u8; 2]) -> [u8; 2];

    #[cfg(all(feature = "alloc", feature = "derive"))]
    static_assertions::assert_not_impl_any!(ArrayCompanion: ReprC);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn accept(_callback: for<'a> unsafe extern "C" fn()) {}

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        pub fn accept(callback: for<'a> unsafe extern "C" fn());
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
        value
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn take<'a, T>(_value: &'a T) {}

    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct ArgumentHost;

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl ArgumentHost {
        fn take<'a, T>(_value: &'a T) {}
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        fn take<'a, T>(value: &'a T);

        impl ArgumentHost {
            fn take<'a, T>(value: &'a T);
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct MethodHost;

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl MethodHost {
        fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
            value
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        impl MethodHost {
            fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
        }
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn generic_reference_companions_preserve_borrowing() {
        assert_eq!(unsafe { existing_raw(7) }, 7);
        let _ = accept_raw;
        let _ = borrowed_generic_raw::<u8>;
        let _ = MethodHost::borrowed_generic_raw::<u8>;
        let _: ArrayCompanion = signature_identity_raw::<[u8; 2]>;

        let value = 7_u8;
        unsafe {
            take_raw::<u8>(CBorrowed::new(CRef::from_ref(&value)));
            ArgumentHost::take_raw::<u8>(CBorrowed::new(CRef::from_ref(&value)));
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "system" fn system_source(value: u8) -> u8 {
        value + 1
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "Rust" fn explicit_rust_source(value: u8) -> u8 {
        value + 4
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn rust_source(value: u8) -> u8 {
        value + 2
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn default_source(value: u8) -> u8 {
        value + 3
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn overwrite(bytes: &mut [u8]) -> usize {
        bytes.fill(7);
        bytes.len()
    }

    #[derive(Clone, Copy, RustSpec, ReprC)]
    #[repr_c(identity)]
    #[repr(transparent)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct CompanionValue(u8);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl CompanionValue {
        fn add(self, amount: u8) -> u8 {
            self.0 + amount
        }

        extern "system" fn add_system(self, amount: u8) -> u8 {
            self.0 + amount + 1
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        pub fn default_source(value: u8) -> u8;
        pub fn overwrite(#[unpack(_, _)] bytes: &mut [u8]) -> usize;

        impl CompanionValue {
            pub fn add(self, amount: u8) -> u8;
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    raw! {
        pub fn rust_source(value: u8) -> u8;
        pub extern "system" fn system_source(value: u8) -> u8;
        pub extern "Rust" fn explicit_rust_source(value: u8) -> u8;

        impl CompanionValue {
            pub extern "system" fn add_system(self, amount: u8) -> u8;
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    type OrdinaryAlias<T> = Option<T>;

    #[cfg(all(feature = "alloc", feature = "derive"))]
    co3::ffi! {
        #![unsafe(export("C"))]
        type SystemCallback = raw extern "system" fn(u8) -> u8;
        type UnpackedCallback = raw fn(#[unpack(_, _)] &mut [u8]) -> usize;
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn companions_preserve_borrowing_and_calling_conventions() {
        let c: unsafe extern "system" fn(u8) -> u8 = system_source;
        let system: unsafe extern "C" fn(u8) -> u8 = rust_source_raw;
        let default: unsafe extern "C" fn(u8) -> u8 = default_source_raw;
        let method: unsafe extern "C" fn(CBorrowed<CompanionValue>, u8) -> u8 =
            CompanionValue::add_raw;
        let callback: SystemCallback = system_source;
        let system_companion: unsafe extern "C" fn(u8) -> u8 = system_source_raw;
        let rust_companion: unsafe extern "C" fn(u8) -> u8 = explicit_rust_source_raw;
        let method_companion: unsafe extern "C" fn(CBorrowed<CompanionValue>, u8) -> u8 =
            CompanionValue::add_system_raw;
        let _: OrdinaryAlias<CompanionValue> = Some(CompanionValue(1));
        let _: unsafe extern "system" fn(u8) -> u8 = callback;
        let unpacked: UnpackedCallback = overwrite_raw;
        let mut bytes = [0_u8; 3];
        assert_eq!(unsafe { unpacked(bytes.as_mut_ptr(), bytes.len()) }, 3);
        assert_eq!(bytes, [7; 3]);

        assert_eq!(unsafe { c(1) }, 2);
        assert_eq!(unsafe { system(1) }, 3);
        assert_eq!(unsafe { default(1) }, 4);
        assert_eq!(unsafe { method(CBorrowed::new(CompanionValue(1)), 2) }, 3);
        assert_eq!(unsafe { callback(1) }, 2);
        assert_eq!(unsafe { system_companion(1) }, 2);
        assert_eq!(unsafe { rust_companion(1) }, 5);
        assert_eq!(
            unsafe { method_companion(CBorrowed::new(CompanionValue(1)), 2) },
            4
        );
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        type AliasExplicitAbi = raw extern "system" fn(u8);
        type AliasInheritedAbi = raw fn(u8);
        type AliasOrdinary = core::num::NonZeroU8;
        type AliasRegular = extern "C" fn(u8);
        type AliasSharedUnpacked = extern "C" fn(#[unpack(_, _)] &[u8]) -> usize;
        type AliasInteriorUnpacked = raw extern "C" fn(#[unpack(*mut core::cell::Cell<u8>, usize)] &[core::cell::Cell<u8>]) -> usize;
        type AliasRegularUnpacked = extern "C" fn(#[unpack(_, _)] &mut [u8]) -> usize;
        type AliasComposite = (String, raw fn(String));
        type AliasMaybeCallback = Option<raw fn(u8)>;
        type AliasNicheOk = Result<core::num::NonZeroU8, ()>;
        type AliasNicheErr = Result<(), core::num::NonZeroU8>;
        type AliasCompositeReturn = (u8, raw fn() -> u8);
        type AliasNested = raw extern "C" fn(raw extern "system" fn(u8));
        type AliasReturned = raw fn() -> raw fn(u8);
        type AliasRawNested = raw extern "C" fn(extern "system" fn(u8));
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        fn alias_interior_slice(#[unpack(*mut core::cell::Cell<u8>, usize)] values: &[core::cell::Cell<u8>]) -> usize;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        type AliasBadArg = raw extern "C" fn(move [u8; 2]);
        type AliasBadReturn = raw extern "C" fn() -> move [u8; 2];
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(export("C"))]

        type AliasMoved<T> = raw extern "C" fn(move T) -> move T;
        type AliasBorrowed<T> = raw extern "C" fn(T) -> T;
        type AliasArray<const N: usize> = raw extern "C" fn(move [u8; N]) -> move [u8; N];
        type AliasCombined<T, const N: usize> = raw extern "C" fn(move [T; N]);
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn raw_aliases_preserve_lowered_signatures() {
        let _: Option<AliasExplicitAbi> = None;
        static_assertions::assert_type_eq_all!(AliasExplicitAbi, unsafe extern "system" fn(u8));
        static_assertions::assert_type_eq_all!(AliasInheritedAbi, unsafe extern "C" fn(u8));
        static_assertions::assert_type_eq_all!(AliasOrdinary, co3::num::NonZero<u8>);
        static_assertions::assert_type_eq_all!(AliasRegular, Option<extern "C" fn(u8)>);
        static_assertions::assert_type_eq_all!(
            AliasSharedUnpacked,
            Option<unsafe extern "C" fn(*const u8, usize) -> usize>
        );
        static_assertions::assert_type_eq_all!(
            AliasInteriorUnpacked,
            unsafe extern "C" fn(*mut core::cell::Cell<u8>, usize) -> usize
        );
        static_assertions::assert_type_eq_all!(
            AliasRegularUnpacked,
            Option<unsafe extern "C" fn(*mut u8, usize) -> usize>
        );
        let _: Option<AliasComposite> = None;
        static_assertions::assert_type_eq_all!(
            AliasMaybeCallback,
            co3::option::COptionSome<unsafe extern "C" fn(u8)>
        );
        static_assertions::assert_type_eq_all!(AliasNicheOk, co3::result::CResultOk<core::num::NonZeroU8, ()>);
        static_assertions::assert_type_eq_all!(AliasNicheErr, co3::result::CResultErr<(), core::num::NonZeroU8>);
        static_assertions::assert_type_eq_all!(
            AliasCompositeReturn,
            <(u8, unsafe extern "C" fn() -> u8) as ReprC>::CType
        );
        static_assertions::assert_type_eq_all!(
            AliasNested,
            unsafe extern "C" fn(Option<unsafe extern "system" fn(u8)>)
        );
        static_assertions::assert_type_eq_all!(
            AliasReturned,
            unsafe extern "C" fn() -> CBorrowed<unsafe extern "C" fn(u8)>
        );
        let _: Option<AliasRawNested> = None;

        static_assertions::assert_type_eq_all!(AliasBadArg, unsafe extern "C" fn([u8; 2]));
        static_assertions::assert_type_eq_all!(AliasBadReturn, unsafe extern "C" fn() -> [u8; 2]);
        static_assertions::assert_not_impl_any!(AliasBadArg: ReprC);
        static_assertions::assert_not_impl_any!(AliasBadReturn: ReprC);

        type AliasStringC = <String as ReprC>::CType;
        static_assertions::assert_type_eq_all!(
            AliasMoved<String>,
            unsafe extern "C" fn(AliasStringC) -> AliasStringC
        );
        static_assertions::assert_type_eq_all!(
            AliasBorrowed<u8>,
            unsafe extern "C" fn(CBorrowed<u8>) -> CBorrowed<u8>
        );
        static_assertions::assert_type_eq_all!(
            AliasArray<2>,
            unsafe extern "C" fn([u8; 2]) -> [u8; 2]
        );
        static_assertions::assert_type_eq_all!(AliasCombined<u8, 2>, unsafe extern "C" fn([u8; 2]));
    }

    #[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
    #[repr(u8)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    enum CustomStatus {
        #[repr_c(allow_zero_tag)]
        Ok,
        TrapValue,
        UnknownHandle,
        SoftSyncError,
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl Error for CustomStatus {
        fn trap_value() -> Self {
            Self::TrapValue
        }

        fn unknown_tag() -> Self {
            Self::UnknownHandle
        }

        fn soft_sync_error() -> Self {
            Self::SoftSyncError
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn export_error_input(value: NonZeroU8) -> CustomStatus {
        if value.get() == 11 {
            CustomStatus::UnknownHandle
        } else {
            CustomStatus::Ok
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(export("C"))]
        #![failure = "error"]
        #![symbol_prefix = "kita_failure"]

        fn export_error_input(value: NonZeroU8) -> CustomStatus;
    }

    #[unsafe(export_name = "kita_failure__extern_error_invalid_return")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" fn extern_error_invalid_return() -> <CustomStatus as ReprC>::CType {
        <CustomStatus as co3::ReprC>::CType::from_u8(7)
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]
        #![failure = "error"]
        #![symbol_prefix = "kita_failure"]

        #[symbol_name = "kita_failure__extern_error_invalid_return"]

        pub fn imported_invalid_return() -> CustomStatus;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe extern "C" {
        #[link_name = "kita_failure__export_error_input"]
        fn export_error_input_raw(value: u8) -> <CustomStatus as ReprC>::CType;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn decode_status(source: <CustomStatus as ReprC>::CType) -> CustomStatus {
        unsafe { co3::decode(source) }.expect("status should decode")
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn fieldless_enum_variant_constructors_expose_tags() {
        assert_eq!(
            <CustomStatus as co3::ReprC>::CType::UnknownHandle().tag(),
            CustomStatus::UnknownHandle as u8
        );
        assert_eq!(
            <CustomStatus as co3::ReprC>::CType::SoftSyncError().tag(),
            CustomStatus::SoftSyncError as u8
        );
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn error_failure_mode_returns_status_on_export_decode_failure() {
        let status = unsafe { export_error_input_raw(0) };

        assert_eq!(CustomStatus::trap_value(), decode_status(status));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn error_failure_mode_preserves_export_success_value() {
        let status = unsafe { export_error_input_raw(11) };

        assert_eq!(CustomStatus::UnknownHandle, decode_status(status));
    }

    #[test]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn error_failure_mode_returns_status_on_extern_return_decode_failure() {
        assert_eq!(CustomStatus::trap_value(), imported_invalid_return());
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    static IMPORTED_ORDER_EVENTS: AtomicUsize = AtomicUsize::new(0);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn validate_imported_order(value: &u8) -> bool {
        IMPORTED_ORDER_EVENTS
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |events| {
                Some(events * 10 + usize::from(*value))
            })
            .unwrap();
        true
    }

    #[derive(RustSpec, ReprC)]
    #[repr(transparent)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value| validate_imported_order(value))]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct ImportedOrderProbe(u8);

    #[derive(RustSpec, ReprC)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct ImportedOrderRecord(ImportedOrderProbe, ImportedOrderProbe, [u8; 1]);

    #[unsafe(export_name = "field_order_tuple")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn receive_tuple(
        values: Tuple3<
            <ImportedOrderProbe as crate::ReprC>::CType,
            <ImportedOrderProbe as crate::ReprC>::CType,
            <ImportedOrderProbe as crate::ReprC>::CType,
            false,
        >,
    ) -> u8 {
        values.0.0 + values.1.0 + values.2.0
    }

    #[unsafe(export_name = "field_order_record")]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    extern "C" fn receive_record(value: <ImportedOrderRecord as co3::ReprC>::CType) -> u8 {
        value.0.0 + value.1.0 + value.2[0]
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "field_order_tuple"]
        pub fn imported_order_tuple(values: (ImportedOrderProbe, ImportedOrderProbe, ImportedOrderProbe)) -> u8;

        #[symbol_name = "field_order_record"]
        pub fn imported_order_record(value: ImportedOrderRecord) -> u8;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn assert_events(expected: &[u8]) {
        let expected = expected
            .iter()
            .fold(0, |events, value| events * 10 + usize::from(*value));
        assert_eq!(IMPORTED_ORDER_EVENTS.swap(0, Ordering::SeqCst), expected);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[cfg(all(feature = "alloc", feature = "derive"))]
    fn imported_arguments_validate_fields_in_declaration_order() {
        assert_eq!(
            imported_order_tuple((
                ImportedOrderProbe(1),
                ImportedOrderProbe(2),
                ImportedOrderProbe(3)
            )),
            6
        );
        assert_events(&[1, 2, 3]);

        assert_eq!(
            imported_order_record(ImportedOrderRecord(
                ImportedOrderProbe(1),
                ImportedOrderProbe(2),
                [3]
            )),
            6
        );
        assert_events(&[1, 2]);
    }
}
