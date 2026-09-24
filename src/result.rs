//! C-compatible representation of [`core::result::Result`].
use core::{mem::MaybeUninit, ops::Add};

use rust_spec::{RustSpec, niche::WithoutNiche};

use crate::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};

/// FFI-safe equivalent of [`core::result::Result`]
#[repr(C)]
pub union ReprCResult<T: Copy, E: Copy> {
    ok: ReprCResultOk<T>,
    err: ReprCResultErr<E>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ReprCResultOk<T: Copy>(u8, MaybeUninit<T>);

#[repr(C)]
#[derive(Clone, Copy)]
struct ReprCResultErr<E: Copy>(u8, MaybeUninit<E>);

impl<T: core::fmt::Debug + Copy, E: core::fmt::Debug + Copy> core::fmt::Debug
    for ReprCResult<T, E>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag() {
            0 => f
                .debug_tuple("ReprCResult::Ok")
                .field(unsafe { self.ok.1.assume_init_ref() })
                .finish(),
            1 => f
                .debug_tuple("ReprCResult::Err")
                .field(unsafe { self.err.1.assume_init_ref() })
                .finish(),
            tag => f
                .debug_struct("ReprCResult::<invalid>")
                .field("tag", &tag)
                .finish(),
        }
    }
}

impl<T: PartialEq + Copy, E: PartialEq + Copy> PartialEq for ReprCResult<T, E> {
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

impl<T: PartialOrd + Copy, E: PartialOrd + Copy> PartialOrd for ReprCResult<T, E> {
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

impl<T: Copy, E: Copy> ReprCResult<T, E> {
    pub(crate) const NICHE_VALUE: Self = Self {
        ok: ReprCResultOk(2, MaybeUninit::zeroed()),
    };

    /// Construct the success value
    #[expect(non_snake_case)]
    pub const fn Ok(ok: T) -> Self {
        Self {
            ok: ReprCResultOk(0, MaybeUninit::new(ok)),
        }
    }

    /// Construct the error value
    #[expect(non_snake_case)]
    pub const fn Err(err: E) -> Self {
        Self {
            err: ReprCResultErr(1, MaybeUninit::new(err)),
        }
    }

    #[inline(always)]
    fn tag(self) -> u8 {
        // SAFETY: Variant structs have tag as the first field
        unsafe { *core::ptr::from_ref(&self).cast::<u8>() }
    }

    #[inline(always)]
    fn forward_payload<U: Copy, V: Copy>(self) -> ReprCResult<U, V> {
        let mut output = MaybeUninit::<ReprCResult<U, V>>::zeroed();

        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::from_ref(&self).cast::<u8>(),
                output.as_mut_ptr().cast::<u8>(),
                core::cmp::min(size_of::<Self>(), size_of::<ReprCResult<U, V>>()),
            );

            output.assume_init()
        }
    }
}

impl<T: Copy, E: Copy> Copy for ReprCResult<T, E> {}
impl<T: Copy, E: Copy> Clone for ReprCResult<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Copy, E: Copy> From<Result<T, E>> for ReprCResult<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(ok) => Self::Ok(ok),
            Err(err) => Self::Err(err),
        }
    }
}

impl<T: Copy, E: Copy> TryFrom<ReprCResult<T, E>> for Result<T, E> {
    type Error = ();

    fn try_from(value: ReprCResult<T, E>) -> Result<Self, Self::Error> {
        match value.tag() {
            0 => Ok(Ok(unsafe { value.ok.1.assume_init() })),
            1 => Ok(Err(unsafe { value.err.1.assume_init() })),
            _ => Err(()),
        }
    }
}

unsafe impl<T: RustSpec + Copy, E: RustSpec + Copy> RustSpec for ReprCResult<T, E>
where
    T: RustSpec<Layout: Add<<E as RustSpec>::Layout>, Trap: Add<E::Trap>>,
    T::Alignment: rust_spec::Max<E::Alignment>,
    T::__IndirectTrap: Add<E::__IndirectTrap>,
{
    type Layout = <<T as RustSpec>::Layout as Add<<E as RustSpec>::Layout>>::Output;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type Alignment = <T::Alignment as rust_spec::Max<E::Alignment>>::Output;
    type Trap = <<T as RustSpec>::Trap as Add<<E as RustSpec>::Trap>>::Output;
    type Niche = WithoutNiche;
    type Mutability = rust_spec::mutability::Exclusive;
    type __IndirectTrap = <T::__IndirectTrap as Add<E::__IndirectTrap>>::Output;
}

