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
    convert::{
        Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
        SoftEncodeRef, SoftEncodeRefUnchecked,
    },
    niche::Niche,
    sync::RefSizedEncodeStore,
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// A transparent `Ok` payload with no Rust result discriminant.
/// Lowers to `ResultOk<T::CType>` and preserves the payload's validity requirements.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub struct ResultOk<T>(pub T);

/// A transparent `Err` payload with no Rust result discriminant.
/// Lowers to `ResultErr<E::CType>` and preserves the payload's validity requirements.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub struct ResultErr<E>(pub E);

crate::option::impl_niche_payload!(ResultOk);
crate::option::impl_niche_payload!(ResultErr);

/// A result with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

/// FFI-safe equivalent of [`core::result::Result`].
/// `T` and `E` name Rust source types; the union stores their C companions.
/// `STABLE` identifies the co3 source family (`true`) or native Rust family (`false`).
#[repr(C)]
#[doc(hidden)]
pub union CResult<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool = false> {
    ok: CResultVariant<T>,
    err: CResultVariant<E>,
}

macro_rules! niche_result_carrier {
    ($name:ident, $payload:ident, $other:ident, [$($generics:tt)*], $description:literal) => {
        #[doc = $description]
        #[repr(transparent)]
        #[derive(RustSpec)]
        #[doc(hidden)]
        pub struct $name<$($generics)*>(pub <$payload as ReprC>::CType, pub PhantomData<$other>);

        impl<$($generics)*> ReprC for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            type CType = Self;
        }
        unsafe impl<$($generics)*> CType for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {}
        unsafe impl<$($generics)*> CFnArg for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>, <$payload as ReprC>::CType: CFnArg {}
        unsafe impl<$($generics)*> CFnReturn for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>, <$payload as ReprC>::CType: CFnReturn {}

        crate::__impl_borrowed_identity!([$($generics)*] $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>);

        unsafe impl<$($generics)*> EncodeMoveUnchecked for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self::CType
            where
                Self: 'a,
            {
                self
            }
        }
        unsafe impl<$($generics)*> EncodeMove for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            type Store = ();

            fn soft_encode<'a>(self, _: &'a mut ()) -> Self::CType
            where
                Self: 'a,
            {
                self
            }
        }
        unsafe impl<'d, $($generics)*> DecodeMove<'d> for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            type Store = ();

            unsafe fn soft_decode<'a: 'd>(source: Self::CType, _: &'a mut ()) -> Option<Self> {
                Some(source)
            }
        }
        impl<$($generics)*> Encode for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {}
        impl<$($generics)*> EncodeUnchecked for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {}
        impl<'d, $($generics)*> Decode<'d> for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {}
        unsafe impl<$($generics)*> AbiCompatibleValidate for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }
        unsafe impl<$($generics)*> AbiCompatible for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {}

        impl<$($generics)*> Copy for $name<Ok, Err> where <$payload as ReprC>::CType: Copy {}
        impl<$($generics)*> Clone for $name<Ok, Err> where <$payload as ReprC>::CType: Clone {
            fn clone(&self) -> Self {
                Self(self.0.clone(), PhantomData)
            }
        }
        impl<$($generics)*> core::fmt::Debug for $name<Ok, Err> where <$payload as ReprC>::CType: core::fmt::Debug {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }

        impl<$($generics)*> Eq for $name<Ok, Err> where <$payload as ReprC>::CType: Eq {}
        impl<$($generics)*> PartialEq for $name<Ok, Err> where <$payload as ReprC>::CType: PartialEq {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl<$($generics)*> Ord for $name<Ok, Err> where <$payload as ReprC>::CType: Ord {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.0.cmp(&other.0)
            }
        }
        impl<$($generics)*> PartialOrd for $name<Ok, Err> where <$payload as ReprC>::CType: PartialOrd {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                self.0.partial_cmp(&other.0)
            }
        }
        impl<$($generics)*> core::hash::Hash for $name<Ok, Err> where <$payload as ReprC>::CType: core::hash::Hash {
            fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                self.0.hash(state);
            }
        }
    };
}

niche_result_carrier!(
    CResultOk,
    Ok,
    Err,
    [Ok: ReprC<CType: Sized>, Err: ReprC],
    "Carrier for a native `Result<T, E>` whose `Ok` value has a niche representing `Err`."
);
niche_result_carrier!(
    CResultErr,
    Err,
    Ok,
    [Ok: ReprC, Err: ReprC<CType: Sized>],
    "Carrier for a native `Result<T, E>` whose `Err` value has a niche representing `Ok`."
);

#[repr(C)]
struct CResultVariant<T: ReprC<CType: Copy>>(u8, MaybeUninit<T::CType>);

