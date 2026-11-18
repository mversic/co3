//! Logic related to the conversion of primitives to and from FFI-compatible representation
//!
//! Raw pointers map to pointers to the lowered pointee type. Pointer conversions
//! require ABI-compatible pointees and matching metadata classifications; they
//! preserve the pointer representation without inspecting the pointee. A type
//! mapping alone does not make a pointer eligible for encoding or decoding.

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

use rust_spec::RustSpec;

#[cfg(feature = "alloc")]
use crate::convert::Owned;
use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked},
    niche::Niche,
    transmute::AbiCompatible,
};

/// C-compatible carrier for [`bool`].
///
/// Every byte value is valid for this carrier; conversion to `bool` accepts only 0 and 1.
#[derive(Debug, Clone, Copy, Default, RustSpec)]
#[repr(transparent)]
pub struct Bool(pub u8);

/// C-compatible carrier for a Unicode scalar value (`char`).
///
/// Every `u32` value is valid for this carrier; conversion to `char` validates the scalar value.
#[derive(Debug, Clone, Copy, Default, RustSpec)]
#[repr(transparent)]
pub struct UnicodeChar(pub u32);

impl PartialEq for UnicodeChar {
    fn eq(&self, other: &Self) -> bool {
        char::from_u32(self.0).is_some() && self.0 == other.0
    }
}

impl PartialEq for Bool {
    fn eq(&self, other: &Self) -> bool {
        self.0 <= 1 && self.0 == other.0
    }
}

unsafe impl crate::Zeroable for Bool {}
unsafe impl crate::Zeroable for UnicodeChar {}

impl From<char> for UnicodeChar {
    #[inline(always)]
    fn from(value: char) -> Self {
        Self(value.into())
    }
}

impl TryFrom<UnicodeChar> for char {
    type Error = <char as TryFrom<u32>>::Error;

    #[inline(always)]
    fn try_from(value: UnicodeChar) -> Result<Self, Self::Error> {
        char::try_from(value.0)
    }
}

impl From<bool> for Bool {
    #[inline(always)]
    fn from(value: bool) -> Self {
        Self(value.into())
    }
}

impl TryFrom<Bool> for bool {
    type Error = <bool as TryFrom<u8>>::Error;

    #[inline(always)]
    fn try_from(value: Bool) -> Result<Self, Self::Error> {
        bool::try_from(value.0)
    }
}

impl Bool {
    pub const FALSE: Self = Self(0);
    pub const TRUE: Self = Self(1);
}

macro_rules! primitive_derive {
    ( $primitive:ty ) => {
        $crate::reference::impl_direct_mut!([] $primitive);
        crate::__impl_borrowed_identity!([] $primitive);

        impl ReprC for $primitive {
            type CType = Self;
        }
        unsafe impl EncodeMove for $primitive {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl $crate::convert::EncodeMoveUnchecked for $primitive {
            type SoftStore = ();

            #[inline(always)]
            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d> DecodeMove<'d> for $primitive {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl Encode for $primitive {}
        impl $crate::convert::EncodeUnchecked for $primitive {}
        impl Decode<'_> for $primitive {}

        unsafe impl $crate::transmute::AbiCompatibleValidate for $primitive {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }
        unsafe impl AbiCompatible for $primitive {}

        unsafe impl CType for $primitive {}
        unsafe impl CFnArg for $primitive {}
        unsafe impl CFnReturn for $primitive {}

    };
}

macro_rules! raw_pointer_derive {
    ( $mutability:tt ) => {
        $crate::reference::impl_direct_mut!([R: ReprC + ?Sized] *$mutability R);
        crate::__impl_borrowed_identity!([R: ?Sized] *$mutability R);


        impl<R: ReprC + ?Sized> ReprC for *$mutability R {
            type CType = *$mutability R::CType;
        }
        unsafe impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> EncodeMove for *$mutability R where R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                // Matching metadata and ABI-compatible pointees preserve the pointer representation.
                unsafe { core::mem::transmute_copy(&self) }
            }
        }
        unsafe impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> EncodeMoveUnchecked for *$mutability R where R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
                // Matching metadata and ABI-compatible pointees preserve the pointer representation.
                unsafe { core::mem::transmute_copy(&self) }
            }
        }
        unsafe impl<'d, R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> DecodeMove<'d> for *$mutability R where R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(unsafe { core::mem::transmute_copy(&source) })
            }
        }

        impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> Encode for *$mutability R where R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata> {}
        impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> $crate::convert::EncodeUnchecked for *$mutability R where R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata> {}
        impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> Decode<'_> for *$mutability R where R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata> {}


        unsafe impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> $crate::transmute::AbiCompatibleValidate for *$mutability R
        where
            R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata>,
        {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }
