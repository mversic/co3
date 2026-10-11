//! Logic related to the conversion of primitives to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::cmp::Ordering;

use rust_spec::RustSpec;

#[cfg(feature = "alloc")]
use crate::stored::Owned;
use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, assert_arr_has_non_zero_len,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    niche::Niche,
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    sync::ArrayStore,
    transmute::CheckedTransmute,
};

/// C-compatible carrier for [`bool`].
///
/// Every byte value is valid for this carrier; conversion to `bool` accepts only 0 and 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, RustSpec)]
#[repr(transparent)]
pub struct CBool(pub u8);

/// C-compatible carrier for [`Ordering`].
///
/// Every byte value is valid for this carrier; conversion to `Ordering` accepts only -1, 0, and 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, RustSpec)]
#[repr(transparent)]
pub struct COrdering(pub i8);

/// C-compatible carrier for a Unicode scalar value (`char`).
///
/// Every `u32` value is valid for this carrier; conversion to `char` validates the scalar value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, RustSpec)]
#[repr(transparent)]
pub struct UnicodeChar(pub u32);

impl CBool {
    pub const FALSE: Self = Self(0);
    pub const TRUE: Self = Self(1);
}

impl COrdering {
    pub const LESS: Self = Self(-1);
    pub const EQUAL: Self = Self(0);
    pub const GREATER: Self = Self(1);
}

