//! Logic related to the conversion of primitives to and from FFI-compatible representation
//!
//! Raw pointers map to pointers to the lowered pointee type. Pointer conversions
//! require ABI-compatible pointees and matching metadata classifications; they
//! preserve the pointer representation without inspecting the pointee. A type
//! mapping alone does not make a pointer eligible for encoding or decoding.

#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};

#[cfg(feature = "alloc")]
use crate::convert::Owned;
use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{
        AbiValidate, Decode, DecodeMove, DecodeMoveUnchecked, DecodeUnchecked, EmptyStore, Encode,
        EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
    },
    lang::Language,
    marker::AbiCompatible,
    niche::Niche,
};

crate::borrow::__impl_borrowed_identity!([]());

impl ReprC for () {
    type CType = Self;
}
unsafe impl<L: Language> EncodeMove<L> for () {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, &mut ()) }
    }
}

unsafe impl EncodeMoveUnchecked for () {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
unsafe impl<'d> DecodeMoveUnchecked<'d> for () {
    type SoftStore = ();

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
        source
    }
}
unsafe impl<'d, L: Language> DecodeMove<'d, L> for () {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl EncodeUnchecked for () {}
impl<L: Language> Encode<L> for () {}
impl DecodeUnchecked<'_> for () {}
impl<L: Language> Decode<'_, L> for () {}

unsafe impl AbiCompatible for () {}
unsafe impl<L: Language> AbiValidate<L> for () {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl CType for () {}
crate::mutable::impl_direct_mut!([]());
unsafe impl CFnReturn for () {}

unsafe impl EmptyStore for () {}

#[cfg(feature = "alloc")]
impl Owned for str {
    type Owned = String;
}
impl ReprC for str {
    type CType = crate::primitive::abi::str;
}

impl TryFrom<abi::char> for char {
    type Error = <char as TryFrom<u32>>::Error;
    fn try_from(value: abi::char) -> Result<Self, Self::Error> {
        char::try_from(value.0)
    }
}

impl TryFrom<abi::bool> for bool {
    type Error = <bool as TryFrom<u8>>::Error;
    fn try_from(value: abi::bool) -> Result<Self, Self::Error> {
        bool::try_from(value.0)
    }
}

macro_rules! primitive_derive {
    ( $primitive:ty ) => {
        $crate::mutable::impl_direct_mut!([] $primitive);
        crate::borrow::__impl_borrowed_identity!([] $primitive);

        impl ReprC for $primitive {
            type CType = Self;
        }
        unsafe impl <L: Language> EncodeMove<L> for $primitive {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, &mut ()) }
            }
        }
        unsafe impl  $crate::convert::EncodeMoveUnchecked for $primitive {
            type SoftStore = ();

            #[inline(always)]
            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, > DecodeMoveUnchecked<'d> for $primitive {
            type SoftStore = ();

            #[inline(always)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self { {
                source
            } }
        }
        unsafe impl<'d, L: Language> DecodeMove<'d, L> for $primitive {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl $crate::convert::EncodeUnchecked for $primitive {}
        impl <L: Language> Encode<L> for $primitive {}
        impl<> DecodeUnchecked<'_> for $primitive {}
        impl<L: Language> Decode<'_, L> for $primitive {}

        unsafe impl AbiCompatible for $primitive {}
        unsafe impl<L: $crate::lang::Language> $crate::convert::AbiValidate<L> for $primitive {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> core::primitive::bool {
                true
            }
        }

        unsafe impl CType for $primitive {}
        unsafe impl CFnArg for $primitive {}
        unsafe impl CFnReturn for $primitive {}

    };
}

