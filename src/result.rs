//! C-compatible representation of [`core::result::Result`].
use core::result::Result as CoreResult;
use core::{mem::MaybeUninit, ops::Add};

use rust_spec::{RustSpec, mutability::Exclusive, niche::WithoutNiche};

use crate::EncodeUnchecked;
use crate::sync::RefSizedEncodeStore;
use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Thin,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
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
    type Borrowed<'a>
        = Result<T::Borrowed<'a>, E::Borrowed<'a>>
    where
        Self: 'a;
    type Owner = Option<CoreResult<T::Owner, E::Owner>>;

    fn borrow<'a>(self, owner: &'a mut Self::Owner) -> Self::Borrowed<'a>
    where
        Self: 'a,
    {
        match self {
            Self::Ok(value) => {
                let Ok(inner) = owner.insert(Ok(Default::default())) else {
                    unreachable!()
                };
                Result::Ok(value.borrow(inner))
            }
            Self::Err(value) => {
                let Err(inner) = owner.insert(Err(Default::default())) else {
                    unreachable!()
                };
                Result::Err(value.borrow(inner))
            }
        }
    }
}

impl<'a, T: FromBorrow<'a>, E: FromBorrow<'a>> FromBorrow<'a> for Result<T, E> {
    fn from_borrow(value: Self::Borrowed<'a>) -> Self {
        match value {
            Result::Ok(value) => Self::Ok(T::from_borrow(value)),
            Result::Err(value) => Self::Err(E::from_borrow(value)),
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

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a> EncodeRefUnchecked<'a>
    for Result<T, E>
where
    Self: RustSpec<Mutability = Exclusive, Size: Thin> + 'a,
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

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, E: ReprC<CType: Copy> + 'a> EncodeRef<'a>
    for Result<T, E>
where
    Self: RustSpec<Mutability = Exclusive, Size: Thin> + 'a,
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

impl<T: Copy, E: Copy> CResult<T, E> {
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

    #[inline(always)]
    fn forward_payload<U: Copy, V: Copy>(self) -> CResult<U, V> {
        let mut output = MaybeUninit::<CResult<U, V>>::zeroed();

        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::from_ref(&self).cast::<u8>(),
                output.as_mut_ptr().cast::<u8>(),
                core::cmp::min(size_of::<Self>(), size_of::<CResult<U, V>>()),
            );

            output.assume_init()
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
    T: RustSpec<Drop: Add<E::Drop, Output: rust_spec::drop::DropKind>>,
    T::Alignment: rust_spec::Max<E::Alignment>,
    T::Mutability: Add<E::Mutability>,
    T::__IndirectTrap: Add<E::__IndirectTrap>,
{
    type Layout = <T::Layout as Add<E::Layout>>::Output;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type Alignment = <T::Alignment as rust_spec::Max<E::Alignment>>::Output;
    type Trap = <T::Trap as Add<E::Trap>>::Output;
    type Niche = WithoutNiche;
    type Drop = <T::Drop as Add<E::Drop>>::Output;
    type Mutability = <T::Mutability as Add<E::Mutability>>::Output;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = <T::__IndirectTrap as Add<E::__IndirectTrap>>::Output;
}

unsafe impl<T: Borrow + Copy, E: Borrow + Copy> Borrow for CResult<T, E>
where
    for<'itm> T::Borrowed<'itm>: Copy,
    for<'itm> E::Borrowed<'itm>: Copy,
{
    type Borrowed<'itm>
        = CResult<T::Borrowed<'itm>, E::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = Option<CoreResult<T::Owner, E::Owner>>;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self.tag() {
            0 => {
                let CoreResult::Ok(owner) = owner.insert(CoreResult::Ok(Default::default())) else {
                    unreachable!()
                };

                CResult::Ok(unsafe { self.ok.1.assume_init() }.borrow(owner))
            }
            1 => {
                let CoreResult::Err(owner) = owner.insert(CoreResult::Err(Default::default()))
                else {
                    unreachable!()
                };

                CResult::Err(unsafe { self.err.1.assume_init() }.borrow(owner))
            }
            _ => self.forward_payload(),
        }
    }
}
impl<'itm, T: FromBorrow<'itm> + Copy, E: FromBorrow<'itm> + Copy> FromBorrow<'itm>
    for CResult<T, E>
where
    for<'borrow> T::Borrowed<'borrow>: Copy,
    for<'borrow> E::Borrowed<'borrow>: Copy,
{
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        match source.tag() {
            0 => Self::Ok(T::from_borrow(unsafe { source.ok.1.assume_init() })),
            1 => Self::Err(E::from_borrow(unsafe { source.err.1.assume_init() })),
            _ => source.forward_payload(),
        }
    }
}

impl<T: ReprC<CType: Copy> + Copy, E: ReprC<CType: Copy> + Copy> ReprC for CResult<T, E> {
    type CType = CResult<T::CType, E::CType>;
}
unsafe impl<T: EncodeMove<CType: Copy> + Copy, E: EncodeMove<CType: Copy> + Copy> EncodeMove
    for CResult<T, E>
where
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
{
    type Store = Option<CoreResult<T::Store, E::Store>>;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        match self.tag() {
            0 => {
                let CoreResult::Ok(store) = store.insert(CoreResult::Ok(Default::default())) else {
                    unreachable!()
                };

                CResult::Ok(unsafe { self.ok.1.assume_init() }.soft_encode(store))
            }
            1 => {
                let CoreResult::Err(store) = store.insert(CoreResult::Err(Default::default()))
                else {
                    unreachable!()
                };

                CResult::Err(unsafe { self.err.1.assume_init() }.soft_encode(store))
            }
            _ => self.forward_payload(),
        }
    }
}
unsafe impl<T: EncodeMoveUnchecked<CType: Copy> + Copy, E: EncodeMoveUnchecked<CType: Copy> + Copy>
    EncodeMoveUnchecked for CResult<T, E>