macro_rules! primitive_derive {
    ( $primitive:ty ) => {
        unsafe impl Borrow for $primitive {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'itm> FromBorrow<'itm> for $primitive {
            #[inline(always)]
            fn from_borrow(source: Self) -> Self {
                source
            }
        }

        impl ReprC for $primitive {
            type CType = Self;
        }
        unsafe impl EncodeOwned for $primitive {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d> DecodeOwned<'d> for $primitive {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl Encode for $primitive {}
        impl Decode<'_> for $primitive {}

        unsafe impl CheckedTransmute for $primitive {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl CType for $primitive {}
        unsafe impl CFnArg for $primitive {}
        unsafe impl CFnReturn for $primitive {}

        unsafe impl BorrowCast for $primitive {
            type AsConst = Self;
        }
        unsafe impl BorrowCastMut for $primitive {
            type AsMut = Self;
        }
    };
}

macro_rules! raw_pointer_derive {
    ( $mutability:tt ) => {
        unsafe impl<R: ?Sized> Borrow for *$mutability R {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'itm, R: ?Sized> FromBorrow<'itm> for *$mutability R {
            #[inline(always)]
            fn from_borrow(source: Self) -> Self {
                source
            }
        }

        impl<R: CType + ?Sized> ReprC for *$mutability R {
            type CType = Self;
        }
        unsafe impl<R: CType + ?Sized> EncodeOwned for *$mutability R {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, R: CType + ?Sized> DecodeOwned<'d> for *$mutability R {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<R: CType + ?Sized> Encode for *$mutability R {}
        impl<R: CType + ?Sized> Decode<'_> for *$mutability R {}

        unsafe impl<R: CType + ?Sized> CheckedTransmute for *$mutability R
        where
            R: RustSpec<Size: crate::Thin>,
        {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<R: CType + ?Sized> CType for *$mutability R {}
        unsafe impl<R: CType> CFnArg for *$mutability R {}
        unsafe impl<R: CType> CFnReturn for *$mutability R {}

        unsafe impl<R: CType + ?Sized> BorrowCast for *$mutability R {
            type AsConst = Self;
        }
        unsafe impl<R: CType + ?Sized> BorrowCastMut for *$mutability R {
            type AsMut = Self;
        }

    };
}

macro_rules! impl_fn_types {
    ( $( ( $( $arg:ident ),* ) ),* $(,)? ) => {
        $(
            impl_fn_types!(@abi "C"; $($arg),*);
            impl_fn_types!(@abi "C-unwind"; $($arg),*);
            impl_fn_types!(@abi "system"; $($arg),*);
            impl_fn_types!(@abi "system-unwind"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "cdecl"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "cdecl-unwind"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "stdcall"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "stdcall-unwind"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "fastcall"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "fastcall-unwind"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "thiscall"; $($arg),*);
            #[cfg(target_arch = "x86")]
            impl_fn_types!(@abi "thiscall-unwind"; $($arg),*);
            #[cfg(target_arch = "x86_64")]
            impl_fn_types!(@abi "sysv64"; $($arg),*);
            #[cfg(target_arch = "x86_64")]
            impl_fn_types!(@abi "sysv64-unwind"; $($arg),*);
            #[cfg(target_arch = "x86_64")]
            impl_fn_types!(@abi "win64"; $($arg),*);
            #[cfg(target_arch = "x86_64")]
            impl_fn_types!(@abi "win64-unwind"; $($arg),*);
            #[cfg(target_arch = "arm")]
            impl_fn_types!(@abi "aapcs"; $($arg),*);
            #[cfg(target_arch = "arm")]
            impl_fn_types!(@abi "aapcs-unwind"; $($arg),*);
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64"))]
            impl_fn_types!(@abi "efiapi"; $($arg),*);
        )*
    };
    (@abi $abi:literal; $($arg:ident),*) => {
        impl_fn_types!(@impl [$($arg),*] extern $abi fn($($arg),*) -> R);
        impl_fn_types!(@impl [$($arg),*] unsafe extern $abi fn($($arg),*) -> R);
    };
    (@impl [$($arg:ident),*] $fn_type:ty) => {
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> Borrow for $fn_type
        where Option<Self>: CType {
            type Borrowed<'itm> = Self where Self: 'itm;
            type Owner = ();
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where Self: 'itm { self }
        }
        impl<'itm, $($arg: CFnArg,)* R: CFnReturn> FromBorrow<'itm> for $fn_type
        where Option<Self>: CType {
            fn from_borrow(source: Self) -> Self { source }
        }
        impl<$($arg: CFnArg,)* R: CFnReturn> ReprC for $fn_type
        where Option<Self>: CType {
            type CType = Option<Self>;
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> EncodeOwned for $fn_type
        where Option<Self>: CType {
            type Store = ();
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where Self: 'itm { Some(self) }
        }
        unsafe impl<'d, $($arg: CFnArg,)* R: CFnReturn> DecodeOwned<'d> for $fn_type
        where Option<Self>: CType {
            type Store = ();
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                source
            }

            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                unsafe { source.unwrap_unchecked() }
            }
        }
        impl<$($arg: CFnArg,)* R: CFnReturn> Encode for $fn_type
        where Option<Self>: CType {}
        impl<$($arg: CFnArg,)* R: CFnReturn> Decode<'_> for $fn_type
        where Option<Self>: CType {}
        impl<$($arg: CFnArg,)* R: CFnReturn> Niche for $fn_type
        where Option<Self>: CType {
            const NICHE: Self::CType = None;
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> CheckedTransmute for $fn_type
        where Option<Self>: CType {
            unsafe fn is_valid(target: &Self::CType) -> bool { target.is_some() }
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> CFnArg for Option<$fn_type>
        where Self: CType {}
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> CFnReturn for Option<$fn_type>
        where Self: CType {}
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> BorrowCast for Option<$fn_type>
        where Self: CType {
            type AsConst = Self;
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> BorrowCastMut for Option<$fn_type>
        where Self: CType {
            type AsMut = Self;
        }
    };
}

macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr; $decode_fn:expr; $encode_fn:expr $(; $unchecked_fn:expr)? ) => {
        unsafe impl CheckedTransmute for $src {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                $validity_fn(target)
            }
        }

        unsafe impl Borrow for $src {
            type Borrowed<'itm> = Self;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm> {
                self
            }
        }
        impl<'itm> FromBorrow<'itm> for $src {
            #[inline(always)]
            fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
                source
            }
        }

        impl ReprC for $src {
            type CType = $dst;
        }
        unsafe impl EncodeOwned for $src {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                $encode_fn(self)
            }
        }
        unsafe impl<'d> DecodeOwned<'d> for $src {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                let source = unsafe { crate::stored::decode_owned::<$dst>(source)? };
                $decode_fn(source)
            }

            $(unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                ($unchecked_fn)(source)
            })?
        }

        impl Encode for $src {}
        impl Decode<'_> for $src {}

        impl Niche for $src {
            const NICHE: Self::CType = $niche_val;
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
primitive_derive! { CBool }
primitive_derive! { COrdering }
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
unsafe impl<R: BorrowCast<AsConst: Copy>> BorrowCast for [R] {
    type AsConst = [R::AsConst];
}
unsafe impl<R: BorrowCastMut<AsMut: Copy>> BorrowCastMut for [R] {
    type AsMut = [R::AsMut];
}

impl<R: ReprC<CType: Sized>, const N: usize> ReprC for [R; N] {
    type CType = [R::CType; N];
}
unsafe impl<R: EncodeOwned<CType: Copy>, const N: usize> EncodeOwned for [R; N] {
    type Store = ArrayStore<R::Store, N>;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        assert_arr_has_non_zero_len::<N>();

        let store = &mut store.0;

        let mut items = self.into_iter();
        let mut stores = store.iter_mut();

        core::array::from_fn(|_| {
            let item = items.next().unwrap();
            let store = stores.next().unwrap();

            item.soft_encode(store)
        })
    }
}
unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, const N: usize> DecodeOwned<'d> for [R; N] {
    type Store = ArrayStore<R::Store, N>;

    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        assert_arr_has_non_zero_len::<N>();

        let mut stores = store.0.iter_mut();
        let decoded = source.map(|item| unsafe { R::soft_decode(item, stores.next().unwrap()) });

        if decoded.iter().any(Option::is_none) {
            return None;
        }

        Some(decoded.map(|item| unsafe { item.unwrap_unchecked() }))
    }

    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        assert_arr_has_non_zero_len::<N>();

        let mut stores = store.0.iter_mut();
        source.map(|item| unsafe { R::soft_decode_unchecked(item, stores.next().unwrap()) })
    }
}

