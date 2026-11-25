//! C-compatible representation of [`core::result::Result`].

use core::{marker::PhantomData, mem::MaybeUninit, ops::Add};

#[cfg(feature = "repr-rust")]
use rust_spec::mutability::{Exclusive, ExclusiveKind};
use rust_spec::{
    RustSpec,
    drop::{DropKind, NoDrop},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    borrow::{BorrowedDecode, BorrowedDecodeUnchecked, BorrowedEncode, BorrowedEncodeUnchecked},
    convert::{
        AbiValidate, Decode, DecodeMove, DecodeMoveUnchecked, DecodeUnchecked, EmptyStore, Encode,
        EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
    },
    lang::Language,
    marker::{AbiCompatible, Native},
};
#[cfg(feature = "repr-rust")]
use crate::{
    Thin,
    convert::{SoftEncodeRef, SoftEncodeRefUnchecked},
    reference::abi::RefCarrier,
    sync::RefSizedEncodeStore,
};

/// A result with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

impl<T, E> From<core::result::Result<T, E>> for Result<T, E> {
    fn from(value: core::result::Result<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(value) => Self::Err(value),
        }
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> ReprC for Result<T, E> {
    type CType = abi::Result<T::CType, E::CType>;
}

// Each slot holds state for the active variant. Prepare them in argument order.
macro_rules! encode_tagged_result {
    ($input:expr, $encode:path, $($slot:ident),+) => {
        match $input {
            Self::Ok(value) => {
                $(let Ok($slot) = $slot.insert(Ok(Default::default())) else { unreachable!() };)+
                abi::Result::Ok($encode(value, $($slot),+))
            }
            Self::Err(value) => {
                $(let Err($slot) = $slot.insert(Err(Default::default())) else { unreachable!() };)+
                abi::Result::Err($encode(value, $($slot),+))
            }
        }
    };
}

unsafe impl<T: EncodeMoveUnchecked<CType: Copy>, E: EncodeMoveUnchecked<CType: Copy>>
    EncodeMoveUnchecked for Result<T, E>
{
    type SoftStore = core::option::Option<core::result::Result<T::SoftStore, E::SoftStore>>;

    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { encode_tagged_result!(self, EncodeMoveUnchecked::soft_encode_unchecked, store) }
    }
}
unsafe impl<L: Language, T: EncodeMove<L, CType: Copy>, E: EncodeMove<L, CType: Copy>> EncodeMove<L>
    for Result<T, E>
{
    type Store = core::option::Option<core::result::Result<T::Store, E::Store>>;

    fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
    where
        Self: 'a,
    {
        encode_tagged_result!(self, EncodeMove::<L>::soft_encode, store)
    }
}

unsafe impl<T: BorrowedEncodeUnchecked<CType: Copy>, E: BorrowedEncodeUnchecked<CType: Copy>>
    BorrowedEncodeUnchecked for Result<T, E>
{
    type Owner = core::option::Option<core::result::Result<T::Owner, E::Owner>>;
    type SoftStore = core::option::Option<core::result::Result<T::SoftStore, E::SoftStore>>;

    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe {
            encode_tagged_result!(
                self,
                BorrowedEncodeUnchecked::soft_encode_unchecked,
                store,
                owner
            )
        }
    }
}
unsafe impl<L: Language, T: BorrowedEncode<L, CType: Copy>, E: BorrowedEncode<L, CType: Copy>>
    BorrowedEncode<L> for Result<T, E>
{
    type Store = core::option::Option<core::result::Result<T::Store, E::Store>>;

    fn soft_encode<'a>(self, store: &'a mut Self::Store, owner: &'a mut Self::Owner) -> Self::CType
    where
        Self: 'a,
    {
        encode_tagged_result!(self, BorrowedEncode::<L>::soft_encode, store, owner)
    }
}

macro_rules! impl_result_decode {
    ($decode:ident, $unchecked:ident) => {
        unsafe impl<'d, T: $unchecked<'d, CType: Copy>, E: $unchecked<'d, CType: Copy>>
            $unchecked<'d> for Result<T, E>
        {
            type SoftStore = core::option::Option<core::result::Result<T::SoftStore, E::SoftStore>>;

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::SoftStore,
            ) -> Self {
                match unsafe { source.into_result().unwrap_unchecked() } {
                    Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        Self::Ok(unsafe {
                            <T as $unchecked<'d>>::soft_decode_unchecked(value, inner)
                        })
                    }
                    Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        Self::Err(unsafe {
                            <E as $unchecked<'d>>::soft_decode_unchecked(value, inner)
                        })
                    }
                }
            }
        }
        unsafe impl<'d, L: Language, T: $decode<'d, L, CType: Copy>, E: $decode<'d, L, CType: Copy>>
            $decode<'d, L> for Result<T, E>
        {
            type Store = core::option::Option<core::result::Result<T::Store, E::Store>>;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                match source.into_result()? {
                    Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        Some(Self::Ok(unsafe {
                            <T as $decode<'d, L>>::soft_decode(value, inner)?
                        }))
                    }
                    Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        Some(Self::Err(unsafe {
                            <E as $decode<'d, L>>::soft_decode(value, inner)?
                        }))
                    }
                }
            }
        }
    };
}
impl_result_decode!(DecodeMove, DecodeMoveUnchecked);
impl_result_decode!(BorrowedDecode, BorrowedDecodeUnchecked);

