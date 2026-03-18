#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::{cell::UnsafeCell, ops::Add};

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::ir::{SizeFamily, Sized_, UnSized};
use crate::{
    assert_arr_has_non_zero_len,
    ir::{Cloned, Opaque, ReprFamily, Robust, Transmuted},
    transmute::CheckedTransmute,
};

trait NonOpaqueOrTransparent {}
impl NonOpaqueOrTransparent for Robust {}
impl<S: Cloned> NonOpaqueOrTransparent for S {}

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
    impl<R> DropFamily for Box<R>
    where
        R: ReprFamily<Kind = Opaque>,
    {
        type Kind = NoDrop;
    }
    //impl<R: ?Sized> DropFamily for Box<R>
    //where
    //    R: ReprFamily<Kind = [Opaque]>,
    //{
    //    type Kind = NeedsDrop;
    //}
    #[cfg(feature = "alloc")]
    impl<R> DropFamily for Box<R>
    where
        R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = Sized_>,
        Self: CheckedTransmute<Target: DropFamily>,
    {
        type Kind = <<Self as CheckedTransmute>::Target as DropFamily>::Kind;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + CheckedTransmute> DropFamily for Box<R>
    where
        R: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = UnSized>,
        Box<<R as CheckedTransmute>::Target>: DropFamily,
    {
        type Kind = <Box<R::Target> as DropFamily>::Kind;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized> DropFamily for Box<R>
    where
        R: ReprFamily<Kind: NonOpaqueOrTransparent>,
    {
        type Kind = NeedsDrop;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, S: NonOpaqueOrTransparent> DropFamily for Box<R>
    where
        R: ReprFamily<Kind = [S]>,
    {
        type Kind = NeedsDrop;
    }

    //impl<R: ?Sized> DropFamily for Box<[R]>
    //where
    //    R: ReprFamily<Kind = [Opaque]>,
    //{
    //    type Kind = NeedsDrop;
    //}
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
    // FIXME: Rename to PartialBorrow? also rename generated names in no_repr.rs
    pub trait Borrow: Sized {
        type Borrowed<'itm>
        where
            Self: 'itm;

        type Store: Default;

        /// Target type
        ///
        /// `core::mem::needs_drop` SHOULD NOT return true for this type unless the type is
        /// [`Opaque`] or [`Box<Opaque>`] or a [`CheckedTransmute`] chain that ends in either.
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized> Borrow for Box<R>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        type Borrowed<'itm> = Self
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
    impl<R: ?Sized> Borrow for Box<R>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        type Borrowed<'itm> = &'itm R
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

    impl<R, const N: usize> Borrow for [R; N]
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        type Borrowed<'itm> = Self
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
    impl<R: Borrow, const N: usize> Borrow for [R; N]
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        type Borrowed<'itm> = [R::Borrowed<'itm>; N]
        where
            Self: 'itm;

        type Store = ArrayBorrowStore<R::Store, N>;

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
        Self: DropFamily<Kind = NoDrop>,
    {
        type Borrowed<'itm> = Self
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
    impl<R: Borrow> Borrow for Option<R>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        type Borrowed<'itm> = Option<R::Borrowed<'itm>>
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

    impl<T, E> Borrow for Result<T, E>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        type Borrowed<'itm> = Self
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
    impl<T: Borrow, E: Borrow> Borrow for Result<T, E>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        type Borrowed<'itm> = Result<T::Borrowed<'itm>, E::Borrowed<'itm>>
        where
            Self: 'itm;

        type Store = Option<Result<T::Store, E::Store>>;

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
        Self: DropFamily<Kind = NoDrop>,
    {
        type Borrowed<'itm> = Self
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
    impl<T: Borrow> Borrow for UnsafeCell<T>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        type Borrowed<'itm> = UnsafeCell<T::Borrowed<'itm>>
        where
            Self: 'itm,
            T: 'itm;

        type Store = T::Store;

        #[inline(always)]
        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            UnsafeCell::new(self.into_inner().borrow(store))
        }
    }
}

// TODO: I hope that some day it'll be possible to join all NoDrop impls into one
disjoint_impls! {
    pub trait ToOwned<'r>: Borrow {
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self;
    }

    #[cfg(feature = "alloc")]
    impl<'r, R: 'r> ToOwned<'r> for Box<R>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    #[cfg(feature = "alloc")]
    impl<'r, R: Clone> ToOwned<'r> for Box<R>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            // TODO: I don't know how to make it work for ?Sized types
            Box::new(borrowed.clone())
        }
    }

    impl<'r, R: 'r, const N: usize> ToOwned<'r> for [R; N]
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    impl<'r, R: ToOwned<'r>, const N: usize> ToOwned<'r> for [R; N]
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed.map(R::to_owned)
        }
    }

    impl<'r, R: 'r> ToOwned<'r> for Option<R>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    impl<'r, R: ToOwned<'r>> ToOwned<'r> for Option<R>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed.map(R::to_owned)
        }
    }

    impl<'r, T: 'r, E: 'r> ToOwned<'r> for Result<T, E>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    impl<'r, T: ToOwned<'r>, E: ToOwned<'r>> ToOwned<'r> for Result<T, E>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            match borrowed {
                Ok(value) => Ok(T::to_owned(value)),
                Err(err) => Err(E::to_owned(err)),
            }
        }
    }

    impl<'r, R: 'r> ToOwned<'r> for UnsafeCell<R>
    where
        Self: DropFamily<Kind = NoDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            borrowed
        }
    }
    impl<'r, R: ToOwned<'r>> ToOwned<'r> for UnsafeCell<R>
    where
        Self: DropFamily<Kind = NeedsDrop>,
    {
        #[inline(always)]
        fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
            UnsafeCell::new(R::to_owned(borrowed.into_inner()))
        }
    }
}