macro_rules! raw_pointer_derive {
    ( $mutability:tt ) => {
        $crate::mutable::impl_direct_mut!([R: ReprC + ?Sized] *$mutability R);
        crate::borrow::__impl_borrowed_identity!([R: ?Sized] *$mutability R);

        impl<R: ReprC + ?Sized> ReprC for *$mutability R {
            type CType = *$mutability R::CType;
        }
        unsafe impl<L: Language, R: AbiCompatible + ?Sized> EncodeMove<L> for *$mutability R {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                // AbiCompatible guarantees compatible pointee and pointer representations.
                unsafe { core::mem::transmute_copy(&self) }
            }
        }
        unsafe impl<R: AbiCompatible + ?Sized> EncodeMoveUnchecked for *$mutability R {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
                // AbiCompatible guarantees compatible pointee and pointer representations.
                unsafe { core::mem::transmute_copy(&self) }
            }
        }
        unsafe impl<'d,  R: AbiCompatible + ?Sized> DecodeMoveUnchecked<'d> for *$mutability R {
            type SoftStore = ();

            #[inline(always)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self { {
                unsafe { core::mem::transmute_copy(&source) }
            } }
        }
        unsafe impl<'d, L: Language, R: AbiCompatible + ?Sized> DecodeMove<'d, L> for *$mutability R {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(unsafe { core::mem::transmute_copy(&source) })
            }
        }

        impl<R: AbiCompatible + ?Sized> EncodeUnchecked for *$mutability R {}
        impl<L: Language, R: AbiCompatible + ?Sized> Encode<L> for *$mutability R {}
        impl< R: AbiCompatible + ?Sized> DecodeUnchecked<'_> for *$mutability R {}
        impl<L: Language, R: AbiCompatible + ?Sized> Decode<'_, L> for *$mutability R {}

        unsafe impl<R: AbiCompatible + ?Sized> AbiCompatible for *$mutability R {}
        unsafe impl<L: $crate::lang::Language, R: AbiCompatible + ?Sized> $crate::convert::AbiValidate<L> for *$mutability R {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<R: CType + ?Sized> CType for *$mutability R {}
        unsafe impl<R: CType> CFnArg for *$mutability R {}
        unsafe impl<R: CType> CFnReturn for *$mutability R {}

    };
}

// Enumerate supported function pointer ABIs and arities before codec generation.
macro_rules! with_fn_types {
    ($callback:path, { $($context:tt)* }) => {
        $crate::primitive::with_fn_types!(@collect $callback, { $($context)* }, [];
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
            (A, B, C, D, E, F, G, H, I, J, K, M),
        );
    };
    (@collect $callback:path, { $($context:tt)* }, [$($types:tt)*]; ($($arg:ident),*), $($rest:tt)*) => {
        $crate::primitive::with_fn_types!(@collect $callback, { $($context)* }, [
            $($types)*
            [($($arg),*)] extern "C" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "C" fn($($arg),*) -> R;
            [($($arg),*)] extern "C-unwind" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "C-unwind" fn($($arg),*) -> R;
            [($($arg),*)] extern "system" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "system" fn($($arg),*) -> R;
            [($($arg),*)] extern "system-unwind" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "system-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "cdecl" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "cdecl" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "cdecl-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "cdecl-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "stdcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "stdcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "stdcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "stdcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "fastcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "fastcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "fastcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "fastcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "thiscall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "thiscall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "thiscall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "thiscall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "sysv64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "sysv64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "sysv64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "sysv64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "win64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "win64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "win64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "win64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] extern "aapcs" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] unsafe extern "aapcs" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] extern "aapcs-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] unsafe extern "aapcs-unwind" fn($($arg),*) -> R;
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64"))]
            [($($arg),*)] extern "efiapi" fn($($arg),*) -> R;
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64"))]
            [($($arg),*)] unsafe extern "efiapi" fn($($arg),*) -> R;
            ]; $($rest)*);
    };
    (@collect $callback:path, { $($context:tt)* }, [$($types:tt)*];) => {
        $callback! { $($context)* [$($types)*] }
    };
}
pub(crate) use with_fn_types;