impl<T: ReprC<CType: Copy>> Copy for CResultVariant<T> {}
impl<T: ReprC<CType: Copy>> Clone for CResultVariant<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> ReprC for Result<T, E> {
    type CType = CResult<T, E, true>;
}

macro_rules! result_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<T: $unchecked<CType: Copy>, E: $unchecked<CType: Copy>> $unchecked
            for Result<T, E>
        {
            type SoftStore = Option<RustResult<T::SoftStore, E::SoftStore>>;

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
                        CResult::Ok(unsafe { value.soft_encode_unchecked(inner) })
                    }
                    Self::Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        CResult::Err(unsafe { value.soft_encode_unchecked(inner) })
                    }
                }
            }
        }
        unsafe impl<T: $encode<CType: Copy>, E: $encode<CType: Copy>> $encode for Result<T, E> {
            type Store = Option<RustResult<T::Store, E::Store>>;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                match self {
                    Self::Ok(value) => {
                        let Ok(inner) = store.insert(Ok(Default::default())) else {
                            unreachable!()
                        };
                        CResult::Ok($encode::soft_encode(value, inner))
                    }
                    Self::Err(value) => {
                        let Err(inner) = store.insert(Err(Default::default())) else {
                            unreachable!()
                        };
                        CResult::Err($encode::soft_encode(value, inner))
                    }
                }
            }
        }
        unsafe impl<'d, T: $decode<'d, CType: Copy>, E: $decode<'d, CType: Copy>> $decode<'d>
            for Result<T, E>
        {
            type Store = Option<RustResult<T::Store, E::Store>>;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                match source.into_result()? {
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
                match unsafe { source.into_result().unwrap_unchecked() } {
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
        type Owner = Option<RustResult<T::Owner, E::Owner>>;
        type SoftStore = Option<RustResult<T::SoftStore, E::SoftStore>>;

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
                    CResult::Ok(unsafe { value.soft_encode_unchecked(inner, inner_owner) })
                }
                Self::Err(value) => {
                    let Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let Err(inner_owner) = owner.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    CResult::Err(unsafe { value.soft_encode_unchecked(inner, inner_owner) })
                }
            }
        }
    }
    unsafe impl<T: BorrowedEncode<CType: Copy>, E: BorrowedEncode<CType: Copy>> BorrowedEncode
        for Result<T, E>
    {
        type Store = Option<RustResult<T::Store, E::Store>>;

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
                    CResult::Ok(BorrowedEncode::soft_encode(value, inner, inner_owner))
                }
                Self::Err(value) => {
                    let Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    let Err(inner_owner) = owner.insert(Err(Default::default())) else {
                        unreachable!()
                    };
                    CResult::Err(BorrowedEncode::soft_encode(value, inner, inner_owner))
                }
            }
        }
    }
    unsafe impl<'d, T: BorrowedDecode<'d, CType: Copy>, E: BorrowedDecode<'d, CType: Copy>>
        BorrowedDecode<'d> for Result<T, E>
    {
        type Store = Option<RustResult<T::Store, E::Store>>;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Option<Self> {
            match source.into_result()? {
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
            match unsafe { source.into_result().unwrap_unchecked() } {
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
    &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
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
                CResult::Ok(unsafe { crate::stored::encode_ref_field_unchecked(value, inner) })
            }
            Self::Err(value) => {
                let Err(inner) = fields.insert(Err(Default::default())) else {
                    unreachable!()
                };
                CResult::Err(unsafe { crate::stored::encode_ref_field_unchecked(value, inner) })
            }
        })
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind>
    SoftEncodeRef<'a> for Result<T, E>
where
    &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType: crate::reference::RefCarrier<T>>,
    &'a E: EncodeMove<CType: crate::reference::RefCarrier<E>>,
{
    type Store = RefSizedEncodeStore<
        Self,
        Option<RustResult<<&'a T as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
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
                CResult::Ok(crate::stored::encode_ref_field(value, inner))
            }
            Self::Err(value) => {
                let Err(inner) = fields.insert(Err(Default::default())) else {
                    unreachable!()
                };
                CResult::Err(crate::stored::encode_ref_field(value, inner))
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

impl<
    T: ReprC<CType: core::fmt::Debug + Copy>,
    E: ReprC<CType: core::fmt::Debug + Copy>,
    const STABLE: bool,
> core::fmt::Debug for CResult<T, E, STABLE>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag() {
            0 => f
                .debug_tuple("CResult::Ok")
                .field(unsafe { self.ok.1.assume_init_ref() })
                .finish(),
            1 => f
                .debug_tuple("CResult::Err")
                .field(unsafe { self.err.1.assume_init_ref() })
                .finish(),
            tag => f
                .debug_struct("CResult::<invalid>")
                .field("tag", &tag)
                .finish(),
        }
    }
}

