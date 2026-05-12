#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;

use crate::{
    dst::{DstFamily, ExternTypeLike, Sized_, SliceLike, TraitObjectLike},
    ir::{Cloned, Opaque, ReprFamily, Robust, Transmuted},
    niche::{NicheFamily, WithNiche, WithoutNiche},
};

trait NonExternTypeLike {}
trait NonOpaqueOrTransmuted {}
impl NonOpaqueOrTransmuted for Robust {}
impl<S: Cloned> NonOpaqueOrTransmuted for S {}
impl<S> NonOpaqueOrTransmuted for [S] {}
impl NonExternTypeLike for Sized_ {}
impl NonExternTypeLike for SliceLike {}
impl NonExternTypeLike for TraitObjectLike {}

/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayBorrowStore<D, const N: usize>([D; N]);
impl<D: Default, const N: usize> Default for ArrayBorrowStore<D, N> {
    #[inline(always)]
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

disjoint_impls! {
    // TODO: It seems silly to take Store just to put the owned value inside it
    // It would make more sense to take a reference and require no store
    // A signature would look like this: `source` is either `&self` or `&mut self`
    // pub trait Borrow: Sized {
    //     type Source<'itm>;
    //     type Borrowed<'itm>;
    //
    //     fn borrow<'itm>(source: Self::Source<'itm>) -> Self::Borrowed<'itm>;
    // }
    //
    // FIXME: Rename the trait
    // TODO: Should I join Borrow and ToOwned?
    // TODO: Should we allow default value IN_STRUCT = false?
    pub trait Borrow<const IN_STRUCT: bool>: Sized {
        /// Target type
        ///
        /// `core::mem::needs_drop` SHOULD NOT return true for this type unless the type is
        /// [`Box<Opaque>`] or a [`CheckedTransmute`] chain that ends with [`Box<Opaque>`].
        type Borrowed<'itm> // FIXME: This bound is required: ExternC<CType: FnArg>
        where
            Self: 'itm;

        type Store: Default;

        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized, const IN_STRUCT: bool> Borrow<IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind = Opaque>,
    {
        type Borrowed<'itm>
            = Self
        where
            Self: 'itm;

        type Store = ();

        #[inline(always)]
        fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self
        }
    }
    #[cfg(feature = "alloc")]
    impl<R, const IN_STRUCT: bool> Borrow<IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = ExternTypeLike>,
    {
        type Borrowed<'itm>
            = Self
        where
            Self: 'itm;

        type Store = ();

        #[inline(always)]
        fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, const IN_STRUCT: bool> Borrow<IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind = Transmuted> + DstFamily<Kind: NonExternTypeLike>,
    {
        type Borrowed<'itm>
            = &'itm R
        where
            Self: 'itm;

        // NOTE: If Option<R> was used a potentially
        // large value would be placed on the stack
        type Store = Option<Self>;

        #[inline(always)]
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            store.insert(self)
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, const IN_STRUCT: bool> Borrow<IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind: NonOpaqueOrTransmuted>,
    {
        type Borrowed<'itm>
            = &'itm R
        where
            Self: 'itm;

        // NOTE: If Option<R> was used a potentially
        // large value would be placed on the stack
        type Store = Option<Self>;

        #[inline(always)]
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            store.insert(self)
        }
    }

    impl<R: Borrow<true>> Borrow<false> for Option<R>
    where
        R: NicheFamily<Kind = WithoutNiche>,
    {
        type Borrowed<'itm>
            = Option<R::Borrowed<'itm>>
        where
            Self: 'itm;

        type Store = R::Store;

        #[inline(always)]
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self.map(|value| value.borrow(store))
        }
    }
    impl<R: Borrow<false>> Borrow<false> for Option<R>
    where
        R: NicheFamily<Kind: WithNiche>,
    {
        type Borrowed<'itm>
            = Option<R::Borrowed<'itm>>
        where
            Self: 'itm;

        type Store = R::Store;

        #[inline(always)]
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self.map(|value| value.borrow(store))
        }
    }

    // FIXME:
    //impl<R: Borrow<true>, E: Borrow<true>> Borrow<false> for Result<R, E>
    //where
    //    Self: NicheFamily<Kind = WithoutNiche>,
    //{
    //    type Borrowed<'itm>
    //        = Result<R::Borrowed<'itm>, E::Borrowed<'itm>>
    //    where
    //        Self: 'itm;

    //    type Store = (R::Store, E::Store);

    //    #[inline(always)]
    //    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    //    where
    //        Self: 'itm,
    //    {
    //        match self {
    //            Ok(value) => Ok(value.borrow(&mut store.0)),
    //            Err(err) => Err(err.borrow(&mut store.1)),
    //        }
    //    }
    //}
    //impl<R: Borrow<false>, E: Borrow<false>> Borrow<false> for Result<R, E>
    //where
    //    Self: NicheFamily<Kind: WithNiche>,
    //{
    //    type Borrowed<'itm>
    //        = Result<R::Borrowed<'itm>, E::Borrowed<'itm>>
    //    where
    //        Self: 'itm;

    //    type Store = (R::Store, E::Store);

    //    #[inline(always)]
    //    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    //    where
    //        Self: 'itm,
    //    {
    //        match self {
    //            Ok(value) => Ok(value.borrow(&mut store.0)),
    //            Err(err) => Err(err.borrow(&mut store.1)),
    //        }
    //    }
    //}
}

