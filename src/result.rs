//! C-compatible representation of [`core::result::Result`].
use core::{marker::PhantomData, mem::MaybeUninit, ops::Add, result::Result as RustResult};

use rust_spec::{
    RustSpec,
    drop::{DropKind, NoDrop},
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Thin,
    convert::AbiCompatibleValidate,
    convert::{
        Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
        SoftEncodeRef, SoftEncodeRefUnchecked,
    },
    marker::AbiCompatible,
    sync::RefSizedEncodeStore,
};

/// A transparent `Ok` payload with no Rust result discriminant.
/// Lowers to `crate::marker::Stable<abi::ResultOk<T::CType, ()>>` and preserves the payload's validity requirements.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub struct ResultOk<T>(pub T);

/// A transparent `Err` payload with no Rust result discriminant.
/// Lowers to `crate::marker::Stable<abi::ResultErr<(), E::CType>>` and preserves the payload's validity requirements.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub struct ResultErr<E>(pub E);

crate::option::impl_niche_payload!(ResultOk => crate::marker::Stable<abi::ResultOk<T::CType, ()>>);
crate::option::impl_niche_payload!(ResultErr => crate::marker::Stable<abi::ResultErr<(), T::CType>>);

/// A result with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> ReprC for Result<T, E> {
    type CType = crate::marker::Stable<abi::Result<T::CType, E::CType>>;
}

macro_rules! result_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<T: $unchecked<CType: Copy>, E: $unchecked<CType: Copy>> $unchecked
            for Result<T, E>
        {
            type SoftStore = core::option::Option<RustResult<T::SoftStore, E::SoftStore>>;

            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                match self {
                    Self::Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        crate::marker::Stable(abi::Result::Ok(unsafe {
                            value.soft_encode_unchecked(inner)
                        }))
                    }
                    Self::Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        crate::marker::Stable(abi::Result::Err(unsafe {
                            value.soft_encode_unchecked(inner)
                        }))
                    }
                }
            }
        }
        unsafe impl<T: $encode<CType: Copy>, E: $encode<CType: Copy>> $encode for Result<T, E> {
            type Store = core::option::Option<RustResult<T::Store, E::Store>>;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                match self {
                    Self::Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        crate::marker::Stable(abi::Result::Ok($encode::soft_encode(value, inner)))
                    }
                    Self::Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        crate::marker::Stable(abi::Result::Err($encode::soft_encode(value, inner)))
                    }
                }
            }
        }
        unsafe impl<'d, T: $decode<'d, CType: Copy>, E: $decode<'d, CType: Copy>> $decode<'d>
            for Result<T, E>
        {
            type Store = core::option::Option<RustResult<T::Store, E::Store>>;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                match source.0.into_result()? {
                    Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        Some(Self::Ok(unsafe { T::soft_decode(value, inner)? }))
                    }
                    Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        Some(Self::Err(unsafe { E::soft_decode(value, inner)? }))
                    }
                }
            }

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Self {
                match unsafe { source.0.into_result().unwrap_unchecked() } {
                    Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        Self::Ok(unsafe { T::soft_decode_unchecked(value, inner) })
                    }
                    Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        Self::Err(unsafe { E::soft_decode_unchecked(value, inner) })
                    }
                }
            }
        }
    };
}
result_codecs!(EncodeMove, EncodeMoveUnchecked, DecodeMove);
const _: () = {
    use crate::borrow::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
    unsafe impl<T: BorrowedEncodeUnchecked<CType: Copy>, E: BorrowedEncodeUnchecked<CType: Copy>>
        BorrowedEncodeUnchecked for Result<T, E>
    {
        type Owner = core::option::Option<RustResult<T::Owner, E::Owner>>;
        type SoftStore = core::option::Option<RustResult<T::SoftStore, E::SoftStore>>;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Self::Ok(value) => {
                    let Ok(inner) = store.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let Ok(inner_owner) = owner.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    crate::marker::Stable(abi::Result::Ok(unsafe {
                        value.soft_encode_unchecked(inner, inner_owner)
                    }))
                }
                Self::Err(value) => {
                    let Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let Err(inner_owner) = owner.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    crate::marker::Stable(abi::Result::Err(unsafe {
                        value.soft_encode_unchecked(inner, inner_owner)
                    }))
                }
            }
        }
    }
    unsafe impl<T: BorrowedEncode<CType: Copy>, E: BorrowedEncode<CType: Copy>> BorrowedEncode
        for Result<T, E>
    {
        type Store = core::option::Option<RustResult<T::Store, E::Store>>;

        fn soft_encode<'a>(
            self,
            store: &'a mut Self::Store,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            match self {
                Self::Ok(value) => {
                    let Ok(inner) = store.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    let Ok(inner_owner) = owner.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    crate::marker::Stable(abi::Result::Ok(BorrowedEncode::soft_encode(
                        value,
                        inner,
                        inner_owner,
                    )))
                }
                Self::Err(value) => {
                    let Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let Err(inner_owner) = owner.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    crate::marker::Stable(abi::Result::Err(BorrowedEncode::soft_encode(
                        value,
                        inner,
                        inner_owner,
                    )))
                }
            }
        }
    }
    unsafe impl<'d, T: BorrowedDecode<'d, CType: Copy>, E: BorrowedDecode<'d, CType: Copy>>
        BorrowedDecode<'d> for Result<T, E>
    {
        type Store = core::option::Option<RustResult<T::Store, E::Store>>;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Option<Self> {
            match source.0.into_result()? {
                Ok(value) => {
                    let Ok(inner) = store.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    Some(Self::Ok(unsafe { T::soft_decode(value, inner)? }))
                }
                Err(value) => {
                    let Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    Some(Self::Err(unsafe { E::soft_decode(value, inner)? }))
                }
            }
        }

        unsafe fn soft_decode_unchecked<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Self {
            match unsafe { source.0.into_result().unwrap_unchecked() } {
                Ok(value) => {
                    let Ok(inner) = store.insert(Ok(Default::default())) else {
                        unreachable!()
                    };
                    Self::Ok(unsafe { T::soft_decode_unchecked(value, inner) })
                }
                Err(value) => {
                    let Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    Self::Err(unsafe { E::soft_decode_unchecked(value, inner) })
                }
            }
        }
    }
};

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind>
    SoftEncodeRefUnchecked<'a> for Result<T, E>