macro_rules! impl_fn_types {
    ([$($(#[$attr:meta])* [($($arg:ident),*)] $fn_type:ty;)*]) => {$(
        $(#[$attr])*
        impl_fn_types!(@impl [$($arg),*] $fn_type);
    )*};
    (@impl [$($arg:ident),*] $fn_type:ty) => {
        crate::borrow::__impl_borrowed_identity!([$($arg,)* R] $fn_type);

        impl<$($arg: CFnArg,)* R: CFnReturn> ReprC for $fn_type
        where
            Option<Self>: CType,
        {
            type CType = Option<Self>;
        }
        unsafe impl<L: Language, $($arg,)* R> EncodeMove<L> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            type Store = ();

            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, &mut ()) }
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
        unsafe impl<'d,  $($arg,)* R> DecodeMoveUnchecked<'d> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            type SoftStore = ();

            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                unsafe { source.unwrap_unchecked() }
            }
        }
        unsafe impl<'d, L: Language, $($arg,)* R> DecodeMove<'d, L> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            type Store = ();

            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                if $crate::lang::checks::nonzero::<L>(source.is_none()) {
                    Some(unsafe { source.unwrap_unchecked() })
                } else {
                    None
                }
            }

        }

        impl<$($arg,)* R> EncodeUnchecked for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }
        impl<L: Language, $($arg,)* R> Encode<L> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }
        impl< $($arg,)* R> DecodeUnchecked<'_> for $fn_type
        where
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }
        impl<L: Language, $($arg,)* R> Decode<'_, L> for $fn_type
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
        unsafe impl<$($arg,)* R> AbiCompatible for $fn_type
        where
            Self: $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>,
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
        }
        unsafe impl<L: $crate::lang::Language, $($arg,)* R> $crate::convert::AbiValidate<L> for $fn_type
        where
            Self: $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>,
            Self: ReprC<CType = Option<Self>>,
            Option<Self>: CType,
        {
            unsafe fn is_valid(target: &Self::CType) -> bool {
                $crate::lang::checks::nonzero::<L>(target.is_none())
            }
        }
    };
}

macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $check:ident; $encode_fn:expr; $unchecked_fn:expr ) => {
        crate::borrow::__impl_borrowed_identity!([] $src);

        impl ReprC for $src {
            type CType = $dst;
        }
        unsafe impl <L: Language> EncodeMove<L> for $src {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, &mut ()) }
            }
        }
        unsafe impl  EncodeMoveUnchecked for $src {
            type SoftStore = ();

            #[inline(always)]
            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                $encode_fn(self)
            }
        }
        unsafe impl<'d, > DecodeMoveUnchecked<'d> for $src {
            type SoftStore = ();

            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                ($unchecked_fn)(source)
            }
        }
        unsafe impl<'d, L: Language> DecodeMove<'d, L> for $src {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                if $crate::lang::checks::$check::<L>(source.0) {
                    Some(($unchecked_fn)(source))
                } else {
                    None
                }
            }

        }

        impl $crate::convert::EncodeUnchecked for $src {}
        impl <L: Language> Encode<L> for $src {}
        impl<> DecodeUnchecked<'_> for $src {}
        impl<L: Language> Decode<'_, L> for $src {}

        unsafe impl AbiCompatible for $src {}
        unsafe impl<L: $crate::lang::Language> $crate::convert::AbiValidate<L> for $src {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                $crate::lang::checks::$check::<L>(target.0)
            }
        }

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
unsafe impl<R: CType, const N: usize> CFnReturn for [R; N] where
    Self: rust_spec::RustSpec<
            Size = rust_spec::size::Sized<rust_spec::Zero>,
            __Alignment = rust_spec::One,
        >
{
}

unsafe impl<T: EmptyStore, const N: usize> EmptyStore for [T; N] {}
// TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
//unsafe impl<T> EmptyStore for [T; 0] {}

crate::primitive::with_fn_types!(impl_fn_types, {});

fieldless_enum_derive! {
    char => abi::char: {abi::char(0x110000)}:
    unicode_scalar;
    |source: char| abi::char::from(source);
    |source: abi::char| unsafe { char::from_u32_unchecked(source.0) }
}

fieldless_enum_derive! {
    bool => abi::bool: {abi::bool(2)}:
    boolean;
    |source: bool| abi::bool(source as u8);
    |source: abi::bool| unsafe { core::mem::transmute::<abi::bool, bool>(source) }
}
pub(crate) use fieldless_enum_derive;
pub(crate) use primitive_derive;

#[doc(hidden)]
pub mod abi {
    use rust_spec::RustSpec;

    use super::{
        AbiCompatible, AbiValidate, CFnArg, CFnReturn, CType, Decode, DecodeMove,
        DecodeMoveUnchecked, DecodeUnchecked, Encode, EncodeMove, ReprC,
    };
    use crate::lang::Language;

