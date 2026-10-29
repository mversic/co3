use crate::transmute::AbiCompatibleValidate;
#[cfg(feature = "alloc")]
pub use alloc::borrow::ToOwned;
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec,
    mutability::{Exclusive, WithInterior, WithoutInterior},
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    niche::Niche,
    stored::{EncodeMove, EncodeMoveUnchecked, Store},
};
use crate::{stored::DecodeMove, transmute::AbiCompatible};

disjoint_impls! {
    /// A trait for structurally borrowing data.
    ///
    /// # Safety
    ///
    /// `Borrowed` must be correctly categorized as:
    ///
    /// - `Self` for types without drop glue
    /// - structural view for types with drop glue
    /// - [`Borrowed<Self>`] for types with custom drop
    pub unsafe trait Borrow: Sized {
        type Borrowed;

        fn borrow(self) -> Self::Borrowed;

        /// Construct owned form from [`Borrow::Borrowed`]
        fn from_borrow(source: Self::Borrowed) -> Self;
    }

    unsafe impl<R: ?Sized> Borrow for &R
    where
        R: RustSpec<Mutability = Exclusive<WithoutInterior>>
    {
        type Borrowed = Self;

        #[inline(always)]
        fn borrow(self) -> Self::Borrowed {
            self
        }

        #[inline(always)]
        fn from_borrow(source: Self::Borrowed) -> Self {
            source
        }
    }
    unsafe impl<R: ?Sized, I> Borrow for &R
    where
        R: RustSpec<Mutability = Exclusive<WithInterior<I>>>
    {
        type Borrowed = Borrowed<Self>;

        #[inline(always)]
        fn borrow(self) -> Self::Borrowed {
            Borrowed(self)
        }

        #[inline(always)]
        fn from_borrow(source: Self::Borrowed) -> Self {
            source.0
        }
    }
    unsafe impl<R: ?Sized, I> Borrow for &R
    where
        R: RustSpec<Mutability = WithInterior<I>>
    {
        type Borrowed = Borrowed<Self>;

        #[inline(always)]
        fn borrow(self) -> Self::Borrowed {
            Borrowed(self)
        }

        #[inline(always)]
        fn from_borrow(source: Self::Borrowed) -> Self {
            source.0
        }
    }
}

/// A transparent wrapper for a value passed through the borrowed conversion path.
///
/// Foreign consumers must only read the carrier, even when its pointer type permits writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
#[repr(transparent)]
pub struct Borrowed<T>(T);

impl<T> Borrowed<T> {
    /// Wrap a value for read-only borrowed conversion, taking ownership of it.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    #[inline]
    pub fn into_inner(value: Self) -> T {
        value.0
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Borrowed<T> {
    type CType = Borrowed<T::CType>;
}

impl<'d, R> crate::Decode<'d> for Borrowed<R> where Self: DecodeMove<'d> {}
impl<T> crate::EncodeUnchecked for Borrowed<T> where Self: EncodeMoveUnchecked {}
impl<T> crate::Encode for Borrowed<T> where Self: EncodeMove {}

unsafe impl<C: CType> CType for Borrowed<C> {}
unsafe impl<C: CFnArg> CFnArg for Borrowed<C> {}
unsafe impl<C: CFnReturn> CFnReturn for Borrowed<C> {}

unsafe impl<T: AbiCompatibleValidate<CType: Sized>> AbiCompatibleValidate for Borrowed<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(&target.0) }
    }
}
unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatible for Borrowed<T> {}

impl<T: Niche> Niche for Borrowed<T> {
    const NICHE: Self::CType = Borrowed(T::NICHE);
}

unsafe impl<R: ?Sized> Borrow for &mut R {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.0
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: ?Sized> Borrow for Box<R> {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.0
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R> Borrow for Vec<R> {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed::new(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        Borrowed::into_inner(source)
    }
}

unsafe impl<R> Borrow for Borrowed<R> {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source
    }
}

#[cfg(feature = "alloc")]
unsafe impl Borrow for alloc::ffi::CString {
    type Borrowed = Borrowed<Self>;

    fn borrow(self) -> Self::Borrowed {
        Borrowed::new(self)
    }

    fn from_borrow(source: Self::Borrowed) -> Self {
        Borrowed::into_inner(source)
    }
}
unsafe impl<R: Borrow, const N: usize> Borrow for [R; N] {
    type Borrowed = [R::Borrowed; N];

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self.map(Borrow::borrow)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.map(R::from_borrow)
    }
}

unsafe impl<R: Borrow> Borrow for Option<R> {
    type Borrowed = Option<R::Borrowed>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self.map(Borrow::borrow)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source.map(R::from_borrow)
    }
}

unsafe impl<T: Borrow, E: Borrow> Borrow for Result<T, E> {
    type Borrowed = Result<T::Borrowed, E::Borrowed>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self.map(Borrow::borrow).map_err(Borrow::borrow)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        match source {
            Ok(value) => Ok(T::from_borrow(value)),
            Err(err) => Err(E::from_borrow(err)),
        }
    }
}

/// Retains the original value until reference conversion and synchronization finish.
pub struct BorrowedEncodeStore<T, S> {
    value: Option<T>,
    reference: S,
}
/// Keeps the carrier alive for references produced during borrowed decoding.
pub struct BorrowedDecodeStore<C, S> {
    carrier: Option<C>,
    reference: S,
}

impl<C, S> BorrowedDecodeStore<C, S> {
    pub(crate) fn insert(&mut self, carrier: C) -> (&C, &mut S) {
        (self.carrier.insert(carrier), &mut self.reference)
    }
}

impl<T, S: Default> Default for BorrowedEncodeStore<T, S> {
    fn default() -> Self {
        Self {
            value: None,
            reference: S::default(),
        }
    }
}
impl<C, S: Default> Default for BorrowedDecodeStore<C, S> {
    fn default() -> Self {
        Self {
            carrier: None,
            reference: S::default(),
        }
    }
}

impl<T, S: Store> Store for BorrowedEncodeStore<T, S> {
    fn sync(self) -> Option<()> {
        let Self { value, reference } = self;
        let result = reference.sync();
        drop(value);
        result
    }
}
impl<C, S: Store> Store for BorrowedDecodeStore<C, S> {
    fn sync(self) -> Option<()> {
        let Self { carrier, reference } = self;
        let result = reference.sync();
        drop(carrier);
        result
    }
}


// Keep the higher-ranked bound outside disjoint_impls, which cannot expand its binder.
#[doc(hidden)]
// TODO: This trait should be removable IMO
pub trait BorrowedOwnerEncode: ReprC<CType: Copy> + Sized {
    type ReferenceStore: Store + Default;
    fn encode_borrowed(
        self,
        store: &mut BorrowedEncodeStore<Self, Self::ReferenceStore>,
    ) -> Borrowed<Self::CType>;
}
impl<T: ReprC<CType: Copy>, S: Store + Default> BorrowedOwnerEncode for T
where
    for<'a> &'a mut T:
        EncodeMoveUnchecked<SoftStore = S, CType = crate::reference::CRefMut<T::CType, true>>,
{
    type ReferenceStore = S;
    fn encode_borrowed(
        self,
        store: &mut BorrowedEncodeStore<Self, Self::ReferenceStore>,
    ) -> Borrowed<Self::CType> {
        let value = store.value.insert(self);
        // SAFETY: The returned borrowed carrier permits only read-only foreign access.
        let encoded = unsafe { value.soft_encode_unchecked(&mut store.reference) };
        // SAFETY: The encoded carrier is initialized and Copy, with its owner retained.
        Borrowed::new(unsafe { encoded.as_ptr().read() })
    }
}