impl<T: ReprC<CType: PartialEq + Copy>, E: ReprC<CType: PartialEq + Copy>, const STABLE: bool>
    PartialEq for CResult<T, E, STABLE>
{
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

impl<T: ReprC<CType: PartialOrd + Copy>, E: ReprC<CType: PartialOrd + Copy>, const STABLE: bool>
    PartialOrd for CResult<T, E, STABLE>
{
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

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool> CResult<T, E, STABLE> {
    pub(crate) fn with_source<const OTHER: bool>(self) -> CResult<T, E, OTHER> {
        // The source discriminator has no storage and does not affect layout.
        unsafe { crate::transmute::transmute_owned(self) }
    }

    pub(crate) const fn niche() -> Self {
        Self {
            ok: CResultVariant(2, MaybeUninit::zeroed()),
        }
    }

    /// Construct a carrier from a result payload.
    pub const fn new(value: RustResult<T::CType, E::CType>) -> Self {
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
        payload: RustResult<MaybeUninit<T::CType>, MaybeUninit<E::CType>>,
    ) -> Self {
        match payload {
            Ok(payload) => Self {
                ok: CResultVariant(tag, payload),
            },
            Err(payload) => Self {
                err: CResultVariant(tag, payload),
            },
        }
    }

    /// Construct the success value
    #[expect(non_snake_case)]
    pub const fn Ok(ok: T::CType) -> Self {
        Self {
            ok: CResultVariant(0, MaybeUninit::new(ok)),
        }
    }

    /// Construct the error value
    #[expect(non_snake_case)]
    pub const fn Err(err: E::CType) -> Self {
        Self {
            err: CResultVariant(1, MaybeUninit::new(err)),
        }
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool> CResult<T, E, STABLE> {
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
    pub const fn into_result(self) -> Option<RustResult<T::CType, E::CType>> {
        match self.tag() {
            0 => Some(Ok(unsafe { self.ok.1.assume_init() })),
            1 => Some(Err(unsafe { self.err.1.assume_init() })),
            _ => None,
        }
    }

    /// Borrow only the active payload, rejecting invalid tags.
    pub(crate) const fn as_result(&self) -> Option<RustResult<&T::CType, &E::CType>> {
        match self.tag() {
            0 => Some(Ok(unsafe { self.ok.1.assume_init_ref() })),
            1 => Some(Err(unsafe { self.err.1.assume_init_ref() })),
            _ => None,
        }
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool> Copy
    for CResult<T, E, STABLE>
{
}
impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool> Clone
    for CResult<T, E, STABLE>
{
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T: ReprC<CType: RustSpec + Copy>, E: ReprC<CType: RustSpec + Copy>, const STABLE: bool>
    RustSpec for CResult<T, E, STABLE>
where
    T::CType: RustSpec<
            Layout: Add<<E::CType as RustSpec>::Layout>,
            Trap: Add<<E::CType as RustSpec>::Trap>,
        >,
    T::CType: RustSpec<Drop: Add<<E::CType as RustSpec>::Drop, Output: DropKind>>,
    <T::CType as RustSpec>::__Alignment: rust_spec::Max<<E::CType as RustSpec>::__Alignment>,
    <T::CType as RustSpec>::Mutability: Add<<E::CType as RustSpec>::Mutability>,
    <T::CType as RustSpec>::__IndirectTrap: Add<<E::CType as RustSpec>::__IndirectTrap>,
{
    type Layout = <<T::CType as RustSpec>::Layout as Add<<E::CType as RustSpec>::Layout>>::Output;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = <<T::CType as RustSpec>::__Alignment as rust_spec::Max<
        <E::CType as RustSpec>::__Alignment,
    >>::Output;
    type Trap = <<T::CType as RustSpec>::Trap as Add<<E::CType as RustSpec>::Trap>>::Output;
    type Niche = WithoutNiche;
    type Drop = <<T::CType as RustSpec>::Drop as Add<<E::CType as RustSpec>::Drop>>::Output;
    type Mutability =
        <<T::CType as RustSpec>::Mutability as Add<<E::CType as RustSpec>::Mutability>>::Output;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = <<T::CType as RustSpec>::__IndirectTrap as Add<
        <E::CType as RustSpec>::__IndirectTrap,
    >>::Output;
}

crate::__impl_borrowed_identity!([T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool] CResult<T, E, STABLE>);

crate::wide::impl_identity_carrier! { CResult, [T: ReprC<CType: Copy>, E: ReprC<CType: Copy>, const STABLE: bool], [T, E, STABLE] }

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