where
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
{
    type SoftStore = Option<CoreResult<T::SoftStore, E::SoftStore>>;

    unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
        match self.tag() {
            0 => {
                let CoreResult::Ok(inner) = store.insert(Ok(Default::default())) else {
                    unreachable!()
                };

                CResult::Ok(unsafe { self.ok.1.assume_init().soft_encode_unchecked(inner) })
            }
            1 => {
                let CoreResult::Err(inner) = store.insert(Err(Default::default())) else {
                    unreachable!()
                };

                CResult::Err(unsafe { self.err.1.assume_init().soft_encode_unchecked(inner) })
            }
            _ => {
                *store = None;
                self.forward_payload()
            }
        }
    }
}
unsafe impl<'a, T: ReprC<CType: Copy> + Copy, E: ReprC<CType: Copy> + Copy> EncodeRef<'a>
    for CResult<T, E>
where
    Self: RustSpec<Mutability = Exclusive, Size: Thin> + 'a,
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
    &'a T: EncodeMove<CType = CRef<<T as ReprC>::CType>>,
    &'a E: EncodeMove<CType = CRef<<E as ReprC>::CType>>,
{
    type Store = crate::sync::RefSizedEncodeStore<
        Self,
        Option<CoreResult<<&'a T as EncodeMove>::Store, <&'a E as EncodeMove>::Store>>,
    >;
    fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
        store.encode(|fields| {
            let store = fields;
            match self.tag() {
                0 => {
                    let CoreResult::Ok(store) = store.insert(CoreResult::Ok(Default::default()))
                    else {
                        unreachable!()
                    };

                    CResult::Ok(crate::stored::encode_ref_field(
                        unsafe { self.ok.1.assume_init_ref() },
                        store,
                    ))
                }
                1 => {
                    let CoreResult::Err(store) = store.insert(CoreResult::Err(Default::default()))
                    else {
                        unreachable!()
                    };

                    CResult::Err(crate::stored::encode_ref_field(
                        unsafe { self.err.1.assume_init_ref() },
                        store,
                    ))
                }
                _ => {
                    *store = None;
                    self.forward_payload()
                }
            }
        })
    }
}
unsafe impl<'a, T: ReprC<CType: Copy> + Copy, E: ReprC<CType: Copy> + Copy> EncodeRefUnchecked<'a>
    for CResult<T, E>
