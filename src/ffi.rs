//! Conversions for types in [`core::ffi`].

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, ffi::CString};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    ffi::{CStr, c_void},
    mem::ManuallyDrop,
};

use rust_spec::RustSpec;

use crate::primitives::primitive_derive;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

macro_rules! c_alias_carrier {
    ($name:ident, $alias:ty) => {
        #[repr(transparent)]
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, RustSpec)]
        pub struct $name($alias);

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

#[cfg(feature = "alloc")]
use crate::{boxed::CBox, niche::Niche};

/// A nul-terminated value represented across the ABI by a pointer to its data.
///
/// # Safety
///
/// Implementations must identify the same valid nul-terminated data through every method.
pub unsafe trait NulTerminatedBuf {
    /// Data of a nul-terminated pointer.
    type Data;

    /// Returns a raw pointer to the underlying data.
    fn as_ptr(ptr: *const Self) -> *const Self::Data;

    /// Forms a nul-terminated reference from a data pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be non-null and properly aligned. The initialized data through the first nul
    /// must be readable within one allocation, fit in `isize::MAX` bytes, and remain unchanged for
    /// `'a`.
    unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self;

    /// Consumes the `Box`, returning a wrapped `NonNull` pointer.
    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data>;

    /// Constructs a box from a `NonNull` pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must come from `Self::into_non_null`, retain its allocation provenance,
    /// and still own the complete allocation.
    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Data>) -> Box<Self>;
}

unsafe impl<R: NulTerminatedBuf + ?Sized> NulTerminatedBuf for ManuallyDrop<R> {
    type Data = R::Data;

    fn as_ptr(ptr: *const Self) -> *const Self::Data {
        R::as_ptr(ptr as *const R)
    }

    unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self {
        let inner = unsafe { R::from_raw(ptr) };
        unsafe { &*(inner as *const R as *const Self) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        let inner = unsafe { Box::from_raw(Box::into_raw(self) as *mut R) };
        R::into_non_null(inner)
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Data>) -> Box<Self> {
        let inner = unsafe { R::from_non_null(ptr) };
        unsafe { Box::from_raw(Box::into_raw(inner) as *mut Self) }
    }
}

impl ReprC for CStr {
    type CType = c_char;
}

unsafe impl NulTerminatedBuf for CStr {
    type Data = c_char;

    fn as_ptr(ptr: *const Self) -> *const Self::Data {
        ptr.cast()
    }

    unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self {
        unsafe { CStr::from_ptr(ptr.cast()) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        let raw = self.into_c_string().into_raw();
        unsafe { NonNull::new_unchecked(raw.cast()) }
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(ptr: NonNull<Self::Data>) -> Box<Self> {
        unsafe { CString::from_raw(ptr.cast().as_ptr()) }.into_boxed_c_str()
    }
}

#[cfg(feature = "alloc")]
impl ReprC for CString {
    type CType = CBox<c_char>;
}
#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for CString {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_boxed_c_str().soft_encode(&mut ())
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for CString {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        unsafe { Box::<CStr>::soft_decode(source, &mut ()) }.map(Into::into)
    }
}

#[cfg(feature = "alloc")]
impl Encode for CString {}
#[cfg(feature = "alloc")]
impl Decode<'_> for CString {}

#[cfg(feature = "alloc")]
impl Niche for CString {
    const NICHE: Self::CType = CBox::NICHE;
}

unsafe impl Borrow for c_void {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;
    type Owner = ();

    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}
impl<'itm> FromBorrow<'itm> for c_void {
    fn from_borrow(source: Self) -> Self {
        source
    }
}
impl ReprC for c_void {
    type CType = Self;
}
unsafe impl CType for c_void {}
unsafe impl BorrowCast for c_void {
    type AsConst = Self;
}
unsafe impl BorrowCastMut for c_void {
    type AsMut = Self;
}

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
        assert_impl_all!(CStr: NulTerminatedBuf);
        assert_not_impl_any!(UnsafeCell<CStr>: NulTerminatedBuf);
        assert_not_impl_any!(Cell<CStr>: NulTerminatedBuf);
        assert_impl_all!(ManuallyDrop<CStr>: NulTerminatedBuf);
        assert_impl_all!(&ManuallyDrop<CStr>: Encode, Decode<'static>);
        assert_not_impl_any!(&mut CStr: ReprC, Encode, Decode<'static>);
        #[cfg(feature = "alloc")]
        {
            assert_impl_all!(Box<ManuallyDrop<CStr>>: EncodeOwned, crate::stored::DecodeOwned<'static>);
            assert_impl_all!(CString: Encode);
        }
    }

    #[test]
    fn cstr_reference_round_trip() {
        let original = c"hello";
        let encoded = crate::encode(original);
        assert_eq!(encoded.cast(), original.as_ptr());

        let decoded: &CStr = unsafe { crate::decode(encoded) }.unwrap();
        assert_eq!(decoded, original);

        let none: Option<&CStr> = unsafe { crate::decode(crate::encode(None::<&CStr>)) }.unwrap();
        assert!(none.is_none());
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn cstr_box_round_trip() {
        let original = CString::new("owned").unwrap().into_boxed_c_str();
        let ptr = original.as_ptr();
        let encoded = crate::stored::encode_owned(original);
        assert_eq!(encoded.data.cast_const().cast(), ptr);
        let decoded: Box<CStr> = unsafe { crate::stored::decode_owned(encoded) }.unwrap();
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
}
