use core::{cell::UnsafeCell, ptr::NonNull};

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
use alloc::{boxed::Box, string::String, vec::Vec};

#[cfg(feature = "owned_as_ref")]
#[cfg(feature = "owned_types")]
use crate::slice::CSliceMut;
use crate::{
    Decode, Encode, ExternC, ReprC,
    ir::{Ir, Transparent},
    mineral,
    niche::{Ir as NicheIr, Niche, WithCustomNiche, WithStableNiche, WithoutNiche},
    slice::CSlice,
    transmute::CheckedTransmute,
    tuple::CTuple2,
};

macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        mineral! {
            unsafe impl Transparent for $ty {
                type Target = $target;

                const NICHE_VALUE: Self::CType = 0;
                fn is_valid(target: &Self::Target) -> bool {
                    *target != <Self as crate::niche::Niche>::NICHE_VALUE
                }
            }
        }

        unsafe impl crate::niche::StableNiche for $ty {})+
    }
}

non_zero_derive! {
    core::num::NonZeroU8 => u8,
    core::num::NonZeroI8 => i8,
    core::num::NonZeroU16 => u16,
    core::num::NonZeroI16 => i16,
    core::num::NonZeroU32 => u32,
    core::num::NonZeroI32 => i32,
    core::num::NonZeroU64 => u64,
    core::num::NonZeroI64 => i64,
    core::num::NonZeroU128 => u128,
    core::num::NonZeroI128 => i128,
}

mineral! {
    unsafe impl(T,) Transparent for core::mem::ManuallyDrop<T> {
        type Target = T;
    }
}

impl<T, E> Ir for Result<T, E> {
    type Type = Self;
}
impl<T> Ir for UnsafeCell<T> {
    type Type = Transparent;
}
impl<T> Ir for NonNull<T> {
    type Type = Transparent;
}
impl Ir for &str {
    type Type = Transparent;
}
#[cfg(feature = "non_robust_ref_mut")]
impl Ir for &mut str {
    type Type = Transparent;
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl Ir for Box<str> {
    type Type = Transparent;
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl Ir for String {
    type Type = Transparent;
}

impl<T, E> NicheIr for Result<T, E>
where
    (T, E): NicheIr,
{
    type Type = <(T, E) as NicheIr>::Type;
}
impl<T> NicheIr for UnsafeCell<T> {
    type Type = WithoutNiche;
}
impl<T> NicheIr for NonNull<T> {
    type Type = WithStableNiche;
}
impl NicheIr for &str {
    type Type = WithCustomNiche;
}
#[cfg(feature = "non_robust_ref_mut")]
impl NicheIr for &mut str {
    type Type = WithCustomNiche;
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl NicheIr for Box<str> {
    type Type = WithCustomNiche;
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl NicheIr for String {
    type Type = WithCustomNiche;
}

unsafe impl<T> CheckedTransmute for UnsafeCell<T> {
    type Target = T;

    #[inline(always)]
    fn is_valid(_: &Self::Target) -> bool {
        true
    }
}
unsafe impl<T> CheckedTransmute for NonNull<T> {
    type Target = *mut T;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        !target.is_null()
    }
}
unsafe impl<'a> CheckedTransmute for &'a str {
    type Target = &'a [u8];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
#[cfg(feature = "non_robust_ref_mut")]
unsafe impl<'a> CheckedTransmute for &'a mut str {
    type Target = &'a mut [u8];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
unsafe impl CheckedTransmute for Box<str> {
    // WARN: `core::str::as_bytes` uses transmute internally which means that
    // even though it's a string slice it can be transmuted into byte slice.
    type Target = Box<[u8]>;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
unsafe impl CheckedTransmute for String {
    // WARN: This can be contested as it is nowhere documented that String is
    // actually transmutable into Vec<u8>, but implicitly it should be
    type Target = Vec<u8>;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}

impl<T: ExternC, E: ExternC> Niche for Result<T, E> {
    const NICHE_VALUE: Self::CType = CTuple2(2u8, unsafe { core::mem::zeroed() });
}
impl<T> Niche for NonNull<T> {
    const NICHE_VALUE: Self::CType = core::ptr::null_mut();
}
impl Niche for &str {
    const NICHE_VALUE: Self::CType = CSlice::none();
}
#[cfg(feature = "non_robust_ref_mut")]
impl Niche for &mut str {
    const NICHE_VALUE: Self::CType = CSliceMut::none();
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CSliceMut::none();
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl Niche for Box<str> {
    const NICHE_VALUE: Self::CType = CSliceMut::none();
}

impl<T: ExternC, E: ExternC> ExternC for Result<T, E> {
    type CType = CTuple2<<u8 as ExternC>::CType, ResultPayload<T::CType, E::CType>>;
}
impl<T: Encode, E: Encode> Encode for Result<T, E> {
    type Store = (T::Store, E::Store);

    fn encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        match self {
            Ok(ok) => CTuple2(
                Encode::encode(0u8, &mut ()),
                ResultPayload {
                    Ok: ok.encode(&mut store.0),
                },
            ),
            Err(err) => CTuple2(
                Encode::encode(1u8, &mut ()),
                ResultPayload {
                    Err: err.encode(&mut store.1),
                },
            ),
        }
    }
}
impl<'d, T: Decode<'d>, E: Decode<'d>> Decode<'d> for Result<T, E> {
    type Store = (T::Store, E::Store);

    unsafe fn decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> crate::Result<Self> {
        let payload = source.1;

        match source.0 {
            0 => Ok(Ok(unsafe { T::decode(payload.Ok, &mut store.0)? })),
            1 => Ok(Err(unsafe { E::decode(payload.Err, &mut store.1)? })),
            _ => Err(crate::FfiReturn::TrapRepresentation),
        }
    }
}

#[repr(C)]
#[expect(non_snake_case)]
pub union ResultPayload<T: ReprC, E: ReprC> {
    pub Ok: T,
    pub Err: E,
}

impl<T: ReprC, E: ReprC> Copy for ResultPayload<T, E> {}
impl<T: ReprC, E: ReprC> Clone for ResultPayload<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

mineral! {
    unsafe impl(T: ReprC, E: ReprC) Robust for ResultPayload<T, E> {}
}