#[cfg(feature = "repr-rust")]
unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind>
    SoftEncodeRefUnchecked<'a> for Result<T, E>
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    &'a E: EncodeMoveUnchecked<CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
{
    type SoftStore = RefSizedEncodeStore<
        Self,
        Option<
            core::result::Result<
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
        store.encode(|fields| unsafe {
            encode_tagged_result!(self, crate::stored::encode_ref_field_unchecked, fields)
        })
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<
    'a,
    L: Language,
    T: ReprC<CType: Copy> + 'a,
    E: ReprC<CType: Copy> + 'a,
    __Interior: ExclusiveKind,
> SoftEncodeRef<'a, L> for Result<T, E>
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<L, CType: RefCarrier<Pointee = <T as ReprC>::CType>>,
    &'a E: EncodeMove<L, CType: RefCarrier<Pointee = <E as ReprC>::CType>>,
{
    type Store = RefSizedEncodeStore<
        Self,
        core::option::Option<
            core::result::Result<<&'a T as EncodeMove<L>>::Store, <&'a E as EncodeMove<L>>::Store>,
        >,
    >;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            encode_tagged_result!(self, crate::stored::encode_ref_field::<_, L>, fields)
        })
    }
}

impl<T: EncodeUnchecked<CType: Copy>, E: EncodeUnchecked<CType: Copy>> EncodeUnchecked
    for Result<T, E>
{
}
impl<L: Language, T: Encode<L, CType: Copy>, E: Encode<L, CType: Copy>> Encode<L> for Result<T, E> {}
impl<'d, T: DecodeUnchecked<'d, CType: Copy>, E: DecodeUnchecked<'d, CType: Copy>>
    DecodeUnchecked<'d> for Result<T, E>
{
}
impl<'d, L: Language, T: Decode<'d, L, CType: Copy>, E: Decode<'d, L, CType: Copy>> Decode<'d, L>
    for Result<T, E>
{
}

unsafe impl<T: AbiCompatible<CType: Copy>, E: AbiCompatible<CType: Copy>> AbiCompatible
    for Result<T, E>
where
    Self: crate::rust_spec::RustSpec<Layout = crate::rust_spec::Specified>,
{
}
unsafe impl<L: Language, T: AbiValidate<L, CType: Copy>, E: AbiValidate<L, CType: Copy>>
    AbiValidate<L> for Result<T, E>
where
    Self: crate::rust_spec::RustSpec<Layout = crate::rust_spec::Specified>,
{
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag() {
            0 => unsafe {
                <T as crate::convert::AbiValidate<L>>::is_valid(target.ok.1.assume_init_ref())
            },
            1 => unsafe {
                <E as crate::convert::AbiValidate<L>>::is_valid(target.err.1.assume_init_ref())
            },
            _ => false,
        }
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> crate::niche::Niche for Result<T, E> {
    const NICHE: Self::CType = abi::Result::niche();

    fn is_niche(value: &Self::CType) -> bool {
        value.is_niche()
    }
}

/// A transparent `Ok` payload with no Rust result discriminant.
/// Lowers to `abi::ResultOk<T::CType, ()>` and preserves the payload's validity requirements.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub struct ResultOk<T>(pub T);

/// A transparent `Err` payload with no Rust result discriminant.
/// Lowers to `abi::ResultErr<(), E::CType>` and preserves the payload's validity requirements.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub struct ResultErr<E>(pub E);

crate::option::impl_niche_payload!(ResultOk => abi::ResultOk<T::CType, ()>);
crate::option::impl_niche_payload!(ResultErr => abi::ResultErr<(), T::CType>);

impl<T, E> From<Result<T, E>> for core::result::Result<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Result::Ok(value) => Self::Ok(value),
            Result::Err(value) => Self::Err(value),
        }
    }
}

unsafe impl<T: EmptyStore, E: EmptyStore> EmptyStore for core::result::Result<T, E> {}

unsafe impl<T, E> CType for core::result::Result<T, E> where
    Self: RustSpec<Layout = rust_spec::Specified, Trap = rust_spec::layout::Robust, Drop = NoDrop>
{
}

unsafe impl<T, E> CFnArg for core::result::Result<T, E> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T, E> CFnReturn for core::result::Result<T, E> where
    Self: CType + crate::abi::CFnReturnLayout
{
}