disjoint_impls! {
    pub trait ToOwned<'r, const IN_STRUCT: bool>: Borrow<IN_STRUCT> {
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self;
    }

    #[cfg(feature = "alloc")]
    impl<'r, R: ?Sized + 'r, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind = Opaque>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    #[cfg(feature = "alloc")]
    impl<'r, R: 'r, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind = Transmuted> + DstFamily<Kind = ExternTypeLike>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    // TODO: This isn't working for Box<[T]> and the like
    #[cfg(feature = "alloc")]
    impl<'r, R: Clone, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind = Transmuted> + DstFamily<Kind: NonExternTypeLike>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            Box::new(borrowed.clone())
        }
    }
    #[cfg(feature = "alloc")]
    impl<'r, R: Clone, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for Box<R>
    where
        R: ReprFamily<Kind: NonOpaqueOrTransmuted>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            Box::new(borrowed.clone())
        }
    }

    impl<'r, R: ToOwned<'r, true>> ToOwned<'r, false> for Option<R>
    where
        R: NicheFamily<Kind = WithoutNiche>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed.map(R::to_owned)
        }
    }
    impl<'r, R: ToOwned<'r, false> + Clone> ToOwned<'r, false> for Option<R>
    where
        R: NicheFamily<Kind: WithNiche>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed.map(R::to_owned)
        }
    }

    //FIXME:
    //impl<'r, R: ToOwned<'r, true>, E: ToOwned<'r, true>> ToOwned<'r, false> for Result<R, E>
    //where
    //    Self: NicheFamily<Kind = WithoutNiche>,
    //{
    //    #[inline(always)]
    //    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
    //        match borrowed {
    //            Ok(value) => Ok(R::to_owned(value)),
    //            Err(err) => Err(E::to_owned(err)),
    //        }
    //    }
    //}
    //impl<'r, R: ToOwned<'r, false>, E: ToOwned<'r, false>> ToOwned<'r, false> for Result<R, E>
    //where
    //    Self: NicheFamily<Kind: WithNiche>,
    //{
    //    #[inline(always)]
    //    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
    //        match borrowed {
    //            Ok(value) => Ok(R::to_owned(value)),
    //            Err(err) => Err(E::to_owned(err)),
    //        }
    //    }
    //}
}

impl<R: ?Sized, const IN_STRUCT: bool> Borrow<IN_STRUCT> for &R {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Store = ();

    #[inline(always)]
    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}

impl<R: ?Sized, const IN_STRUCT: bool> Borrow<IN_STRUCT> for &mut R {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Store = ();

    #[inline(always)]
    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}

#[cfg(feature = "alloc")]
impl<R, const IN_STRUCT: bool> Borrow<IN_STRUCT> for Vec<R> {
    type Borrowed<'itm>
        = &'itm Self
    where
        Self: 'itm;

    type Store = Option<Self>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        store.insert(self)
    }
}

impl<R, const N: usize> Borrow<false> for [R; N] {
    type Borrowed<'itm>
        = &'itm Self
    where
        Self: 'itm;

    type Store = Option<Self>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        store.insert(self)
    }
}
impl<R: Borrow<true>, const N: usize> Borrow<true> for [R; N] {
    type Borrowed<'itm>
        = [R::Borrowed<'itm>; N]
    where
        Self: 'itm;

    type Store = ArrayBorrowStore<R::Store, N>;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
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

impl<'r, R: ?Sized, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for &'r R {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed
    }
}

impl<'r, R: ?Sized, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for &'r mut R {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed
    }
}

#[cfg(feature = "alloc")]
impl<'r, R: Clone, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for Vec<R> {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed.clone()
    }
}

impl<'r, R: Clone, const N: usize> ToOwned<'r, false> for [R; N] {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed.clone()
    }
}
impl<'r, R: ToOwned<'r, true>, const N: usize> ToOwned<'r, true> for [R; N] {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed.map(R::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU8;

    use super::*;

    #[test]
    fn array_borrow() {
        let _: Option<<[u8; 2] as Borrow<false>>::Borrowed<'_>> = None::<&[u8; 2]>;
        let _: Option<<[NonZeroU8; 2] as Borrow<false>>::Borrowed<'_>> = None::<&[NonZeroU8; 2]>;

        let _: Option<<[u8; 2] as Borrow<true>>::Borrowed<'_>> = None::<[u8; 2]>;
        let _: Option<<[NonZeroU8; 2] as Borrow<true>>::Borrowed<'_>> = None::<[NonZeroU8; 2]>;

        let _: Option<<Option<[u8; 2]> as Borrow<false>>::Borrowed<'_>> = None::<Option<[u8; 2]>>;
        let _: Option<<Option<[NonZeroU8; 2]> as Borrow<false>>::Borrowed<'_>> =
            None::<Option<&[NonZeroU8; 2]>>;

        let _: Option<<Option<[u8; 2]> as Borrow<true>>::Borrowed<'_>> = None::<Option<[u8; 2]>>;
        let _: Option<<Option<[NonZeroU8; 2]> as Borrow<true>>::Borrowed<'_>> =
            None::<Option<[NonZeroU8; 2]>>;
    }
}