    /// C-compatible byte region used as the representation of `str`.
    ///
    /// Every byte value is valid for this carrier; conversion to `str` accepts only valid `UTF-8`;
    #[derive(RustSpec)]
    #[expect(non_camel_case_types)]
    #[repr(transparent)]
    pub struct str(pub [u8]);

    /// C-compatible carrier for [`prim@bool`].
    ///
    /// Every byte value is valid for this carrier; conversion to `bool` accepts only 0 and 1.
    #[derive(Debug, Clone, Copy, Default, RustSpec)]
    #[expect(non_camel_case_types)]
    #[repr(transparent)]
    pub struct bool(pub u8);

    /// C-compatible carrier for a Unicode scalar value (`char`).
    ///
    /// Every `u32` value is valid for this carrier; conversion to `char` validates the scalar value.
    #[derive(Debug, Clone, Copy, Default, RustSpec)]
    #[expect(non_camel_case_types)]
    #[repr(transparent)]
    pub struct char(pub u32);

    unsafe impl CType for str {}

    impl ReprC for str {
        type CType = Self;
    }

    unsafe impl AbiCompatible for str {}
    unsafe impl<L: Language> AbiValidate<L> for str {
        unsafe fn is_valid(_: &Self::CType) -> core::primitive::bool {
            true
        }
    }

    unsafe impl crate::wide::Wide for str {
        type Header = [u8; 0];
        type Metadata = usize;

        fn metadata(ptr: *const Self) -> usize {
            (ptr as *const [u8]).len()
        }

        fn as_ptr(ptr: *const Self) -> *const Self::Header {
            ptr.cast()
        }

        fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
            ptr.cast()
        }

        unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: usize) -> &'a Self {
            let bytes = unsafe { core::slice::from_raw_parts(data.cast(), len) };
            unsafe { &*(bytes as *const [u8] as *const Self) }
        }

        unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: usize) -> &'a mut Self {
            let bytes = unsafe { core::slice::from_raw_parts_mut(data.cast(), len) };
            unsafe { &mut *(bytes as *mut [u8] as *mut Self) }
        }

        #[cfg(feature = "alloc")]
        fn into_non_null(self: alloc::boxed::Box<Self>) -> core::ptr::NonNull<Self::Header> {
            alloc::boxed::Box::into_non_null(self).cast()
        }

        #[cfg(feature = "alloc")]
        unsafe fn from_non_null(
            data: core::ptr::NonNull<Self::Header>,
            len: usize,
        ) -> alloc::boxed::Box<Self> {
            let ptr = core::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<u8>(), len);
            unsafe { alloc::boxed::Box::from_raw(ptr as *mut Self) }
        }
    }

    impl PartialEq for char {
        fn eq(&self, other: &Self) -> core::primitive::bool {
            core::primitive::char::from_u32(self.0).is_some() && self.0 == other.0
        }
    }

    impl PartialEq for bool {
        fn eq(&self, other: &Self) -> core::primitive::bool {
            self.0 <= 1 && self.0 == other.0
        }
    }

    unsafe impl crate::marker::Zeroable for bool {}
    unsafe impl crate::marker::Zeroable for char {}

    impl From<core::primitive::char> for char {
        fn from(value: core::primitive::char) -> Self {
            Self(value.into())
        }
    }

    impl From<core::primitive::bool> for bool {
        fn from(value: core::primitive::bool) -> Self {
            Self(value.into())
        }
    }

    impl bool {
        pub const FALSE: Self = Self(0);
        pub const TRUE: Self = Self(1);
    }

    primitive_derive! { bool }
    primitive_derive! { char }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec::Vec};
    use core::cmp::Ordering;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::{boxed::abi::Box as AbiBox, wide::abi::WideBox as AbiWideBox};
    use crate::{
        cmp::abi::Ordering as COrdering, convert::Encode, lang, marker::Native,
        option::abi::Option as AbiOption, reference::abi::Ref,
    };

    #[test]
    fn scalar_carriers_accept_all_values_without_niches() {
        assert_impl_all!(abi::bool: CType, ReprC<CType = abi::bool>);
        assert_impl_all!(COrdering: CType, ReprC<CType = COrdering>);
        assert_impl_all!(abi::char: CType, ReprC<CType = abi::char>);
        assert_not_impl_any!(abi::char: Eq, Ord);
        assert_not_impl_any!(abi::bool: Niche);
        assert_not_impl_any!(COrdering: Niche);
        assert_not_impl_any!(abi::char: Niche);
        assert_impl_all!(Option<abi::bool>: ReprC<CType = Native<AbiOption<abi::bool>>>);
        assert_impl_all!(Option<COrdering>: ReprC<CType = Native<AbiOption<COrdering>>>);
        assert_impl_all!(Option<abi::char>: ReprC<CType = Native<AbiOption<abi::char>>>);

        for raw in u8::MIN..=u8::MAX {
            let boolean = abi::bool(raw);
            assert_eq!(boolean.0, raw);
            assert_eq!(boolean == boolean, raw <= 1);
            assert_eq!(
                unsafe { crate::decode::<abi::bool, lang::C>(boolean) }.map(|v| v.0),
                Some(raw)
            );
            assert_eq!(
                unsafe { crate::decode::<bool, lang::C>(boolean) }.is_some(),
                raw <= 1
            );
            let ordering = COrdering(raw as i8);
            assert_eq!(ordering.0, raw as i8);
            assert_eq!(
                unsafe { crate::decode::<COrdering, lang::C>(ordering) }.map(|v| v.0),
                Some(ordering.0)
            );
            assert_eq!(
                unsafe { crate::decode::<Ordering, lang::C>(ordering) }.is_some(),
                (-1..=1).contains(&ordering.0)
            );
        }

        for raw in [0, 0xd800, 0x10ffff, 0x110000, u32::MAX] {
            let scalar = abi::char(raw);
            assert_eq!(scalar.0, raw);
            assert_eq!(scalar == scalar, char::from_u32(raw).is_some());
            assert_eq!(
                unsafe { crate::decode::<abi::char, lang::C>(scalar) }.map(|value| value.0),
                Some(raw)
            );
            assert_eq!(
                unsafe { crate::decode::<char, lang::C>(scalar) },
                char::from_u32(raw)
            );
        }
        for value in [Ordering::Less, Ordering::Equal, Ordering::Greater] {
            assert_eq!(
                unsafe { crate::decode_unchecked::<Ordering>(crate::encode::<_, lang::C>(value)) },
                value
            );
        }
        assert!(bool::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<bool>) }.0.0
        ));
        assert!(Ordering::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<Ordering>) }.0.0
        ));
        assert!(char::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<char>) }.0.0
        ));
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
        let encoded = unsafe { crate::encode::<_, lang::C>(encoded) };
        let decoded = unsafe { crate::decode::<Callback, lang::C>(encoded) }.unwrap();
        assert_eq!(decoded(41), 42);
        assert!(unsafe { crate::decode::<Callback, lang::C>(None) }.is_none());
        assert!(
            unsafe { crate::soft_encode::<_, lang::C>(Option::<Callback>::None, &mut None) }
                .0
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

        #[expect(improper_ctypes_definitions)]
        type TupleCallback = extern "C" fn((u8,)) -> u8;
        assert_not_impl_any!(TupleCallback: ReprC, Encode, Decode<'static>);

        #[expect(improper_ctypes_definitions)]
        type SystemCallbackWithRustTypes = unsafe extern "system" fn(bool) -> (u8,);
        assert_not_impl_any!(SystemCallbackWithRustTypes: ReprC, Encode, Decode<'static>);

        #[cfg(feature = "alloc")]
        {
            assert_not_impl_any!(extern "C" fn(Box<u8>) -> (): ReprC);
            assert_not_impl_any!(extern "C" fn() -> Box<u8>: ReprC);
            assert_not_impl_any!(fn(Box<u8>) -> u8: ReprC);
            type OwnedCallback = extern "C" fn(AbiBox<u8>) -> AbiBox<u8>;
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
            Niche<CType = Ref<'static, u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut u8:
            Niche<CType = crate::reference::abi::RefMut<'static, u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<u8>:
            Niche<CType = AbiBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[u8]:
            Niche<CType = crate::wide::abi::WideRef<'static, [u8]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [u8]:
            Niche<CType = crate::wide::abi::WideRefMut<'static, [u8]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[u8]>:
            Niche<CType = AbiWideBox<[u8]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<u8>:
            Niche<CType = Native<crate::vec::abi::Vec<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([u8; 2]:
            Decode<'static>,
            Encode,
            CType,
        );
        assert_impl_all!(Option<u8>:
            Niche<CType = Native<AbiOption<u8>>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn char_has_distinct_scalar_carrier() {
        static_assertions::assert_impl_all!(char: ReprC<CType = abi::char>);
        static_assertions::assert_impl_all!(abi::char: From<char>);
        static_assertions::assert_not_impl_any!(abi::char: From<u32>, TryFrom<u32>);
        assert_eq!(abi::char('🍃' as u32), unsafe {
            crate::encode::<_, lang::C>('🍃')
        });
        assert_eq!(unsafe { crate::encode::<_, lang::C>('🍃') }.0, '🍃' as u32);
        assert_eq!(
            unsafe { crate::decode::<char, lang::C>(abi::char(0x110000)) },
            None
        );
        assert_eq!(
            unsafe { crate::decode::<char, lang::C>(abi::char(0xd800)) },
            None
        );
    }

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str: ReprC<CType = crate::primitive::abi::str>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(String: ReprC<CType = Native<crate::string::abi::String>>);

        assert_impl_all!(&str:
            Niche<CType = crate::wide::abi::WideRef<'static, <str as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            Niche<CType = crate::wide::abi::SoftWideRefMut<<str as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            Niche<CType = AbiWideBox<<str as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn str_decode_rejects_invalid_utf8() {
        let invalid = [0xff];

        let source =
            unsafe { crate::wide::abi::WideRef::from_raw_parts(invalid.as_ptr(), invalid.len()) };
        let decoded = unsafe { crate::decode::<&str, lang::C>(source) };
        assert!(decoded.is_none());

        let mut invalid = [0xff];
        let source = unsafe {
            crate::wide::abi::SoftWideRefMut::<_>::from_raw_parts_mut(
                invalid.as_mut_ptr(),
                invalid.len(),
            )
        };
        let decoded = unsafe { crate::decode::<&mut str, lang::C>(source) };
        assert!(decoded.is_none());

        #[cfg(feature = "alloc")]
        {
            let allocation = Box::into_non_null(Box::new([0xff_u8])).cast();
            let source = unsafe {
                AbiWideBox::<<str as crate::ReprC>::CType>::from_raw_parts(allocation, 1)
            };
            let decoded = unsafe { crate::decode::<Box<str>, lang::C>(source) };
            assert!(decoded.is_none());
        }
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_strings_round_trip_without_recursive_conversion() {
        let text = String::from("hello 🍃");
        let encoded = unsafe { crate::encode::<_, lang::C>(text.clone()) };
        assert_eq!(
            unsafe { crate::decode::<String, lang::C>(encoded) },
            Some(text.clone())
        );

        let encoded = unsafe { crate::encode::<_, lang::C>(text.clone().into_boxed_str()) };
        assert_eq!(
            unsafe { crate::decode::<Box<str>, lang::C>(encoded) }.as_deref(),
            Some(text.as_str())
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_string_preserves_allocation_and_capacity() {
        let mut text = String::with_capacity(128);
        text.push_str("hello 🍃");
        let pointer = text.as_ptr();
        let capacity = text.capacity();
        let carrier = unsafe { crate::encode::<_, lang::C>(text) };
        assert_eq!(carrier.as_ptr(), pointer.cast_mut());
        assert_eq!(carrier.capacity(), capacity);
        let decoded = unsafe { crate::decode::<String, lang::C>(carrier) }.unwrap();
        assert_eq!(decoded, "hello 🍃");
        assert_eq!(decoded.as_ptr(), pointer);
        assert_eq!(decoded.capacity(), capacity);

        let invalid = crate::string::abi::String(crate::vec::abi::Vec::from_vec(alloc::vec![0xff]));
        assert!(unsafe { crate::decode::<String, lang::C>(Native(invalid)) }.is_none());
        assert!(
            unsafe { crate::decode::<String, lang::C>(Native(crate::string::abi::String::null())) }
                .is_none()
        );
    }
}
