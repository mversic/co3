#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use crate::assert_arr_has_non_zero_len;

// TODO: It seems silly to take Store just to put the owned value inside it.
// It would make more sense to take a reference and require no tore
// A signature would look like this: `source` is either `&self` or `&mut self`
//pub trait Borrow: Sized {
//    type Source<'itm>;
//    type Borrowed<'itm>;
//
//    fn borrow<'itm>(source: Self::Source<'itm>) -> Self::Borrowed<'itm>;
//}
//
// FIXME: Rename to PartialBorrow? also rename generated names in no_repr.rs
pub trait Borrow: Sized {
    type Store: Default;

    type Borrowed<'itm>
    where
        Self: 'itm;

    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm;
}

impl<R: ?Sized> Borrow for &R {
    type Store = ();

    type Borrowed<'itm>
        = &'itm R
    where
        Self: 'itm;

    #[inline(always)]
    fn borrow<'itm>(self, (): &'itm mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}

impl<R: ?Sized> Borrow for &mut R {
    type Store = ();

    type Borrowed<'itm>
        = &'itm mut R
    where
        Self: 'itm;

    #[inline(always)]
    fn borrow<'itm>(self, (): &'itm mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}

// FIXME: Not correct for Box<Opaque>
#[cfg(feature = "alloc")]
impl<R: ?Sized> Borrow for Box<R> {
    type Store = Option<Box<R>>;

    type Borrowed<'itm>
        = &'itm R
    where
        Self: 'itm;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        store.insert(self)
    }
}

#[cfg(feature = "alloc")]
impl<R> Borrow for Vec<R> {
    type Store = Self;

    type Borrowed<'itm>
        = &'itm [R]
    where
        Self: 'itm;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *store = self;
        store
    }
}

impl<R: Borrow, const N: usize> Borrow for [R; N] {
    type Store = ArrayBorrowStore<R::Store, N>;

    type Borrowed<'itm>
        = [R::Borrowed<'itm>; N]
    where
        Self: 'itm;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        assert_arr_has_non_zero_len::<N>();
        let store = &mut store.0;

        let mut items = self.into_iter();
        let mut stores = store.iter_mut();

        core::array::from_fn(|_| {
            let item = items.next().unwrap();
            let store = stores.next().unwrap();

            item.borrow(store)
        })
    }
}

impl<R: Borrow> Borrow for Option<R> {
    type Store = R::Store;

    type Borrowed<'itm>
        = Option<R::Borrowed<'itm>>
    where
        Self: 'itm;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.map(|e| e.borrow(store))
    }
}

// TODO: This struct exists only because arrays don't implement Default
// https://github.com/rust-lang/rust/issues/61415
pub struct ArrayBorrowStore<D, const N: usize>([D; N]);
impl<D: Default, const N: usize> Default for ArrayBorrowStore<D, N> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}