unsafe impl<R: AbiCompatible + crate::ptr::PointerMetadata + ?Sized> AbiCompatible for *$mutability R
        where
            R::CType: crate::ptr::PointerMetadata<Metadata = <R as crate::ptr::PointerMetadata>::Metadata>,
        {}

        unsafe impl<R: CType + ?Sized> CType for *$mutability R {}
        unsafe impl<R: CType> CFnArg for *$mutability R {}
        unsafe impl<R: CType> CFnReturn for *$mutability R {}


    };
}

macro_rules! impl_fn_types {
    ([$($(#[$attr:meta])* [($($arg:ident),*)] $fn_type:ty;)*]) => {$(
        $(#[$attr])*
        impl_fn_types!(@impl [$($arg),*] $fn_type);
    )*};
    (@impl [$($arg:ident),*] $fn_type:ty) => {
        crate::__impl_borrowed_identity!([$($arg,)* R] $fn_type);

        impl<$($arg: CFnArg,)* R: CFnReturn> ReprC for $fn_type
        where
            Option<Self>: CType,
        {
            type CType = Option<Self>;
        }
        unsafe impl<$($arg,)* R> EncodeMove for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            type Store = ();

            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl<$($arg,)* R> EncodeMoveUnchecked for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                Some(self)
            }
        }
        unsafe impl<'d, $($arg,)* R> DecodeMove<'d> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            type Store = ();

            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                source
            }

            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                unsafe { source.unwrap_unchecked() }
            }
        }

        impl<$($arg,)* R> Encode for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }
        impl<$($arg,)* R> $crate::convert::EncodeUnchecked for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }
        impl<$($arg,)* R> Decode<'_> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }

        impl<$($arg,)* R> Niche for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            const NICHE: Self::CType = None;

            fn is_niche(value: &Self::CType) -> bool {
                value.is_none()
            }
        }
        unsafe impl<$($arg,)* R> $crate::transmute::AbiCompatibleValidate for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            unsafe fn is_valid(target: &Self::CType) -> bool {
                target.is_some()
            }
        }
unsafe impl<$($arg,)* R> AbiCompatible for $fn_type
where
    Self: ReprC<CType = Option<Self>>,
    Option<Self>: CType,
{
}
    };
}

macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr; $decode_fn:expr; $encode_fn:expr $(; $unchecked_fn:expr)? ) => {
        crate::__impl_borrowed_identity!([] $src);


        impl ReprC for $src {
            type CType = $dst;
        }
        unsafe impl EncodeMove for $src {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl EncodeMoveUnchecked for $src {
            type SoftStore = ();

            #[inline(always)]
            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                $encode_fn(self)
            }
        }
        unsafe impl<'d> DecodeMove<'d> for $src {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                let source = unsafe { crate::convert::decode_move::<$dst>(source)? };
                $decode_fn(source)
            }

            $(unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                ($unchecked_fn)(source)
            })?
        }

        impl Encode for $src {}
        impl $crate::convert::EncodeUnchecked for $src {}
        impl Decode<'_> for $src {}


        unsafe impl $crate::transmute::AbiCompatibleValidate for $src {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                $validity_fn(target)
            }
        }
unsafe impl AbiCompatible for $src {}

        impl Niche for $src {
            const NICHE: Self::CType = $niche_val;

            fn is_niche(value: &Self::CType) -> bool {
                value.0 == ($niche_val).0
            }
        }
    };
}

primitive_derive! { usize }
primitive_derive! { isize }
primitive_derive! { u8 }
primitive_derive! { i8 }
primitive_derive! { u16 }
primitive_derive! { i16 }
primitive_derive! { u32 }
primitive_derive! { i32 }
primitive_derive! { u64 }
primitive_derive! { i64 }
primitive_derive! { u128 }
primitive_derive! { i128 }
primitive_derive! { f32 }
primitive_derive! { f64 }
primitive_derive! { Bool }
primitive_derive! { UnicodeChar }

