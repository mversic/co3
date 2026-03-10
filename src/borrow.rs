#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::{cell::UnsafeCell, ops::Add};

use disjoint_impls::disjoint_impls;

use crate::{
    assert_arr_has_non_zero_len,
    ir::{Cloned, Opaque, ReprFamily, Robust, Transmuted},
    transmute::CheckedTransmute,
};

trait NonOpaque {}
impl NonOpaque for Robust {}
impl<T: Cloned> NonOpaque for T {}

// TODO: This struct exists only because arrays don't implement Default
// https://github.com/rust-lang/rust/issues/61415
pub struct ArrayBorrowStore<D, const N: usize>([D; N]);
impl<D: Default, const N: usize> Default for ArrayBorrowStore<D, N> {
    #[inline(always)]
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

pub enum NeedsDrop {}
pub enum NoDrop {}

disjoint_impls! {
    pub trait DropFamily {
        type Kind;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized> DropFamily for Box<R>
    where
        R: ReprFamily<Kind = Opaque>,
    {
        type Kind = NoDrop;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized> DropFamily for Box<R>
    where
        Self: CheckedTransmute<Target: DropFamily>,
        R: ReprFamily<Kind = Transmuted>,
    {
        type Kind = <<Self as CheckedTransmute>::Target as DropFamily>::Kind;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized> DropFamily for Box<R>
    where
        R: ReprFamily<Kind: NonOpaque>,
    {
        type Kind = NeedsDrop;
    }
}

disjoint_impls! {
    // TODO: It seems silly to take Store just to put the owned value inside it
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

        /// Target type
        ///
        /// `core::mem::needs_drop` SHOULD NOT return true for this type unless the type is
        /// [`Opaque`] or [`Box<Opaque>`] or a [`CheckedTransmute`] chain that ends in either
        type Borrowed<'itm>
        where
            Self: 'itm;

        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized> Borrow for Box<R>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        type Store = ();

        type Borrowed<'itm>
            = Self
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
    #[cfg(feature = "alloc")]
    impl<R: ?Sized> Borrow for Box<R>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        type Store = Option<Self>;

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

    impl<R, const N: usize> Borrow for [R; N]
    where
        Self: DropFamily<Kind = NoDrop>
    {
        type Store = ();

        type Borrowed<'itm>
            = Self
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
    impl<R: Borrow, const N: usize> Borrow for [R; N]
    where
        Self: DropFamily<Kind = NeedsDrop>
    {
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
            let mut items = self.into_iter();
            let mut stores = store.0.iter_mut();

            core::array::from_fn(|_| {
                let item = items.next().unwrap();
                let store = stores.next().unwrap();

                item.borrow(store)
            })
        }
    }

    impl<R> Borrow for Option<R>
    where
        Self: DropFamily<Kind = NoDrop>
    {
        type Store = ();

        type Borrowed<'itm>
            = Self
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
    impl<R: Borrow> Borrow for Option<R>
    where
        Self: DropFamily<Kind = NeedsDrop>
    {
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
            self.map(|b| b.borrow(store))
        }
    }

    impl<T, E> Borrow for Result<T, E>
    where
        Self: DropFamily<Kind = NoDrop>
    {
        type Store = ();

        type Borrowed<'itm>
            = Self
        where
            Self: 'itm;

        #[inline(always)]
        fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self
        }
    }
    impl<T: Borrow, E: Borrow> Borrow for Result<T, E>
    where
        Self: DropFamily<Kind = NeedsDrop>
    {
        type Store = Option<Result<T::Store, E::Store>>;

        type Borrowed<'itm>
            = Result<T::Borrowed<'itm>, E::Borrowed<'itm>>
        where
            Self: 'itm;

        #[inline(always)]
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => {
                    let ok_store = store.insert(Ok(Default::default()));
                    Ok(ok.borrow(unsafe { ok_store.as_mut().unwrap_unchecked() }))
                }
                Err(err) => {
                    let err_store = store.insert(Err(Default::default()));
                    Err(err.borrow(unsafe { err_store.as_mut().unwrap_err_unchecked() }))
                }
            }
        }
    }

    impl<T> Borrow for UnsafeCell<T>
    where
        Self: DropFamily<Kind = NoDrop>
    {
        type Store = ();

        type Borrowed<'itm>
            = Self
        where
            Self: 'itm;

        fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self
        }
    }
    impl<T> Borrow for UnsafeCell<T>
    where
        Self: DropFamily<Kind = NeedsDrop>
    {
        type Store = Option<Self>;

        type Borrowed<'itm>
            = &'itm Self
        where
            Self: 'itm;

        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            store.insert(self)
        }
    }
}

impl<R: ?Sized> DropFamily for &R {
    type Kind = NoDrop;
}

impl<R: ?Sized> DropFamily for &mut R {
    type Kind = NoDrop;
}

impl<R: DropFamily> DropFamily for [R] {
    type Kind = R::Kind;
}

// FIXME: This should be covered by blanket IMO
#[cfg(feature = "alloc")]
impl<R> DropFamily for Box<[R]> {
    type Kind = NeedsDrop;
}

#[cfg(feature = "alloc")]
impl<R> DropFamily for Vec<R> {
    type Kind = NeedsDrop;
}

impl<R: DropFamily, const N: usize> DropFamily for [R; N] {
    type Kind = R::Kind;
}

impl<R: DropFamily> DropFamily for Option<R> {
    type Kind = R::Kind;
}

impl<R: ?Sized> Borrow for &R {
    type Store = ();

    type Borrowed<'itm>
        = Self
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
        = Self
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

impl Add for NoDrop {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}
impl Add<NeedsDrop> for NoDrop {
    type Output = NeedsDrop;

    fn add(self, _: NeedsDrop) -> Self::Output {
        unreachable!()
    }
}
impl Add<NoDrop> for NeedsDrop {
    type Output = NeedsDrop;

    fn add(self, _: NoDrop) -> Self::Output {
        unreachable!()
    }
}
impl Add for NeedsDrop {
    type Output = Self;

    fn add(self, _: Self) -> Self::Output {
        unreachable!()
    }
}