where
    Self: RustSpec<Mutability = Exclusive, Size: Thin> + 'a,
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
    &'a T: EncodeMoveUnchecked<CType = CRef<<T as ReprC>::CType>>,
    &'a E: EncodeMoveUnchecked<CType = CRef<<E as ReprC>::CType>>,
{
    type SoftStore = crate::sync::RefSizedEncodeStore<
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
        store.encode(|fields| {
            let store = fields;
            match self.tag() {
                0 => {
                    let CoreResult::Ok(inner) = store.insert(Ok(Default::default())) else {
                        unreachable!()
                    };

                    CResult::Ok(unsafe {
                        crate::stored::encode_ref_field_unchecked(
                            self.ok.1.assume_init_ref(),
                            inner,
                        )
                    })
                }
                1 => {
                    let CoreResult::Err(inner) = store.insert(Err(Default::default())) else {
                        unreachable!()
                    };

                    CResult::Err(unsafe {
                        crate::stored::encode_ref_field_unchecked(
                            self.err.1.assume_init_ref(),
                            inner,
                        )
                    })
                }
                _ => {
                    *store = None;
                    self.forward_payload()
                }
            }
        })
    }
}

unsafe impl<'d, T: DecodeOwned<'d, CType: Copy> + Copy, E: DecodeOwned<'d, CType: Copy> + Copy>
    DecodeOwned<'d> for CResult<T, E>
where
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
{
    type Store = Option<CoreResult<T::Store, E::Store>>;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        match source.tag() {
            0 => {
                let CoreResult::Ok(store) = store.insert(CoreResult::Ok(Default::default())) else {
                    unreachable!()
                };

                Some(Self::Ok(unsafe {
                    T::soft_decode(source.ok.1.assume_init(), store)?
                }))
            }
            1 => {
                let CoreResult::Err(store) = store.insert(CoreResult::Err(Default::default()))
                else {
                    unreachable!()
                };

                Some(Self::Err(unsafe {
                    E::soft_decode(source.err.1.assume_init(), store)?
                }))
            }
            _ => Some(source.forward_payload()),
        }
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        match source.tag() {
            0 => {
                let CoreResult::Ok(store) = store.insert(CoreResult::Ok(Default::default())) else {
                    unreachable!()
                };
                Self::Ok(unsafe { T::soft_decode_unchecked(source.ok.1.assume_init(), store) })
            }
            1 => {
                let CoreResult::Err(store) = store.insert(CoreResult::Err(Default::default()))
                else {
                    unreachable!()
                };
                Self::Err(unsafe { E::soft_decode_unchecked(source.err.1.assume_init(), store) })
            }
            _ => source.forward_payload(),
        }
    }
}

impl<T: Encode<CType: Copy> + Copy, E: Encode<CType: Copy> + Copy> Encode for CResult<T, E> where
    Self: ReprC<CType = CResult<T::CType, E::CType>>
{
}
impl<T: EncodeUnchecked<CType: Copy> + Copy, E: EncodeUnchecked<CType: Copy> + Copy> EncodeUnchecked
    for CResult<T, E>
where
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
{
}
impl<'d, T: Decode<'d, CType: Copy> + Copy, E: Decode<'d, CType: Copy> + Copy> Decode<'d>
    for CResult<T, E>
where
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
{
}

unsafe impl<T: AbiCompatible<CType: Copy> + Copy, E: AbiCompatible<CType: Copy> + Copy>
    AbiCompatible for CResult<T, E>
where
    Self: ReprC<CType = CResult<T::CType, E::CType>>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag() {
            0 => unsafe { T::is_valid(&*target.ok.1.as_ptr()) },
            1 => unsafe { E::is_valid(&*target.err.1.as_ptr()) },
            _ => true,
        }
    }
}

unsafe impl<T: CType + Copy, E: CType + Copy> CType for CResult<T, E> {}
unsafe impl<T: CType + Copy, E: CType + Copy> CFnArg for CResult<T, E> {}
unsafe impl<T: CType + Copy, E: CType + Copy> CFnReturn for CResult<T, E> {}

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