raw_pointer_derive! { const }
raw_pointer_derive! { mut }

#[cfg(feature = "alloc")]
impl<R> Owned for [R] {
    type Owned = Vec<R>;
}
unsafe impl<R: CType> CType for [R] {}

impl<R: ReprC<CType: Sized>> ReprC for [R] {
    type CType = [R::CType];
}
impl<R: ReprC<CType: Sized>, const N: usize> ReprC for [R; N] {
    type CType = [R::CType; N];
}

unsafe impl<R: CType, const N: usize> CType for [R; N] {}

unsafe impl<T: EmptyStore, const N: usize> EmptyStore for [T; N] {}
// TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
//unsafe impl<T> EmptyStore for [T; 0] {}

crate::fn_types::with_fn_types!(impl_fn_types, {});

fieldless_enum_derive! {
    char => UnicodeChar: {UnicodeChar(0x110000)}:
    |i: &UnicodeChar| char::try_from(*i).is_ok();
    |source: UnicodeChar| char::try_from(source).ok();
    |source: char| UnicodeChar::from(source);
    |source: UnicodeChar| unsafe { char::try_from(source).unwrap_unchecked() }
}

fieldless_enum_derive! {
    bool => Bool: {Bool(2)}:
    |i: &Bool| i.0 == 0 || i.0 == 1;
    |source: Bool| match source.0 {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    |source: bool| Bool(source as u8);
    |source: Bool| unsafe { core::mem::transmute::<Bool, bool>(source) }
}
pub(crate) use fieldless_enum_derive;
pub(crate) use primitive_derive;

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec::Vec};
    use core::cmp::Ordering;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::{boxed::CBox, wide::CWideBox};
    use crate::{cmp::Ordering as COrdering, convert::Encode, option::COption, reference::CRef};

    #[test]
    fn scalar_carriers_accept_all_values_without_niches() {
        assert_impl_all!(Bool: CType, ReprC<CType = Bool>);
        assert_impl_all!(COrdering: CType, ReprC<CType = COrdering>);
        assert_impl_all!(UnicodeChar: CType, ReprC<CType = UnicodeChar>);
        assert_not_impl_any!(UnicodeChar: Eq, Ord);
        assert_not_impl_any!(Bool: Niche);
        assert_not_impl_any!(COrdering: Niche);
        assert_not_impl_any!(UnicodeChar: Niche);
        assert_impl_all!(Option<Bool>: ReprC<CType = COption<Bool>>);
        assert_impl_all!(Option<COrdering>: ReprC<CType = COption<COrdering>>);
        assert_impl_all!(Option<UnicodeChar>: ReprC<CType = COption<UnicodeChar>>);

        for raw in u8::MIN..=u8::MAX {
            let boolean = Bool(raw);
            assert_eq!(boolean.0, raw);
            assert_eq!(boolean == boolean, raw <= 1);
            assert_eq!(
                unsafe { crate::decode::<Bool>(boolean) }.map(|v| v.0),
                Some(raw)
            );
            assert_eq!(
                unsafe { crate::decode::<bool>(boolean) }.is_some(),
                raw <= 1
            );
            let ordering = COrdering(raw as i8);
            assert_eq!(ordering.0, raw as i8);
            assert_eq!(
                unsafe { crate::decode::<COrdering>(ordering) }.map(|v| v.0),
                Some(ordering.0)
            );
            assert_eq!(
                unsafe { crate::decode::<Ordering>(ordering) }.is_some(),
                (-1..=1).contains(&ordering.0)
            );
        }

        for raw in [0, 0xd800, 0x10ffff, 0x110000, u32::MAX] {
            let scalar = UnicodeChar(raw);
            assert_eq!(scalar.0, raw);
            assert_eq!(scalar == scalar, char::from_u32(raw).is_some());
            assert_eq!(
                unsafe { crate::decode::<UnicodeChar>(scalar) }.map(|value| value.0),
                Some(raw)
            );
            assert_eq!(
                unsafe { crate::decode::<char>(scalar) },
                char::from_u32(raw)
            );
        }
        for value in [Ordering::Less, Ordering::Equal, Ordering::Greater] {
            assert_eq!(
                unsafe { crate::decode_unchecked::<Ordering>(crate::encode(value)) },
                value
            );
        }
        assert!(bool::is_niche(&crate::encode(None::<bool>).0));
        assert!(Ordering::is_niche(&crate::encode(None::<Ordering>).0));
        assert!(char::is_niche(&crate::encode(None::<char>).0));
    }

    #[test]
    fn function_pointer_conversion() {
        type Callback = extern "C" fn(u8) -> u8;
        extern "C" fn increment(value: u8) -> u8 {
            value + 1
        }

        assert_impl_all!(Callback: ReprC<CType = Option<Callback>>, Encode, Decode<'static>, Niche);
        assert_impl_all!(Option<Callback>: CType, CFnArg, Encode, Decode<'static>);

        type SystemCallback = extern "system" fn(u8) -> u8;
        assert_impl_all!(Option<SystemCallback>: CFnArg);

        let encoded = increment as Callback;
        let encoded = crate::encode(encoded);
        let decoded = unsafe { crate::decode::<Callback>(encoded) }.unwrap();
        assert_eq!(decoded(41), 42);
        assert!(unsafe { crate::decode::<Callback>(None) }.is_none());
        assert!(
            crate::soft_encode(Option::<Callback>::None, &mut None)
                .0
                .is_none()
        );

        type VoidCallback = extern "C" fn();
        assert_impl_all!(VoidCallback: ReprC, Encode, Decode<'static>);
        assert_impl_all!(unsafe extern "C" fn(u8) -> u8: ReprC, Encode, Decode<'static>);
        assert_not_impl_any!(fn(u8) -> u8: ReprC, Encode, Decode<'static>);
        assert_not_impl_any!(unsafe fn(bool) -> u8: ReprC);

        type BoolCallback = extern "C" fn(bool) -> u8;
        assert_not_impl_any!(BoolCallback: ReprC, Encode, Decode<'static>);

        #[allow(improper_ctypes_definitions)]
        type TupleCallback = extern "C" fn((u8,)) -> u8;
        assert_not_impl_any!(TupleCallback: ReprC, Encode, Decode<'static>);

        #[allow(improper_ctypes_definitions)]
        type SystemCallbackWithRustTypes = unsafe extern "system" fn(bool) -> (u8,);
        assert_not_impl_any!(SystemCallbackWithRustTypes: ReprC, Encode, Decode<'static>);

        #[cfg(feature = "alloc")]
        {
            assert_not_impl_any!(extern "C" fn(Box<u8>) -> (): ReprC);
            assert_not_impl_any!(extern "C" fn() -> Box<u8>: ReprC);
            assert_not_impl_any!(fn(Box<u8>) -> u8: ReprC);
            type OwnedCallback = extern "C" fn(CBox<u8>) -> CBox<u8>;
            assert_impl_all!(OwnedCallback: ReprC<CType = Option<OwnedCallback>>, Encode, Decode<'static>);
        }
    }

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ReprC<CType = u8>,
            Decode<'static>,
            Encode,
            CType,
        );
        assert_impl_all!(&u8:
            Niche<CType = CRef<'static, u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            Niche<CType = crate::reference::CRefMut<'static, u8, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<u8>:
            Niche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[u8]:
            Niche<CType = crate::wide::CWideRef<'static, [u8]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [u8]:
            Niche<CType = crate::wide::CWideRef<'static, [u8]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            Niche<CType = CWideBox<[u8]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            Niche<CType = crate::vec::CVec<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([u8; 2]:
            Decode<'static>,
            Encode,
            CType,
        );
        assert_impl_all!(Option<u8>:
            Niche<CType = COption<u8>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn char_has_distinct_scalar_carrier() {
        static_assertions::assert_impl_all!(char: ReprC<CType = UnicodeChar>);
        static_assertions::assert_impl_all!(UnicodeChar: From<char>);
        static_assertions::assert_not_impl_any!(UnicodeChar: From<u32>, TryFrom<u32>);
        assert_eq!(UnicodeChar('🍃' as u32), crate::encode('🍃'));
        assert_eq!(crate::encode('🍃').0, '🍃' as u32);
        assert_eq!(
            unsafe { crate::decode::<char>(UnicodeChar(0x110000)) },
            None
        );
        assert_eq!(unsafe { crate::decode::<char>(UnicodeChar(0xd800)) }, None);
    }
}
