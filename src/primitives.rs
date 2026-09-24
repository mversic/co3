//! Logic related to the conversion of primitives to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::cmp::Ordering;

#[cfg(feature = "alloc")]
use crate::stored::Owned;
use crate::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, ReprC, assert_arr_has_non_zero_len,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    niche::Niche,
    stored::{ArrayStore, DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};

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

        impl ExternC for $primitive {
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

        unsafe impl ReprC for $primitive {}
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

        impl<R: ReprC + ?Sized> ExternC for *$mutability R {
            type CType = Self;
        }
        unsafe impl<R: ReprC + ?Sized> EncodeOwned for *$mutability R {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, R: ReprC + ?Sized> DecodeOwned<'d> for *$mutability R {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<R: ReprC + ?Sized> Encode for *$mutability R {}
        impl<R: ReprC + ?Sized> Decode<'_> for *$mutability R {}

        unsafe impl<R: ReprC + ?Sized> CheckedTransmute for *$mutability R {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<R: ReprC + ?Sized> ReprC for *$mutability R {}
        unsafe impl<R: ReprC> CFnArg for *$mutability R {}
        unsafe impl<R: ReprC> CFnReturn for *$mutability R {}

        unsafe impl<R: ReprC + ?Sized> BorrowCast for *$mutability R {
            type AsConst = Self;
        }
        unsafe impl<R: ReprC + ?Sized> BorrowCastMut for *$mutability R {
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
        // A function pointer cannot be null, so use its nullable form at the ABI
        // boundary and reject null before constructing the Rust pointer.
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> Borrow for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            type Borrowed<'itm> = Self where Self: 'itm;
            type Owner = ();
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where Self: 'itm { self }
        }
        impl<'itm, $($arg: CFnArg,)* R: CFnReturn> FromBorrow<'itm> for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            fn from_borrow(source: Self) -> Self { source }
        }
        impl<$($arg: CFnArg,)* R: CFnReturn> ExternC for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            type CType = Option<Self>;
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> EncodeOwned for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            type Store = ();
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where Self: 'itm { Some(self) }
        }
        unsafe impl<'d, $($arg: CFnArg,)* R: CFnReturn> DecodeOwned<'d> for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            type Store = ();
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                source
            }
        }
        impl<$($arg: CFnArg,)* R: CFnReturn> Encode for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {}
        impl<$($arg: CFnArg,)* R: CFnReturn> Decode<'_> for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {}
        impl<$($arg: CFnArg,)* R: CFnReturn> Niche for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            const NICHE_VALUE: Self::CType = None;
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> CheckedTransmute for $fn_type
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            unsafe fn is_valid(target: &Self::CType) -> bool { target.is_some() }
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> ReprC for Option<$fn_type>
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {}
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> CFnArg for Option<$fn_type>
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {}
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> CFnReturn for Option<$fn_type>
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {}
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> BorrowCast for Option<$fn_type>
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            type AsConst = Self;
        }
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> BorrowCastMut for Option<$fn_type>
        where $fn_type: rust_spec::RustSpec<Layout = rust_spec::Stable> {
            type AsMut = Self;
        }
    };
}

macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr ) => {
        fieldless_enum_derive! {
            $src => $dst: {$niche_val}: $validity_fn;
            |source: $dst| -> Option<$src> { source.try_into().ok() }
        }
    };
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr; $decode_fn:expr ) => {
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

        impl ExternC for $src {
            type CType = $dst;
        }
        unsafe impl EncodeOwned for $src {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self as $dst
            }
        }
        unsafe impl<'d> DecodeOwned<'d> for $src {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                let source = unsafe { crate::stored::decode_owned::<$dst>(source)? };
                $decode_fn(source)
            }
        }

        impl Encode for $src {}
        impl Decode<'_> for $src {}

        impl Niche for $src {
            const NICHE_VALUE: Self::CType = $niche_val;
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

raw_pointer_derive! { const }
raw_pointer_derive! { mut }

#[cfg(feature = "alloc")]
impl<R> Owned for [R] {
    type Owned = Vec<R>;
}

unsafe impl<R: ReprC> ReprC for [R] {}

impl<R: ExternC<CType: Sized>> ExternC for [R] {
    type CType = [R::CType];
}
unsafe impl<R: BorrowCast<AsConst: Copy>> BorrowCast for [R] {
    type AsConst = [R::AsConst];
}
unsafe impl<R: BorrowCastMut<AsMut: Copy>> BorrowCastMut for [R] {
    type AsMut = [R::AsMut];
}

impl<R: ExternC<CType: Sized>, const N: usize> ExternC for [R; N] {
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
}

unsafe impl<R: ReprC, const N: usize> ReprC for [R; N] {}

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
    char => u32: {0x110000}:
    |i: &u32| char::from_u32(*i).is_some()
}
fieldless_enum_derive! {
    bool => u8: {2}:
    |i: &u8| *i == 0 || *i == 1
}
fieldless_enum_derive! {
    Ordering => i8: {2}:
    |i: &i8| (-1..=1).contains(i);
    |source| match source {
        -1 => Some(Ordering::Less),
        0 => Some(Ordering::Equal),
        1 => Some(Ordering::Greater),
        _ => None,
    }
}

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
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn function_pointer_conversion() {
        type Callback = extern "C" fn(u8) -> u8;
        extern "C" fn increment(value: u8) -> u8 {
            value + 1
        }

        assert_impl_all!(Callback: ExternC<CType = Option<Callback>>, Encode, Decode<'static>, Niche);
        assert_impl_all!(Option<Callback>: ReprC, CFnArg, Encode, Decode<'static>);

        type SystemCallback = extern "system" fn(u8) -> u8;
        assert_impl_all!(Option<SystemCallback>: CFnArg);

        let encoded = increment as Callback;
        let encoded = encoded.soft_encode(&mut ());
        let decoded = unsafe { Callback::soft_decode(encoded, &mut ()) }.unwrap();
        assert_eq!(decoded(41), 42);
        assert!(unsafe { Callback::soft_decode(None, &mut ()) }.is_none());
        assert!(Option::<Callback>::None.soft_encode(&mut ()).is_none());

        type VoidCallback = extern "C" fn();
        assert_impl_all!(VoidCallback: ExternC, Encode, Decode<'static>);
        assert_impl_all!(unsafe extern "C" fn(u8) -> u8: ExternC, Encode, Decode<'static>);
        assert_not_impl_any!(fn(u8) -> u8: ExternC, Encode, Decode<'static>);
        assert_not_impl_any!(extern "C" fn(bool) -> u8: ExternC, Encode, Decode<'static>);
        assert_not_impl_any!(extern "C" fn((u8,)) -> u8: ExternC, Encode, Decode<'static>);
    }

    #[test]
    fn robust_u8() {
        assert_impl_all!(u8:
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
            ReprC,
        );
        assert_impl_all!(&u8:
            Niche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            Niche<CType = *mut u8>,
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
            ReprC,
        );
        assert_impl_all!(Option<u8>:
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );
    }
}
