#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};

#[cfg(feature = "alloc")]
use rust_spec::{
    RustSpec,
    size::{MetaSized, MetadataKind, SizedKind},
};

use crate::{CType, stored::ArrayStore};

// TODO: Remove this once extern types are stable
// https://github.com/rust-lang/rust/issues/43467
#[cfg(feature = "alloc")]
trait NonExternTypeLike {}
#[cfg(feature = "alloc")]
impl<K: MetadataKind> NonExternTypeLike for MetaSized<K> {}
#[cfg(feature = "alloc")]
impl<K: SizedKind> NonExternTypeLike for rust_spec::size::Sized<K> {}

/// A layout-compatible borrowed view of a robust C representation.
///
/// # Safety
///
/// - only owned to borrowed const pointer casting is allowed
// TODO: Stupid trait with a stupid name
pub unsafe trait BorrowCast: CType {
    type AsConst: CType + ?Sized;
}

/// A layout-compatible mutably borrowed view of a robust C representation.
///
/// # Safety
///
/// - only owned to borrowed mut pointer casting is allowed
pub unsafe trait BorrowCastMut: CType {
    type AsMut: CType + ?Sized;
}

#[inline(always)]
pub const fn borrow_cast<C: BorrowCast<AsConst: Copy> + Copy>(source: C) -> C::AsConst {
    unsafe { core::mem::transmute_copy(&source) }
}

#[inline(always)]
pub const fn borrow_cast_mut<C: BorrowCastMut<AsMut: Copy> + Copy>(source: C) -> C::AsMut {
    unsafe { core::mem::transmute_copy(&source) }
}

/// A trait for structurally borrowing data.
///
/// It should hold that `<T::CType as BorrowCast>::AsConst == <T::Borrowed as ReprC>::CType`
///
/// # Safety
///
/// If [`Self::Owner`] implements [`crate::stored::EmptyStore`], [`Borrow::borrow`] must not
/// return references into `owner`. This rule prevents ownership transfer in return values.
pub unsafe trait Borrow: Sized {
    type Borrowed<'itm>
    where
        Self: 'itm;

    type Owner: Default;
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm;
}
// TODO: Join the 2 traits?
pub trait FromBorrow<'itm>: Borrow {
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self;
}

unsafe impl<R: Borrow> Borrow for Option<R> {
    type Borrowed<'itm>
        = Option<R::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = R::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.map(|value| value.borrow(owner))
    }
}
impl<'itm, R: FromBorrow<'itm>> FromBorrow<'itm> for Option<R> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source.map(R::from_borrow)
    }
}

unsafe impl<T: Borrow, E: Borrow> Borrow for Result<T, E> {
    type Borrowed<'itm>
        = Result<T::Borrowed<'itm>, E::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = Option<Result<T::Owner, E::Owner>>;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self {
            Ok(value) => {
                let ok_owner = owner.insert(Ok(Default::default())).as_mut();
                Ok(value.borrow(unsafe { ok_owner.unwrap_unchecked() }))
            }
            Err(err) => {
                let err_owner = owner.insert(Err(Default::default())).as_mut();
                Err(err.borrow(unsafe { err_owner.unwrap_err_unchecked() }))
            }
        }
    }
}
impl<'itm, T: FromBorrow<'itm>, E: FromBorrow<'itm>> FromBorrow<'itm> for Result<T, E> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        match source {
            Ok(value) => Ok(T::from_borrow(value)),
            Err(err) => Err(E::from_borrow(err)),
        }
    }
}

unsafe impl<R: ?Sized> Borrow for &R {
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
impl<'itm, 'a: 'itm, R: ?Sized> FromBorrow<'itm> for &'a R {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

unsafe impl<R: ?Sized> Borrow for &mut R {
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
impl<'itm, 'a: 'itm, R: ?Sized> FromBorrow<'itm> for &'a mut R {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

#[cfg(feature = "alloc")]
// NOTE: extern types cannot be borrowed, only moved
unsafe impl<R: RustSpec<Size: NonExternTypeLike> + ?Sized> Borrow for Box<R> {
    type Borrowed<'itm>
        = &'itm R
    where
        Self: 'itm;

    // NOTE: If just `Self` was used, a potentially
    // large value would be placed on the stack
    type Owner = Option<Self>;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        owner.insert(self)
    }
}
#[cfg(feature = "alloc")]
impl<'itm, R: RustSpec<Size: NonExternTypeLike> + ToOwned + ?Sized> FromBorrow<'itm> for Box<R>
where
    R::Owned: Into<Self>,
{
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        ToOwned::to_owned(source).into()
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R> Borrow for Vec<R> {
    type Borrowed<'itm>
        = &'itm [R]
    where
        Self: 'itm;

    type Owner = Self;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *owner = self;
        owner
    }
}
#[cfg(feature = "alloc")]
impl<'itm, R: Clone> FromBorrow<'itm> for Vec<R> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source.to_vec()
    }
}

unsafe impl<R: Borrow, const N: usize> Borrow for [R; N] {
    type Borrowed<'itm>
        = [R::Borrowed<'itm>; N]
    where
        Self: 'itm;

    type Owner = ArrayStore<R::Owner, N>;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        let mut borrowed: [_; N] = [const { core::mem::MaybeUninit::uninit() }; N];

        for ((elem, substore), borrow) in self.into_iter().zip(&mut owner.0).zip(&mut borrowed) {
            borrow.write(elem.borrow(substore));
        }

        // TODO: use https://github.com/rust-lang/rust/issues/96097
        unsafe {
            core::mem::transmute_copy::<
                [core::mem::MaybeUninit<R::Borrowed<'itm>>; N],
                [R::Borrowed<'itm>; N],
            >(&borrowed)
        }
    }
}
impl<'itm, R: FromBorrow<'itm>, const N: usize> FromBorrow<'itm> for [R; N] {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source.map(R::from_borrow)
    }
}