where
    &'a Self: crate::ReprC<CType = crate::reference::SoftRef<<Self as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType: crate::reference::RefCarrier<T>>,
    &'a E: EncodeMoveUnchecked<CType: crate::reference::RefCarrier<E>>,
{
    type SoftStore = RefSizedEncodeStore<
        Self,
        Option<
            RustResult<
                <&'a T as EncodeMoveUnchecked>::SoftStore,
                <&'a E as EncodeMoveUnchecked>::SoftStore,
            >,
        >,
    >;

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| match self {
            Self::Ok(value) => {
                let Ok(inner) = fields.insert(Ok(Default::default())) else {
                    unreachable!()
                };
                crate::marker::Stable(abi::Result::Ok(unsafe {
                    crate::stored::encode_ref_field_unchecked(value, inner)
                }))
            }
            Self::Err(value) => {
                let Err(inner) = fields.insert(Err(Default::default())) else {
                    unreachable!()
                };
                crate::marker::Stable(abi::Result::Err(unsafe {
                    crate::stored::encode_ref_field_unchecked(value, inner)
                }))
            }
        })
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind>
    SoftEncodeRef<'a> for Result<T, E>
where
    &'a Self: crate::ReprC<CType = crate::reference::SoftRef<<Self as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType: crate::reference::RefCarrier<T>>,
    &'a E: EncodeMove<CType: crate::reference::RefCarrier<E>>,
{
    type Store = RefSizedEncodeStore<
        Self,
        core::option::Option<
            RustResult<<&'a T as EncodeMove>::Store, <&'a E as EncodeMove>::Store>,
        >,
    >;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| match self {
            Self::Ok(value) => {
                let Ok(inner) = fields.insert(Ok(Default::default())) else {
                    unreachable!()
                };
                crate::marker::Stable(abi::Result::Ok(crate::stored::encode_ref_field(
                    value, inner,
                )))
            }
            Self::Err(value) => {
                let Err(inner) = fields.insert(Err(Default::default())) else {
                    unreachable!()
                };
                crate::marker::Stable(abi::Result::Err(crate::stored::encode_ref_field(
                    value, inner,
                )))
            }
        })
    }
}

impl<T: Encode<CType: Copy>, E: Encode<CType: Copy>> Encode for Result<T, E> {}
impl<T: EncodeUnchecked<CType: Copy>, E: EncodeUnchecked<CType: Copy>> EncodeUnchecked
    for Result<T, E>
{
}
impl<'d, T: Decode<'d, CType: Copy>, E: Decode<'d, CType: Copy>> Decode<'d> for Result<T, E> {}

