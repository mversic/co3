//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

use crate::ReprC;
use crate::ir::Cloned;
#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::InfallibleTransmute;
use crate::{
    ExternC, assert_arr_has_non_zero_len,
    ir::{Opaque, Robust, Transparent},
    slice::{RefMutSlice, RefSlice},
    transmute::Transmute,
};

/// Type that utilizes niche optimization (e.g. `Option<T>`)
///
/// This is a type that can't just be transmuted into inner because it can have a niche
/// It's usually a marker for derivatives of `Option<T>` like `&Option<T>`, `&mut Option<T>` or `Box<Option<T>>`
///
/// # Safety
///
/// - type must have no trap representations
pub unsafe trait Optional {
    /// It would be incorrect to transmute into intermediate type
    /// but transmuting into end type is ok
    type Inner: ReprC;
}

disjoint_impls! {
    /// Type that has a trap representation that can be used as a niche value.
    pub trait Niche: ExternC {
        const NICHE_VALUE: Self::CType;
    }

    impl<R, C> Niche for Box<R>
    where
        Self: ExternC<CType = *mut C>,
    {
        const NICHE_VALUE: *mut C = core::ptr::null_mut();
    }

    impl<R, C> Niche for Box<R>
    where
        Self: ExternC<CType = *const C>,
    {
        const NICHE_VALUE: *const C = core::ptr::null();
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R, C> Niche for Box<[R]>
    where
        Self: ExternC<CType = RefSlice<C>>,
    {
        const NICHE_VALUE: RefSlice<C> = RefSlice::null();
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R, C> Niche for Box<[R]>
    where
        Self: ExternC<CType = RefMutSlice<C>>,
    {
        const NICHE_VALUE: RefMutSlice<C> = RefMutSlice::null_mut();
    }
}

disjoint_impls! {
    /// Type that has a compiler guaranteed trap representation that can be used as a [`Niche`] value.
    ///
    /// The stable niche value is made use of when serializing [`Option<T>`].
    ///
    /// # Example
    ///
    /// [`Option<bool>`]     - will be serilized into one byte
    /// [`Option<*const T>`] - will take the size of the pointer
    ///
    /// # Safety
    ///
    /// - the niche value must be congruent with what is guaranteed by the Rust compiler
    /// - if type is [`Transmute`], it must have the same niche as [`Transmute::Target`]
    /// - type must have exactly one trap representation
    pub unsafe trait StableNiche: Niche {}

    #[cfg(feature = "owned_as_ref")]
    unsafe impl<R, C> StableNiche for Box<R> where Self: ExternC<CType = *const C> {}
    unsafe impl<R, C> StableNiche for Box<R> where Self: ExternC<CType = *mut C> {}
}

disjoint_impls! {
    /// Used to implement specialized impls of [`crate::ir::Ir`] for [`Option<T>`]
    pub trait Ir {
        /// Internal representation of [`Option<T>`]
        type Type;
    }

    // TODO: Implement for Box<Robust> types
    impl<R: crate::ir::Ir<Type = Transparent>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Robust>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for &R {
        type Type = Transparent;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: crate::ir::Ir<Type: Cloned + 'a>> Ir for &'a R {
        type Type = &'a R::Type;
    }

    impl<
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &mut R
    where
        R: crate::ir::Ir<Type = Transparent>,
    {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Robust>> Ir for &mut R {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for &mut R {
        type Type = Transparent;
    }

    impl<R: crate::ir::Ir<Type = Transparent>> Ir for Box<R> {
        type Type = Transparent;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Robust>> Ir for Box<R> {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for Box<R> {
        type Type = Transparent;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type: Cloned>> Ir for Box<R> {
        type Type = Box<R::Type>;
    }

    impl<'a, R: crate::ir::Ir<Type = Transparent>> Ir for &'a [R] {
        type Type = &'a [Transparent];
    }
    impl<'a, R: crate::ir::Ir<Type = Robust>> Ir for &'a [R] {
        type Type = &'a [Robust];
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: crate::ir::Ir<Type = Opaque>> Ir for &'a [R] {
        type Type = &'a [Opaque];
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: crate::ir::Ir<Type: Cloned + 'a>> Ir for &'a [R] {
        type Type = &'a [R::Type];
    }

    impl<
        'a,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: InfallibleTransmute,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > Ir for &'a mut [R]
    where
        R: crate::ir::Ir<Type = Transparent>,
    {
        type Type = &'a mut [Transparent];
    }
    impl<'a, R: crate::ir::Ir<Type = Robust>> Ir for &'a mut [R] {
        type Type = &'a mut [Robust];
    }

    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Transparent>> Ir for Box<[R]> {
        type Type = Box<[Transparent]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Robust>> Ir for Box<[R]> {
        type Type = Box<[Robust]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for Box<[R]> {
        type Type = Box<[Opaque]>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type: Cloned>> Ir for Box<[R]> {
        type Type = Box<[R::Type]>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Transparent>> Ir for Vec<R> {
        type Type = Vec<Transparent>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Robust>> Ir for Vec<R> {
        type Type = Vec<Robust>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for Vec<R> {
        type Type = Vec<Opaque>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: crate::ir::Ir<Type: Cloned>> Ir for Vec<R> {
        type Type = Vec<R::Type>;
    }

    impl<R: crate::ir::Ir<Type = Transparent> + StableNiche, const N: usize> Ir for [R; N]
    where
        Option<Self>: crate::ir::Ir<Type = Option<Transparent>>,
    {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Transparent> + StableNiche, const N: usize> Ir for [R; N]
    where
        Option<Self>: crate::ir::Ir<Type = Option<Robust>>,
    {
        type Type = Robust;
    }
    impl<R: crate::ir::Ir<Type: Cloned> + StableNiche, S: Cloned, const N: usize> Ir for [R; N]
    where
        Option<Self>: crate::ir::Ir<Type = Option<S>>,
    {
        type Type = [<R as crate::ir::Ir>::Type; N];
    }
    impl<R: crate::ir::Ir<Type: Cloned> + StableNiche, const N: usize> Ir for [R; N]
    where
        Option<Self>: crate::ir::Ir<Type = Option<Robust>>,
    {
        type Type = Robust;
    }

    //impl<R: crate::ir::Ir<Type = Option<Robust>>> Ir for R {
    //    type Type = Option<Robust>;
    //}
    //impl<R: crate::ir::Ir<Type = Transparent>> Ir for Option<R> {
    //    type Type = Transparent;
    //}
    //impl<R: crate::ir::Ir<Type: Cloned>> Ir for Option<R> {
    //    type Type = Option<S>;
    //}
}

// TODO: Do it for all tuples in impl_tuple! macro

//impl<R: crate::niche::Ir<Type = Robust>, const N: usize> Ir for [] where (R,): Ir<Type = Robust> {
//    type Type = Robust;
//}
//impl<R: crate::ir::Ir<Type: Cloned>, const N: usize> Ir for [(R,); N] where (R,): crate::niche::Ir<Type = Robust> {
//    type Type = Robust;
//}

impl<R, C> Niche for &R
where
    Self: ExternC<CType = *const C>,
{
    const NICHE_VALUE: *const C = core::ptr::null();
}

impl<R, C> Niche for &mut R
where
    Self: ExternC<CType = *mut C>,
{
    const NICHE_VALUE: *mut C = core::ptr::null_mut();
}

impl<R, C> Niche for &[R]
where
    Self: ExternC<CType = RefSlice<C>>,
{
    const NICHE_VALUE: RefSlice<C> = RefSlice::null();
}

impl<R, C> Niche for &mut [R]
where
    Self: ExternC<CType = RefMutSlice<C>>,
{
    const NICHE_VALUE: RefMutSlice<C> = RefMutSlice::null_mut();
}

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
impl<R, C> Niche for Vec<R>
where
    Self: ExternC<CType = RefSlice<C>>,
{
    const NICHE_VALUE: RefSlice<C> = RefSlice::null();
}

impl<R: Niche, const N: usize> Niche for [R; N]
where
    Self: ExternC<CType = [R::CType; N]>,
{
    const NICHE_VALUE: [R::CType; N] = {
        assert_arr_has_non_zero_len::<N>();
        [R::NICHE_VALUE; N]
    };
}

unsafe impl<R, C> StableNiche for &R where Self: ExternC<CType = *const C> {}
unsafe impl<R, C> StableNiche for &mut R where Self: ExternC<CType = *mut C> {}
unsafe impl<R> StableNiche for core::ptr::NonNull<R> {}

unsafe impl<R: Transmute + StableNiche> Optional for Option<R> {
    type Inner = R::CType;
}

unsafe impl<R: Optional> Optional for &R {
    type Inner = *const R::Inner;
}

unsafe impl<R: Optional> Optional for &mut R {
    type Inner = *mut R::Inner;
}

// TODO: Should it be conditionally enabled?
//#[cfg(feature = "owned_types")]
unsafe impl<R: Optional> Optional for Box<R> {
    type Inner = *mut R::Inner;
}

unsafe impl<R: Optional, const N: usize> Optional for [R; N] {
    type Inner = [R::Inner; N];
}

// TODO: Impl for derivative types
//unsafe impl<R: Optional> Optional for UnsafeCell<R> {
//    type Inner = R::Inner;
//}
