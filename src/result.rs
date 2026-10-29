//! C-compatible representation of [`core::result::Result`].
use core::result::Result as CoreResult;
use core::{mem::MaybeUninit, ops::Add};

use rust_spec::drop::NoDrop;
use rust_spec::{
    RustSpec,
    drop::DropKind,
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::EncodeUnchecked;
use crate::sync::RefSizedEncodeStore;
use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Thin,
    borrow::{Borrow, BorrowCast, BorrowCastMut},
    reference::CRef,
    stored::{
        DecodeOwned, EmptyStore, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked,
    },
    transmute::AbiCompatible,
};

/// A result with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(RustSpec, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

/// FFI-safe equivalent of [`core::result::Result`]
#[repr(C)]
pub union CResult<T: Copy, E: Copy> {
    ok: CResultOk<T>,
    err: CResultErr<E>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CResultOk<T: Copy>(u8, MaybeUninit<T>);

#[repr(C)]
#[derive(Clone, Copy)]
struct CResultErr<E: Copy>(u8, MaybeUninit<E>);

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
    type CType = CResult<T::CType, E::CType>;
}

unsafe impl<T: EncodeMoveUnchecked<CType: Copy>, E: EncodeMoveUnchecked<CType: Copy>>
    EncodeMoveUnchecked for Result<T, E>
{
    type SoftStore = Option<CoreResult<T::SoftStore, E::SoftStore>>;

    unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
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
    type Store = Option<CoreResult<T::Store, E::Store>>;

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
    &'a T: EncodeMoveUnchecked<CType = CRef<T::CType>>,
    &'a E: EncodeMoveUnchecked<CType = CRef<E::CType>>,
{
    type SoftStore = RefSizedEncodeStore<
        Self,
        Option<
            CoreResult<
                <&'a T as EncodeMoveUnchecked>::SoftStore,
                <&'a E as EncodeMoveUnchecked>::SoftStore,
            >,
        >,
    >;

    unsafe fn encode_ref_unchecked(
        &'a self,
        store: &mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType {
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
    &'a T: EncodeMove<CType = CRef<T::CType>>,
    &'a E: EncodeMove<CType = CRef<E::CType>>,
{
    type Store = RefSizedEncodeStore<
        Self,
        Option<CoreResult<<&'a T as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
    >;

    fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
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

unsafe impl<'d, T: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d>
    for Result<T, E>
{
    type Store = Option<CoreResult<T::Store, E::Store>>;

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
        match unsafe { source.into_result_unchecked() } {
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

unsafe impl<T: AbiCompatible<CType: Copy>, E: AbiCompatible<CType: Copy>> AbiCompatible
    for Result<T, E>
{
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag() {
            0 => unsafe { T::is_valid(target.ok.1.assume_init_ref()) },
            1 => unsafe { E::is_valid(target.err.1.assume_init_ref()) },
            _ => false,
        }
    }
}

impl<T: core::fmt::Debug + Copy, E: core::fmt::Debug + Copy> core::fmt::Debug for CResult<T, E> {
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

impl<T: PartialEq + Copy, E: PartialEq + Copy> PartialEq for CResult<T, E> {
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
            (left, right) if left >= 2 && right >= 2 => left == right,
            _ => false,
        }
    }
}

impl<T: PartialOrd + Copy, E: PartialOrd + Copy> PartialOrd for CResult<T, E> {
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

impl<T: CType + Copy, E: CType + Copy> CResult<T, E> {
    pub(crate) const fn niche() -> Self {
        Self {
            ok: CResultOk(2, MaybeUninit::zeroed()),
        }
    }

    /// Construct the success value
    #[expect(non_snake_case)]
    pub const fn Ok(ok: T) -> Self {
        Self {
            ok: CResultOk(0, MaybeUninit::new(ok)),
        }
    }

    /// Construct the error value
    #[expect(non_snake_case)]
    pub const fn Err(err: E) -> Self {
        Self {
            err: CResultErr(1, MaybeUninit::new(err)),
        }
    }
}

impl<T: Copy, E: Copy> CResult<T, E> {
    /// Return the stored variant tag.
    #[inline(always)]
    pub fn tag(&self) -> u8 {
        // SAFETY: Both union variants place the tag at offset zero.
        unsafe { *core::ptr::from_ref(self).cast::<u8>() }
    }

    /// Return the represented `core::result::Result<T, E>`, or `None` for an invalid tag.
    pub fn into_result(self) -> Option<CoreResult<T, E>> {
        match self.tag() {
            0 => Some(Ok(unsafe { self.ok.1.assume_init() })),
            1 => Some(Err(unsafe { self.err.1.assume_init() })),
            _ => None,
        }
    }

    pub(crate) unsafe fn into_result_unchecked(self) -> CoreResult<T, E> {
        match self.tag() {
            0 => Ok(unsafe { self.ok.1.assume_init() }),
            1 => Err(unsafe { self.err.1.assume_init() }),
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }
}

impl<T: Copy, E: Copy> Copy for CResult<T, E> {}
impl<T: Copy, E: Copy> Clone for CResult<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T: RustSpec + Copy, E: RustSpec + Copy> RustSpec for CResult<T, E>
where
    T: RustSpec<Layout: Add<<E as RustSpec>::Layout>, Trap: Add<E::Trap>>,
    T: RustSpec<Drop: Add<E::Drop, Output: DropKind>>,
    T::__Alignment: rust_spec::Max<E::__Alignment>,
    T::Mutability: Add<E::Mutability>,
    T::__IndirectTrap: Add<E::__IndirectTrap>,
{
    type Layout = <T::Layout as Add<E::Layout>>::Output;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = <T::__Alignment as rust_spec::Max<E::__Alignment>>::Output;
    type Trap = <T::Trap as Add<E::Trap>>::Output;
    type Niche = WithoutNiche;
    type Drop = <T::Drop as Add<E::Drop>>::Output;
    type Mutability = <T::Mutability as Add<E::Mutability>>::Output;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = <T::__IndirectTrap as Add<E::__IndirectTrap>>::Output;
}

unsafe impl<T: CType + Copy, E: CType + Copy> Borrow for CResult<T, E> {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }
    fn from_borrow(source: Self) -> Self {
        source
    }
}

crate::slice::impl_identity_carrier! { CResult, [T: CType + Copy, E: CType + Copy], [T, E] }

unsafe impl<T: BorrowCast<AsConst: Copy> + Copy, E: BorrowCast<AsConst: Copy> + Copy> BorrowCast
    for CResult<T, E>
{
    type AsConst = CResult<T::AsConst, E::AsConst>;
}

unsafe impl<T: BorrowCastMut<AsMut: Copy> + Copy, E: BorrowCastMut<AsMut: Copy> + Copy>
    BorrowCastMut for CResult<T, E>
{
    type AsMut = CResult<T::AsMut, E::AsMut>;
}

unsafe impl<T: EmptyStore, E: EmptyStore> EmptyStore for CoreResult<T, E> {}

unsafe impl<T, E> CType for CoreResult<T, E> where
    Self: RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::Robust, Drop = NoDrop>
{
}

unsafe impl<T, E> CFnArg for CoreResult<T, E> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T, E> CFnReturn for CoreResult<T, E> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

unsafe impl<T, E> BorrowCast for CoreResult<T, E>
where
    Self: CType,
{
    type AsConst = Self;
}
unsafe impl<T, E> BorrowCastMut for CoreResult<T, E>
where
    Self: CType,
{
    type AsMut = Self;
}

impl<R: Encode, E: Encode> Encode for CoreResult<R, E> where Self: EncodeMove {}
impl<R: EncodeUnchecked, E: EncodeUnchecked> EncodeUnchecked for CoreResult<R, E> where
    Self: EncodeMoveUnchecked
{
}
impl<'d, R: Decode<'d>, E: Decode<'d>> Decode<'d> for CoreResult<R, E> where Self: DecodeOwned<'d> {}

impl<T, E> From<CoreResult<T, E>> for Result<T, E> {
    fn from(value: CoreResult<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(value) => Self::Err(value),
        }
    }
}
impl<T, E> From<Result<T, E>> for CoreResult<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Result::Ok(value) => Self::Ok(value),
            Result::Err(value) => Self::Err(value),
        }
    }
}