unsafe impl<T: AbiCompatibleValidate<CType: Copy>, E: AbiCompatibleValidate<CType: Copy>>
    AbiCompatibleValidate for Result<T, E>
{
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag() {
            0 => unsafe { T::is_valid(target.ok.1.assume_init_ref()) },
            1 => unsafe { E::is_valid(target.err.1.assume_init_ref()) },
            _ => false,
        }
    }
}
unsafe impl<T: AbiCompatible<CType: Copy>, E: AbiCompatible<CType: Copy>> AbiCompatible
    for Result<T, E>
{
}

unsafe impl<T: EmptyStore, E: EmptyStore> EmptyStore for RustResult<T, E> {}

unsafe impl<T, E> CType for RustResult<T, E> where
    Self: RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::Robust, Drop = NoDrop>
{
}

unsafe impl<T, E> CFnArg for RustResult<T, E> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T, E> CFnReturn for RustResult<T, E> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

impl<R: Encode, E: Encode> Encode for RustResult<R, E> where Self: EncodeMove {}
impl<R: EncodeUnchecked, E: EncodeUnchecked> EncodeUnchecked for RustResult<R, E> where
    Self: EncodeMoveUnchecked
{
}
impl<'d, R: Decode<'d>, E: Decode<'d>> Decode<'d> for RustResult<R, E> where Self: DecodeMove<'d> {}

impl<T, E> From<RustResult<T, E>> for Result<T, E> {
    fn from(value: RustResult<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(value) => Self::Err(value),
        }
    }
}
impl<T, E> From<Result<T, E>> for RustResult<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Result::Ok(value) => Self::Ok(value),
            Result::Err(value) => Self::Err(value),
        }
    }
}

#[doc(hidden)]
pub mod abi {
    macro_rules! niche_result_carrier {
    ($name:ident, $payload:ident, $other:ident, [$($generics:tt)*], $description:literal) => {
        #[doc = $description]
        /// The co3 source wraps this carrier in [`crate::marker::Stable`].
        /// Co3 payload wrappers use `()` as the marker for the other arm.
        #[repr(transparent)]
        #[derive(RustSpec)]
        #[doc(hidden)]
        pub struct $name<$($generics)*>(pub $payload, pub PhantomData<$other>);

        impl<$($generics)*> $name<Ok, Err> {
            pub(super) fn from_payload(value: $payload) -> Self {
                Self(value, PhantomData)
            }
        }

        impl<$($generics)*> ReprC for $name<Ok, Err> {
            type CType = Self;
        }
        unsafe impl<$($generics)*> CType for $name<Ok, Err> {}
        unsafe impl<$($generics)*> CFnArg for $name<Ok, Err> where $payload: CFnArg {}
        unsafe impl<$($generics)*> CFnReturn for $name<Ok, Err> where $payload: CFnReturn {}

        crate::borrow::__impl_borrowed_identity!([$($generics)*] $name<Ok, Err> );

        unsafe impl<$($generics)*> EncodeMoveUnchecked for $name<Ok, Err> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self::CType
            where
                Self: 'a,
            {
                self
            }
        }
        unsafe impl<$($generics)*> EncodeMove for $name<Ok, Err> {
            type Store = ();

            fn soft_encode<'a>(self, _: &'a mut ()) -> Self::CType
            where
                Self: 'a,
            {
                self
            }
        }
        unsafe impl<'d, $($generics)*> DecodeMove<'d> for $name<Ok, Err> {
            type Store = ();

            unsafe fn soft_decode<'a: 'd>(source: Self::CType, _: &'a mut ()) -> Option<Self> {
                Some(source)
            }
        }
        impl<$($generics)*> Encode for $name<Ok, Err> {}
        impl<$($generics)*> EncodeUnchecked for $name<Ok, Err> {}
        impl<'d, $($generics)*> Decode<'d> for $name<Ok, Err> {}
        unsafe impl<$($generics)*> AbiCompatibleValidate for $name<Ok, Err> {
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }
        unsafe impl<$($generics)*> AbiCompatible for $name<Ok, Err> {}