unsafe impl<R: CType, const N: usize> CType for [R; N] {}

unsafe impl<R: BorrowCast<AsConst: Copy> + Copy, const N: usize> BorrowCast for [R; N] {
    type AsConst = [R::AsConst; N];
}
unsafe impl<R: BorrowCastMut<AsMut: Copy> + Copy, const N: usize> BorrowCastMut for [R; N] {
    type AsMut = [R::AsMut; N];
}

unsafe impl<T: EmptyStore, const N: usize> EmptyStore for [T; N] {}
// TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
//unsafe impl<T> EmptyStore for [T; 0] {}

impl_fn_types! {
    (),
    (A),
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H),
    (A, B, C, D, E, F, G, H, I),
    (A, B, C, D, E, F, G, H, I, J),
    (A, B, C, D, E, F, G, H, I, J, K),
    (A, B, C, D, E, F, G, H, I, J, K, L),
}

fieldless_enum_derive! {
    char => UnicodeChar: {UnicodeChar(0x110000)}:
    |i: &UnicodeChar| char::from_u32(i.0).is_some();
    |source: UnicodeChar| char::from_u32(source.0);
    |source: char| UnicodeChar(source as u32);
    |source: UnicodeChar| unsafe { char::from_u32_unchecked(source.0) }
}

#[cfg(test)]
mod unicode_char_tests {
    use super::*;