impl<R: EncodeUnchecked, E: EncodeUnchecked> EncodeUnchecked for core::result::Result<R, E> where
    Self: EncodeMoveUnchecked
{
}
impl<L: Language, R: Encode<L>, E: Encode<L>> Encode<L> for core::result::Result<R, E> where
    Self: EncodeMove<L>
{
}
impl<'d, R: DecodeUnchecked<'d>, E: DecodeUnchecked<'d>> DecodeUnchecked<'d>
    for core::result::Result<R, E>
where
    Self: DecodeMoveUnchecked<'d>,
{
}
impl<'d, L: Language, R: Decode<'d, L>, E: Decode<'d, L>> Decode<'d, L>
    for core::result::Result<R, E>
where
    Self: DecodeMove<'d, L>,
{
}

impl<R: ReprC<CType: Copy>, E: ReprC<CType: Copy>> crate::niche::Niche
    for core::result::Result<R, E>
where
    Self: ReprC<CType = Native<abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
{
    const NICHE: Self::CType = Native(abi::Result::niche());

    fn is_niche(value: &Self::CType) -> bool {
        value.is_niche()
    }
}

#[doc(hidden)]
pub mod abi {
    use crate::lang::Language;

    macro_rules! niche_result_carrier {
    ($name:ident, $payload:ident, $other:ident, [$($generics:tt)*], $description:literal) => {
        #[doc = $description]
        /// CO3 sources select this carrier directly.
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
        unsafe impl<L: Language, $($generics)*> EncodeMove<L> for $name<Ok, Err> {
            type Store = ();

            fn soft_encode<'a>(self, _: &'a mut ()) -> Self::CType
            where
                Self: 'a,
            {
                self
            }
        }
        unsafe impl<'d,  $($generics)*> DecodeMoveUnchecked<'d> for $name<Ok, Err> {
            type SoftStore = ();

            unsafe fn soft_decode_unchecked<'a: 'd>(source: Self::CType, _: &'a mut ()) -> Self { {
                source
            } }
        }
        unsafe impl<'d, L: Language, $($generics)*> DecodeMove<'d, L> for $name<Ok, Err> {
            type Store = ();

            unsafe fn soft_decode<'a: 'd>(source: Self::CType, _: &'a mut ()) -> Option<Self> {
                Some(source)
            }
        }
        impl<$($generics)*> EncodeUnchecked for $name<Ok, Err> {}
        impl<L: Language, $($generics)*> Encode<L> for $name<Ok, Err> {}
        impl<'d,  $($generics)*> DecodeUnchecked<'d> for $name<Ok, Err> {}
        impl<'d, L: Language, $($generics)*> Decode<'d, L> for $name<Ok, Err> {}
        unsafe impl<$($generics)*> AbiCompatible for $name<Ok, Err>
        where
            Self: $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>,
        {}
        unsafe impl<L: $crate::lang::Language, $($generics)*> AbiValidate<L> for $name<Ok, Err>
        where
            Self: $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>,
        {
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

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
    /// CO3 sources select this carrier directly; native tagged or container sources wrap it in `Native`.
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
        pub const fn new(value: core::result::Result<T, E>) -> Self {
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
            payload: core::result::Result<MaybeUninit<T>, MaybeUninit<E>>,
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
        pub const fn into_result(self) -> core::option::Option<core::result::Result<T, E>> {
            match self.tag() {
                0 => Some(Ok(unsafe { self.ok.1.assume_init() })),
                1 => Some(Err(unsafe { self.err.1.assume_init() })),
                _ => None,
            }
        }

        /// Borrow only the active payload, rejecting invalid tags.
        #[cfg(feature = "repr-rust")]
        pub(crate) const fn as_result(&self) -> core::option::Option<core::result::Result<&T, &E>> {
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

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::abi::Result as AbiResult;

    const CONST_RESULT: Option<Result<u32, u16>> = AbiResult::<u32, u16>::Err(4).into_result();

    #[test]
    fn tagged_result_carrier_preserves_active_payloads() {
        assert_eq!(CONST_RESULT, Some(Err(4)));
        assert_eq!(AbiResult::<u32, u16>::new(Ok(7)).into_result(), Some(Ok(7)));
        assert_eq!(
            AbiResult::<u32, u16>::new(Err(4)).into_result(),
            Some(Err(4))
        );
        let result = unsafe { AbiResult::<u32, u16>::from_raw_parts(2, Ok(MaybeUninit::uninit())) };
        assert!(result != result);
        assert_eq!(result.partial_cmp(&result), None);
        assert_eq!(result.into_result(), None);
        let result = unsafe { AbiResult::<u32, u16>::from_raw_parts(1, Err(MaybeUninit::new(5))) };
        assert_eq!(result.into_result(), Some(Err(5)));
    }
}
