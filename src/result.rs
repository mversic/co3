//! C-compatible representation of [`core::result::Result`].
use core::{marker::PhantomData, mem::MaybeUninit, ops::Add, result::Result as RustResult};

use rust_spec::{
    RustSpec,
    drop::{DropKind, NoDrop},
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, EncodeUnchecked, ReprC, Thin,
    borrow::Borrow,
    niche::Niche,
    reference::CRef,
    stored::{
        DecodeMove, EmptyStore, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked,
    },
    sync::RefSizedEncodeStore,
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// A result with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

/// FFI-safe equivalent of [`core::result::Result`].
/// `T` and `E` name Rust source types; the union stores their C companions.
#[repr(C)]
pub union CResult<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> {
    ok: CResultVariant<T>,
    err: CResultVariant<E>,
}

macro_rules! niche_result_carrier {
    ($name:ident, $payload:ident, $other:ident, [$($generics:tt)*], $description:literal) => {
        #[doc = $description]
        #[repr(transparent)]
        #[derive(RustSpec)]
        pub struct $name<$($generics)*>(<$payload as ReprC>::CType, PhantomData<$other>);

        impl<$($generics)*> $name<Ok, Err> {
            pub(crate) const fn from_carrier(value: <$payload as ReprC>::CType) -> Self {
                Self(value, PhantomData)
            }
            pub(crate) const fn carrier(&self) -> &<$payload as ReprC>::CType { &self.0 }
            pub(crate) fn into_carrier(self) -> <$payload as ReprC>::CType { self.0 }
            /// Return whether the carrier contains the payload type's niche value.
            pub(crate) fn is_niche(&self) -> bool
            where
                $payload: Niche,
            {
                <$payload as Niche>::is_niche(&self.0)
            }
        }

        impl<$($generics)*> ReprC for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            type CType = Self;
        }
        unsafe impl<$($generics)*> CType for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {}
        unsafe impl<$($generics)*> CFnArg for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>, <$payload as ReprC>::CType: CFnArg {}
        unsafe impl<$($generics)*> CFnReturn for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>, <$payload as ReprC>::CType: CFnReturn {}

        unsafe impl<$($generics)*> Borrow for $name<Ok, Err> where $payload: Niche, $other: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>> {
            type Borrowed = Self;

            fn borrow(self) -> Self {
                self
            }

            fn from_borrow(source: Self) -> Self {
                source
            }
        }

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

impl<Ok: Niche<CType: Sized>, Err: ReprC> CResultOk<Ok, Err>
where
    Err: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
{
    /// Constructs a carrier when `R` has a niche and `E` is zero-sized.
    pub const fn new(value: Ok::CType) -> Self {
        Self::from_carrier(value)
    }
}

impl<Ok: ReprC, Err: Niche<CType: Sized>> CResultErr<Ok, Err>
where
    Ok: RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
{
    /// Constructs a carrier when `E` has a niche and `R` is zero-sized.
    pub const fn new(value: Err::CType) -> Self {
        Self::from_carrier(value)
    }
}

#[repr(C)]
struct CResultVariant<T: ReprC<CType: Copy>>(u8, MaybeUninit<T::CType>);

impl<T: ReprC<CType: Copy>> Copy for CResultVariant<T> {}
impl<T: ReprC<CType: Copy>> Clone for CResultVariant<T> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T: Borrow, E: Borrow> Borrow for Result<T, E> {
    type Borrowed = Result<T::Borrowed, E::Borrowed>;
    fn borrow(self) -> Self::Borrowed {
        match self {
            Self::Ok(v) => Result::Ok(v.borrow()),
            Self::Err(v) => Result::Err(v.borrow()),
        }
    }
    fn from_borrow(source: Self::Borrowed) -> Self {
        match source {
            Result::Ok(v) => Self::Ok(T::from_borrow(v)),
            Result::Err(v) => Self::Err(E::from_borrow(v)),
        }
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> ReprC for Result<T, E> {
    type CType = CResult<T, E>;
}

unsafe impl<T: EncodeMoveUnchecked<CType: Copy>, E: EncodeMoveUnchecked<CType: Copy>>
    EncodeMoveUnchecked for Result<T, E>
{
    type SoftStore = Option<RustResult<T::SoftStore, E::SoftStore>>;

    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
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

unsafe impl<T: EncodeMove<CType: Copy>, E: EncodeMove<CType: Copy>> EncodeMove for Result<T, E> {
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
                CResult::Ok(value.soft_encode(inner))
            }
            Self::Err(value) => {
                let Err(inner) = store.insert(Err(Default::default())) else {
                    unreachable!()
                };
                CResult::Err(value.soft_encode(inner))
            }
        }
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind>
    EncodeRefUnchecked<'a> for Result<T, E>
where
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType = CRef<T>>,
    &'a E: EncodeMoveUnchecked<CType = CRef<E>>,
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

    unsafe fn encode_ref_unchecked<'itm>(
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
    EncodeRef<'a> for Result<T, E>
where
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType = CRef<T>>,
    &'a E: EncodeMove<CType = CRef<E>>,
{
    type Store = RefSizedEncodeStore<
        Self,
        Option<RustResult<<&'a T as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
    >;

    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
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

unsafe impl<'d, T: DecodeMove<'d, CType: Copy>, E: DecodeMove<'d, CType: Copy>> DecodeMove<'d>
    for Result<T, E>
{
    type Store = Option<RustResult<T::Store, E::Store>>;

    unsafe fn soft_decode<'a: 'd>(source: Self::CType, store: &'a mut Self::Store) -> Option<Self> {
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

impl<T: ReprC<CType: core::fmt::Debug + Copy>, E: ReprC<CType: core::fmt::Debug + Copy>>
    core::fmt::Debug for CResult<T, E>
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

impl<T: ReprC<CType: PartialEq + Copy>, E: ReprC<CType: PartialEq + Copy>> PartialEq
    for CResult<T, E>
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

impl<T: ReprC<CType: PartialOrd + Copy>, E: ReprC<CType: PartialOrd + Copy>> PartialOrd
    for CResult<T, E>
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

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> CResult<T, E> {
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

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> CResult<T, E> {
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

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> Copy for CResult<T, E> {}
impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> Clone for CResult<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T: ReprC<CType: RustSpec + Copy>, E: ReprC<CType: RustSpec + Copy>> RustSpec
    for CResult<T, E>
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

unsafe impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> Borrow for CResult<T, E> {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }
    fn from_borrow(source: Self) -> Self {
        source
    }
}

crate::wide::impl_identity_carrier! { CResult, [T: ReprC<CType: Copy>, E: ReprC<CType: Copy>], [T, E] }

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