        impl<$($generics)*> Copy for $name<Ok, Err> where $payload: Copy {}
        impl<$($generics)*> Clone for $name<Ok, Err> where $payload: Clone {
            fn clone(&self) -> Self {
                Self(self.0.clone(), PhantomData)
            }
        }
        impl<$($generics)*> core::fmt::Debug for $name<Ok, Err> where $payload: core::fmt::Debug {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl<$($generics)*> Eq for $name<Ok, Err> where $payload: Eq {}
        impl<$($generics)*> PartialEq for $name<Ok, Err> where $payload: PartialEq {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl<$($generics)*> Ord for $name<Ok, Err> where $payload: Ord {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.0.cmp(&other.0)
            }
        }
        impl<$($generics)*> PartialOrd for $name<Ok, Err> where $payload: PartialOrd {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                self.0.partial_cmp(&other.0)
            }
        }
        impl<$($generics)*> core::hash::Hash for $name<Ok, Err> where $payload: core::hash::Hash {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                self.0.hash(state);
            }
        }
    };
}

    niche_result_carrier!(
        ResultOk,
        Ok,
        Err,
        [Ok: CType, Err: CType],
        "Carrier for a native `Result<T, E>` whose `Ok` value has a niche representing `Err`."
    );
    niche_result_carrier!(
        ResultErr,
        Err,
        Ok,
        [Ok: CType, Err: CType],
        "Carrier for a native `Result<T, E>` whose `Err` value has a niche representing `Ok`."
    );

    use super::*;

    /// FFI-safe equivalent of [`core::result::Result`].
    /// `T` and `E` are lowered payload types stored directly in the union.
    /// Co3 sources select [`crate::marker::Stable<Self>`]; native Rust sources select this carrier directly.
    #[repr(C)]
    #[doc(hidden)]
    pub union Result<T: CType + Copy, E: CType + Copy> {
        pub(super) ok: ResultVariant<T>,
        pub(super) err: ResultVariant<E>,
    }

    #[repr(C)]
    pub(super) struct ResultVariant<T: CType + Copy>(pub(super) u8, pub(super) MaybeUninit<T>);

    impl<T: CType + Copy> Copy for ResultVariant<T> {}

    impl<T: CType + Copy> Clone for ResultVariant<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T: CType + core::fmt::Debug + Copy, E: CType + core::fmt::Debug + Copy> core::fmt::Debug
        for Result<T, E>
    {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self.tag() {
                0 => f
                    .debug_tuple("Result::Ok")
                    .field(unsafe { self.ok.1.assume_init_ref() })
                    .finish(),
                1 => f
                    .debug_tuple("Result::Err")
                    .field(unsafe { self.err.1.assume_init_ref() })
                    .finish(),
                tag => f
                    .debug_struct("Result::<invalid>")
                    .field("tag", &tag)
                    .finish(),
            }
        }
    }

    impl<T: CType + PartialEq + Copy, E: CType + PartialEq + Copy> PartialEq for Result<T, E> {
        fn eq(&self, other: &Self) -> bool {
            match (self.tag(), other.tag()) {
                (0, 0) => {
                    let self_payload = unsafe { self.ok.1.assume_init_ref() };
                    let other_payload = unsafe { other.ok.1.assume_init_ref() };

                    self_payload == other_payload
                }
                (1, 1) => {
                    let self_payload = unsafe { self.err.1.assume_init_ref() };
                    let other_payload = unsafe { other.err.1.assume_init_ref() };

                    self_payload == other_payload
                }
                _ => false,
            }
        }
    }

    impl<T: CType + PartialOrd + Copy, E: CType + PartialOrd + Copy> PartialOrd for Result<T, E> {
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
            match (self.tag(), other.tag()) {
                (0, 0) => {
                    let self_payload = unsafe { self.ok.1.assume_init_ref() };
                    let other_payload = unsafe { other.ok.1.assume_init_ref() };

                    self_payload.partial_cmp(other_payload)
                }
                (0, 1) => Some(core::cmp::Ordering::Greater),
                (1, 0) => Some(core::cmp::Ordering::Less),
                (1, 1) => {
                    let self_payload = unsafe { self.err.1.assume_init_ref() };
                    let other_payload = unsafe { other.err.1.assume_init_ref() };

                    self_payload.partial_cmp(other_payload)
                }
                _ => None,
            }
        }
    }

    impl<T: CType + Copy, E: CType + Copy> Result<T, E> {
        pub(crate) const fn niche() -> Self {
            Self {
                ok: ResultVariant(2, MaybeUninit::zeroed()),
            }
        }

        /// Construct a carrier from a result payload.
        pub const fn new(value: RustResult<T, E>) -> Self {
            match value {
                Ok(value) => Self::Ok(value),
                Err(value) => Self::Err(value),
            }
        }

        /// Construct a carrier from a tag and one of its raw payload representations.
        ///
        /// # Safety
        /// A tag of 0 requires an initialized `Ok` payload, and a tag of 1 requires
        /// an initialized `Err` payload. Other tags permit either payload to be uninitialized.
        pub const unsafe fn from_raw_parts(
            tag: u8,
            payload: RustResult<MaybeUninit<T>, MaybeUninit<E>>,
        ) -> Self {
            match payload {
                Ok(payload) => Self {
                    ok: ResultVariant(tag, payload),
                },
                Err(payload) => Self {
                    err: ResultVariant(tag, payload),
                },
            }
        }

        /// Construct the success value
        #[expect(non_snake_case)]
        pub const fn Ok(ok: T) -> Self {
            Self {
                ok: ResultVariant(0, MaybeUninit::new(ok)),
            }
        }

        /// Construct the error value
        #[expect(non_snake_case)]
        pub const fn Err(err: E) -> Self {
            Self {
                err: ResultVariant(1, MaybeUninit::new(err)),
            }
        }
    }

    impl<T: CType + Copy, E: CType + Copy> Result<T, E> {
        /// Return the stored variant tag.
        #[inline(always)]
        pub const fn tag(&self) -> u8 {
            // SAFETY: Both union variants place the tag at offset zero.
            unsafe { *core::ptr::from_ref(self).cast::<u8>() }
        }

        pub(crate) const fn is_niche(&self) -> bool {
            self.tag() == Self::niche().tag()
        }

        /// Return the represented `core::result::Result<T, E>`, or `None` for an invalid tag.
        pub const fn into_result(self) -> core::option::Option<RustResult<T, E>> {
            match self.tag() {
                0 => Some(Ok(unsafe { self.ok.1.assume_init() })),
                1 => Some(Err(unsafe { self.err.1.assume_init() })),
                _ => None,
            }
        }

        /// Borrow only the active payload, rejecting invalid tags.
        pub(crate) const fn as_result(&self) -> core::option::Option<RustResult<&T, &E>> {
            match self.tag() {
                0 => Some(Ok(unsafe { self.ok.1.assume_init_ref() })),
                1 => Some(Err(unsafe { self.err.1.assume_init_ref() })),
                _ => None,
            }
        }
    }

    impl<T: CType + Copy, E: CType + Copy> Copy for Result<T, E> {}

    impl<T: CType + Copy, E: CType + Copy> Clone for Result<T, E> {
        fn clone(&self) -> Self {
            *self
        }
    }

    unsafe impl<T: CType + RustSpec + Copy, E: CType + RustSpec + Copy> RustSpec for Result<T, E>
    where
        T: RustSpec<Layout: Add<<E as RustSpec>::Layout>, Trap: Add<<E as RustSpec>::Trap>>,
        T: RustSpec<Drop: Add<<E as RustSpec>::Drop, Output: DropKind>>,
        <T as RustSpec>::__Alignment: rust_spec::Max<<E as RustSpec>::__Alignment>,
        <T as RustSpec>::Mutability: Add<<E as RustSpec>::Mutability>,
        <T as RustSpec>::__IndirectTrap: Add<<E as RustSpec>::__IndirectTrap>,
    {
        type Layout = <<T as RustSpec>::Layout as Add<<E as RustSpec>::Layout>>::Output;
        type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
        type __Alignment =
            <<T as RustSpec>::__Alignment as rust_spec::Max<<E as RustSpec>::__Alignment>>::Output;
        type Trap = <<T as RustSpec>::Trap as Add<<E as RustSpec>::Trap>>::Output;
        type Niche = WithoutNiche;
        type Drop = <<T as RustSpec>::Drop as Add<<E as RustSpec>::Drop>>::Output;
        type Mutability = <<T as RustSpec>::Mutability as Add<<E as RustSpec>::Mutability>>::Output;
        type Opacity = rust_spec::NonOpaque;
        type __IndirectTrap =
            <<T as RustSpec>::__IndirectTrap as Add<<E as RustSpec>::__IndirectTrap>>::Output;
    }

    crate::borrow::__impl_borrowed_identity!([T: CType + Copy, E: CType + Copy] Result<T, E>);

    crate::wide::impl_identity_carrier! { Result, [T: CType + Copy, E: CType + Copy], [T, E] }
}