    #[test]
    fn char_has_distinct_scalar_carrier() {
        static_assertions::assert_impl_all!(char: ReprC<CType = UnicodeChar>);
        static_assertions::assert_not_impl_any!(UnicodeChar: From<char>, From<u32>, TryFrom<u32>);
        assert_eq!(UnicodeChar('🍃' as u32), crate::encode('🍃'));
        assert_eq!(crate::encode('🍃').0, '🍃' as u32);
        assert_eq!(
            unsafe { crate::decode::<char>(UnicodeChar(0x110000)) },
            None
        );
        assert_eq!(unsafe { crate::decode::<char>(UnicodeChar(0xd800)) }, None);
    }
}
fieldless_enum_derive! {
    bool => CBool: {CBool(2)}:
    |i: &CBool| i.0 == 0 || i.0 == 1;
    |source: CBool| match source.0 {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    |source: bool| CBool(source as u8);
    |source: CBool| unsafe { core::mem::transmute::<CBool, bool>(source) }
}
fieldless_enum_derive! {
    Ordering => COrdering: {COrdering(2)}:
    |i: &COrdering| (-1..=1).contains(&i.0);
    |source: COrdering| match source.0 {
        -1 => Some(Ordering::Less),
        0 => Some(Ordering::Equal),
        1 => Some(Ordering::Greater),
        _ => None,
    };
    |source: Ordering| COrdering(source as i8);
    |source: COrdering| unsafe { core::mem::transmute::<COrdering, Ordering>(source) }
}

pub(crate) use primitive_derive;

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec::Vec};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxedSlice};
    use crate::{
        Encode,
        option::ReprCOption,
        reference::CRefMut,
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn scalar_carriers_accept_all_values_without_niches() {
        assert_impl_all!(CBool: CType, ReprC<CType = CBool>);
        assert_impl_all!(COrdering: CType, ReprC<CType = COrdering>);
        assert_impl_all!(UnicodeChar: CType, ReprC<CType = UnicodeChar>);
        assert_not_impl_any!(CBool: Niche);
        assert_not_impl_any!(COrdering: Niche);
        assert_not_impl_any!(UnicodeChar: Niche);
        assert_impl_all!(Option<CBool>: ReprC<CType = ReprCOption<CBool>>);
        assert_impl_all!(Option<COrdering>: ReprC<CType = ReprCOption<COrdering>>);
        assert_impl_all!(Option<UnicodeChar>: ReprC<CType = ReprCOption<UnicodeChar>>);

        for raw in u8::MIN..=u8::MAX {
            let boolean = CBool(raw);
            assert_eq!(boolean.0, raw);
            assert_eq!(unsafe { crate::decode::<CBool>(boolean) }, Some(boolean));
            assert_eq!(
                unsafe { crate::decode::<bool>(boolean) }.is_some(),
                raw <= 1
            );
            let ordering = COrdering(raw as i8);
            assert_eq!(ordering.0, raw as i8);
            assert_eq!(
                unsafe { crate::decode::<COrdering>(ordering) },
                Some(ordering)
            );
            assert_eq!(
                unsafe { crate::decode::<Ordering>(ordering) }.is_some(),
                (-1..=1).contains(&ordering.0)
            );
        }

        for raw in [0, 0xd800, 0x10ffff, 0x110000, u32::MAX] {
            let scalar = UnicodeChar(raw);
            assert_eq!(scalar.0, raw);
            assert_eq!(
                unsafe { crate::decode::<UnicodeChar>(scalar) },
                Some(scalar)
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
        assert_eq!(crate::encode(None::<bool>), bool::NICHE);
        assert_eq!(crate::encode(None::<Ordering>), Ordering::NICHE);
        assert_eq!(crate::encode(None::<char>), char::NICHE);
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
        let encoded = encoded.soft_encode(&mut ());
        let decoded = unsafe { Callback::soft_decode(encoded, &mut ()) }.unwrap();
        assert_eq!(decoded(41), 42);
        assert!(unsafe { Callback::soft_decode(None, &mut ()) }.is_none());
        assert!(Option::<Callback>::None.soft_encode(&mut ()).is_none());

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
            Niche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            Niche<CType = CRefMut<u8>>,
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
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [u8]:
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([u8; 2]:
            Decode<'static>,
            Encode,
            CType,
        );
        assert_impl_all!(Option<u8>:
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );
    }
}