impl<R> DropFamily for [R] {
    type Kind = NeedsDrop;
}

impl<R: ?Sized> DropFamily for &R {
    type Kind = NoDrop;
}

impl<R: ?Sized> DropFamily for &mut R {
    type Kind = NoDrop;
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

impl<R: ?Sized> Borrow for &mut R {
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

impl<'r, R: ?Sized> ToOwned<'r> for &'r R {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed
    }
}

impl<'r, R: ?Sized> ToOwned<'r> for &'r mut R {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed
    }
}

#[cfg(feature = "alloc")]
impl<R> Borrow for Vec<R> {
    type Borrowed<'itm>
        = &'itm [R]
    where
        Self: 'itm;

    type Store = Self;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *store = self;
        store
    }
}

#[cfg(feature = "alloc")]
impl<'r, R: Clone> ToOwned<'r> for Vec<R> {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed.to_vec()
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

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::{boxed::Box, string::String, vec::Vec};
    use core::{cell::UnsafeCell, ptr::NonNull};

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{
        external::{ExternRef, ExternRefMut},
        ir::Opaque,
    };

    struct OpaqueStruct;
    struct ExternStruct;

    impl ReprFamily for OpaqueStruct {
        type Kind = Opaque;
    }
    impl ReprFamily for ExternStruct {
        type Kind = Transmuted;
    }

    #[test]
    fn references_are_no_drop() {
        #[cfg(feature = "alloc")]
        assert_impl_all!(&String: DropFamily<Kind = NoDrop>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut String: DropFamily<Kind = NoDrop>);

        #[cfg(feature = "alloc")]
        assert_impl_all!(&[String]: DropFamily<Kind = NoDrop>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut [String]: DropFamily<Kind = NoDrop>);

        assert_impl_all!(&OpaqueStruct: DropFamily<Kind = NoDrop>);
        assert_impl_all!(&mut OpaqueStruct: DropFamily<Kind = NoDrop>);

        assert_impl_all!(&[OpaqueStruct]: DropFamily<Kind = NoDrop>);
        assert_impl_all!(&mut [OpaqueStruct]: DropFamily<Kind = NoDrop>);

        assert_impl_all!(ExternRef<'static, u8>: DropFamily<Kind = NoDrop>);
        assert_impl_all!(ExternRefMut<'static, u8>: DropFamily<Kind = NoDrop>);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn alloc_needs_drop() {
        assert_impl_all!([u8]: DropFamily<Kind = NeedsDrop>);
        assert_impl_all!(Box<u8>: DropFamily<Kind = NeedsDrop>);
        assert_impl_all!(Box<[u8]>: DropFamily<Kind = NeedsDrop>);
        assert_impl_all!(Box<bool>: DropFamily<Kind = NeedsDrop>);
        assert_impl_all!(Box<str>: DropFamily<Kind = NeedsDrop>);

        assert_impl_all!(Vec<u8>: DropFamily<Kind = NeedsDrop>);
        assert_impl_all!(String: DropFamily<Kind = NeedsDrop>);
    }

    // FIXME:
    //#[test]
    //#[cfg(feature = "alloc")]
    //fn opaque_box_is_no_drop() {
    //    assert_impl_all!(Box<OpaqueStruct>: DropFamily<Kind = NoDrop>);
    //    assert_impl_all!(Box<&OpaqueStruct>: DropFamily<Kind = NeedsDrop>);
    //    assert_impl_all!(Box<&mut OpaqueStruct>: DropFamily<Kind = NeedsDrop>);
    //    assert_impl_all!(Box<&[OpaqueStruct]>: DropFamily<Kind = NeedsDrop>);
    //    assert_impl_all!(Box<&mut [OpaqueStruct]>: DropFamily<Kind = NeedsDrop>);

    //    // FIX: Extern types are very broken
    //    //assert_impl_all!(Box<ExternStruct>: DropFamily<Kind = NoDrop>);
    //    //assert_impl_all!(Box<&ExternStruct>: DropFamily<Kind = NeedsDrop>);
    //    //assert_impl_all!(Box<&mut ExternStruct>: DropFamily<Kind = NeedsDrop>);
    //    //assert_impl_all!(Box<&[ExternStruct]>: DropFamily<Kind = NeedsDrop>);
    //    //assert_impl_all!(Box<&mut [ExternStruct]>: DropFamily<Kind = NeedsDrop>);
    //    assert_impl_all!(Box<ExternRef<'static, u8>>: DropFamily<Kind = NeedsDrop>);
    //    assert_impl_all!(Box<ExternRefMut<'static, u8>>: DropFamily<Kind = NeedsDrop>);
    //}

    #[test]
    fn containers_delegate_drop_family() {
        assert_impl_all!([u8; 2]: DropFamily<Kind = NoDrop>);
        #[cfg(feature = "alloc")]
        assert_impl_all!([String; 2]: DropFamily<Kind = NeedsDrop>);

        assert_impl_all!(Option<u8>: DropFamily<Kind = NoDrop>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Option<String>: DropFamily<Kind = NeedsDrop>);

        assert_impl_all!(Result<u8, NonNull<u8>>: DropFamily<Kind = NoDrop>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(Result<String, u8>: DropFamily<Kind = NeedsDrop>);

        assert_impl_all!(UnsafeCell<u8>: DropFamily<Kind = NoDrop>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(UnsafeCell<String>: DropFamily<Kind = NeedsDrop>);
    }
}