unsafe impl<T: Borrow + Copy, E: Borrow + Copy> Borrow for ReprCResult<T, E>
where
    for<'itm> T::Borrowed<'itm>: Copy,
    for<'itm> E::Borrowed<'itm>: Copy,
{
    type Borrowed<'itm>
        = ReprCResult<T::Borrowed<'itm>, E::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = Option<Result<T::Owner, E::Owner>>;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self.tag() {
            0 => {
                let Result::Ok(owner) = owner.insert(Result::Ok(Default::default())) else {
                    unreachable!()
                };

                ReprCResult::Ok(unsafe { self.ok.1.assume_init() }.borrow(owner))
            }
            1 => {
                let Result::Err(owner) = owner.insert(Result::Err(Default::default())) else {
                    unreachable!()
                };

                ReprCResult::Err(unsafe { self.err.1.assume_init() }.borrow(owner))
            }
            _ => self.forward_payload(),
        }
    }
}
impl<'itm, T: FromBorrow<'itm> + Copy, E: FromBorrow<'itm> + Copy> FromBorrow<'itm>
    for ReprCResult<T, E>
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

impl<T: ExternC<CType: Copy> + Copy, E: ExternC<CType: Copy> + Copy> ExternC for ReprCResult<T, E> {
    type CType = ReprCResult<T::CType, E::CType>;
}
unsafe impl<T: EncodeOwned<CType: Copy> + Copy, E: EncodeOwned<CType: Copy> + Copy> EncodeOwned
    for ReprCResult<T, E>
{
    type Store = Option<Result<T::Store, E::Store>>;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        match self.tag() {
            0 => {
                let Result::Ok(store) = store.insert(Result::Ok(Default::default())) else {
                    unreachable!()
                };

                ReprCResult::Ok(unsafe { self.ok.1.assume_init() }.soft_encode(store))
            }
            1 => {
                let Result::Err(store) = store.insert(Result::Err(Default::default())) else {
                    unreachable!()
                };

                ReprCResult::Err(unsafe { self.err.1.assume_init() }.soft_encode(store))
            }
            _ => self.forward_payload(),
        }
    }
}
unsafe impl<'d, T: DecodeOwned<'d, CType: Copy> + Copy, E: DecodeOwned<'d, CType: Copy> + Copy>
    DecodeOwned<'d> for ReprCResult<T, E>
{
    type Store = Option<Result<T::Store, E::Store>>;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        match source.tag() {
            0 => {
                let Result::Ok(store) = store.insert(Result::Ok(Default::default())) else {
                    unreachable!()
                };

                Some(Self::Ok(unsafe {
                    T::soft_decode(source.ok.1.assume_init(), store)?
                }))
            }
            1 => {
                let Result::Err(store) = store.insert(Result::Err(Default::default())) else {
                    unreachable!()
                };

                Some(Self::Err(unsafe {
                    E::soft_decode(source.err.1.assume_init(), store)?
                }))
            }
            _ => Some(source.forward_payload()),
        }
    }
}

impl<T: Encode<CType: Copy> + Copy, E: Encode<CType: Copy> + Copy> Encode for ReprCResult<T, E> {}
impl<'d, T: Decode<'d, CType: Copy> + Copy, E: Decode<'d, CType: Copy> + Copy> Decode<'d>
    for ReprCResult<T, E>
{
}

unsafe impl<T: CheckedTransmute<CType: Copy> + Copy, E: CheckedTransmute<CType: Copy> + Copy>
    CheckedTransmute for ReprCResult<T, E>
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

unsafe impl<T: ReprC + Copy, E: ReprC + Copy> ReprC for ReprCResult<T, E> {}
unsafe impl<T: ReprC + Copy, E: ReprC + Copy> CFnArg for ReprCResult<T, E> {}
unsafe impl<T: ReprC + Copy, E: ReprC + Copy> CFnReturn for ReprCResult<T, E> {}

unsafe impl<T: BorrowCast<AsConst: Copy> + Copy, E: BorrowCast<AsConst: Copy> + Copy> BorrowCast
    for ReprCResult<T, E>
{
    type AsConst = ReprCResult<T::AsConst, E::AsConst>;
}

unsafe impl<T: BorrowCastMut<AsMut: Copy> + Copy, E: BorrowCastMut<AsMut: Copy> + Copy>
    BorrowCastMut for ReprCResult<T, E>
{
    type AsMut = ReprCResult<T::AsMut, E::AsMut>;
}

unsafe impl<T: EmptyStore, E: EmptyStore> EmptyStore for Result<T, E> {}
