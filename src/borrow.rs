#[cfg(feature = "alloc")]
use alloc_crate::{borrow::ToOwned as StdToOwned, boxed::Box, string::String, vec::Vec};
use core::{cell::UnsafeCell, ptr::NonNull};

use crate::{
    ReprC,
    size::{DynTraitLike, MetaSized, SizeFamily, SliceLike},
    stored::ArrayStore,
};

pub(crate) trait NonExternTypeLike {}
impl NonExternTypeLike for MetaSized<SliceLike> {}
impl NonExternTypeLike for MetaSized<DynTraitLike> {}
impl<S> NonExternTypeLike for crate::size::Sized<S> {}

/// A layout-compatible borrowed view of a robust C representation.
///
/// # Safety
///
/// - only owned to borrowed pointer casting is allowed
// TODO: Stupid trait with a stupid name and stupid bounds
pub unsafe trait BorrowCast: ReprC + Sized {
    type AsConst: ReprC;
    type AsMut: ReprC;
}

#[inline(always)]
pub fn borrow_cast<C: BorrowCast>(source: C) -> C::AsConst {
    unsafe { core::mem::transmute_copy(&source) }
}

#[inline(always)]
pub fn borrow_cast_mut<C: BorrowCast>(source: C) -> C::AsMut {
    unsafe { core::mem::transmute_copy(&source) }
}

/// A trait for structurally borrowing data.
///
/// It should hold that `<T::CType as BorrowCast>::AsConst == <T::Borrowed as ExternC>::CType`
pub trait Borrow: Sized {
    type Borrowed<'itm>
    where
        Self: 'itm;

    type Owner: Default;
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm;
}
// TODO: Join the 2 traits?
pub trait ToOwned<'itm>: Borrow {
    fn to_owned(source: Self::Borrowed<'itm>) -> Self;
}

impl<R: Borrow> Borrow for Option<R> {
    type Borrowed<'itm>
        = Option<R::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = R::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.map(|value| value.borrow(store))
    }
}
impl<'itm, R: ToOwned<'itm>> ToOwned<'itm> for Option<R> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.map(R::to_owned)
    }
}

impl<T: Borrow, E: Borrow> Borrow for Result<T, E> {
    type Borrowed<'itm>
        = Result<T::Borrowed<'itm>, E::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = Option<Result<T::Owner, E::Owner>>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self {
            Ok(value) => {
                let ok_store = store.insert(Ok(Default::default())).as_mut();
                Ok(value.borrow(unsafe { ok_store.unwrap_unchecked() }))
            }
            Err(err) => {
                let err_store = store.insert(Err(Default::default())).as_mut();
                Err(err.borrow(unsafe { err_store.unwrap_err_unchecked() }))
            }
        }
    }
}
impl<'itm, T: ToOwned<'itm>, E: ToOwned<'itm>> ToOwned<'itm> for Result<T, E> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        match source {
            Ok(value) => Ok(T::to_owned(value)),
            Err(err) => Err(E::to_owned(err)),
        }
    }
}

impl<R: ?Sized> Borrow for &R {
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
impl<'itm, 'a: 'itm, R: ?Sized> ToOwned<'itm> for &'a R {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<R: ?Sized> Borrow for &mut R {
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
impl<'itm, 'a: 'itm, R: ?Sized> ToOwned<'itm> for &'a mut R {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

#[cfg(feature = "alloc")]
// NOTE: extern types cannot be borrowed, only moved
// TODO: But maybe the bound of NonExternTypeLike is not required
impl<R: SizeFamily<Kind: NonExternTypeLike> + ?Sized> Borrow for Box<R> {
    type Borrowed<'itm>
        = &'itm R
    where
        Self: 'itm;

    // NOTE: If Option<R> was used a potentially
    // large value would be placed on the stack
    type Owner = Option<Self>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        store.insert(self)
    }
}
#[cfg(feature = "alloc")]
impl<'itm, R: SizeFamily<Kind: NonExternTypeLike> + StdToOwned + ?Sized> ToOwned<'itm> for Box<R>
where
    R::Owned: Into<Self>,
{
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        StdToOwned::to_owned(source).into()
    }
}

#[cfg(feature = "alloc")]
impl<R> Borrow for Vec<R> {
    type Borrowed<'itm>
        = &'itm [R]
    where
        Self: 'itm;

    type Owner = Self;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *store = self;
        store
    }
}
#[cfg(feature = "alloc")]
impl<'itm, R: Clone> ToOwned<'itm> for Vec<R> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.to_vec()
    }
}

#[cfg(feature = "alloc")]
impl Borrow for String {
    type Borrowed<'itm>
        = &'itm str
    where
        Self: 'itm;

    type Owner = Self;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *store = self;
        store
    }
}
#[cfg(feature = "alloc")]
impl<'itm> ToOwned<'itm> for String {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.into()
    }
}

impl<T> Borrow for NonNull<T> {
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
impl<'itm, T: 'itm> ToOwned<'itm> for NonNull<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: Borrow> Borrow for UnsafeCell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.into_inner().borrow(store)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for UnsafeCell<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        UnsafeCell::new(T::to_owned(source))
    }
}

impl<R: Borrow, const N: usize> Borrow for [R; N] {
    type Borrowed<'itm>
        = [R::Borrowed<'itm>; N]
    where
        Self: 'itm;

    type Owner = ArrayStore<R::Owner, N>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        let mut borrowed: [_; N] = [const { core::mem::MaybeUninit::uninit() }; N];

        for ((elem, substore), borrow) in self.into_iter().zip(&mut store.0).zip(&mut borrowed) {
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
impl<'itm, R: ToOwned<'itm>, const N: usize> ToOwned<'itm> for [R; N] {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source.map(R::to_owned)
    }
}

impl Borrow for () {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Owner = ();

    #[inline(always)]
    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm> {}
}
impl<'itm> ToOwned<'itm> for () {
    #[inline(always)]
    fn to_owned(_: ()) -> Self {}
}
