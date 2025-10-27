//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

use alloc::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

#[cfg(feature = "cloned_types")]
use crate::ir::Cloned;
use crate::{
    ExternC, ReprC, assert_arr_has_non_zero_len,
    ir::{Opaque, Robust, Transparent},
    slice::{RefMutSlice, RefSlice},
    transmute::Transmute,
};

/// Type that utilizes niche optimization (e.g. `Option<T>`)
///
/// This is a type that can't just be transmuted into inner because it can have a niche
/// It's usually a marker for derivatives of `Option<T>` like `&Option<T>`, `&mut Option<T>` or `Box<Option<T>>`
pub unsafe trait Optional {
    /// It would be incorrect to transmute into intermediate type
    /// but transmuting into end type is ok
    type Inner: ReprC;
}

disjoint_impls! {
    /// Type that has at least one trap representation that can be used as a niche value.
    ///
    /// The niche value is used in the serialization of [`Option<T>`]. For example, [`Option<bool>`]
    /// will be serilized into one byte and [`Option<*const T>`] will take the size of the pointer.
    ///
    /// Trait is not unsafe because incorrect implementations can lead only to data corruption, not UB
    pub trait Niche: ExternC {
        /// The niche value of the type
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
    /// Used to implement specialized impls of [`crate::ir::Ir`] for [`Option<T>`]
    pub trait Ir {
        /// Internal representation of [`Option<T>`]
        type Type;
    }

    impl<R: crate::ir::Ir<Type = Transparent>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Robust>> Ir for &R {
        type Type = Transparent;
    }
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for &R {
        type Type = Transparent;
    }
    #[cfg(feature = "cloned_types")]
    impl<'a, R: crate::ir::Ir<Type = S>, S: Cloned + 'a> Ir for &'a R {
        type Type = &'a S;
    }

    impl<R: crate::ir::Ir<Type = Transparent>> Ir for &mut R {
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
    #[cfg(feature = "cloned_types")]
    impl<R: crate::ir::Ir<Type = S>, S: Cloned> Ir for Box<R> {
        type Type = Box<S>;
    }

    impl<'a, R: crate::ir::Ir<Type = Transparent>> Ir for &'a [R] {
        type Type = &'a [Transparent];
    }
    impl<'a, R: crate::ir::Ir<Type = Robust>> Ir for &'a [R] {
        type Type = &'a [Robust];
    }
    #[cfg(feature = "cloned_types")]
    impl<'a, R: crate::ir::Ir<Type = Opaque>> Ir for &'a [R] {
        type Type = &'a [Opaque];
    }
    #[cfg(feature = "cloned_types")]
    impl<'a, R: crate::ir::Ir<Type = S>, S: Cloned + 'a> Ir for &'a [R] {
        type Type = &'a [S];
    }

    impl<'a, R: crate::ir::Ir<Type = Transparent>> Ir for &'a mut [R] {
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
    #[cfg(feature = "cloned_types")]
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for Box<[R]> {
        type Type = Box<[Opaque]>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "cloned_types")]
    impl<R: crate::ir::Ir<Type = S>, S: Cloned> Ir for Box<[R]> {
        type Type = Box<[S]>;
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
    #[cfg(feature = "cloned_types")]
    impl<R: crate::ir::Ir<Type = Opaque>> Ir for Vec<R> {
        type Type = Vec<Opaque>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "cloned_types")]
    impl<R: crate::ir::Ir<Type = S>, S: Cloned> Ir for Vec<R> {
        type Type = Vec<S>;
    }

    impl<R: crate::ir::Ir<Type = Transparent> + Niche, const N: usize> Ir for [R; N] where Option<Self>: crate::ir::Ir<Type = Option<Transparent>> {
        type Type = Transparent;
    }
    // FIXME: Need to support the disjoint_impls upstream
    //impl<R: crate::ir::Ir<Type = Transparent> + Niche, const N: usize> Ir for [R; N] where Option<Self>: crate::ir::Ir<Type = Option<Robust>> {
    //    type Type = Robust;
    //}
    #[cfg(feature = "cloned_types")]
    impl<R: crate::ir::Ir<Type = S> + Niche, S: Cloned, const N: usize> Ir for [R; N] where Option<Self>: crate::ir::Ir<Type = Option<S>> {
        type Type = [S; N];
    }
    //#[cfg(feature = "cloned_types")]
    //impl<R: crate::ir::Ir<Type = S> + Niche, S: Cloned, const N: usize> Ir for [R; N] where Option<Self>: crate::ir::Ir<Type = Option<Robust>> {
    //    type Type = Robust;
    //}

    //impl<R: crate::ir::Ir<Type = Option<Robust>>> Ir for R {
    //    type Type = Option<Robust>;
    //}
    //impl<R: crate::ir::Ir<Type = Transparent>> Ir for Option<R> {
    //    type Type = Transparent;
    //}
    //#[cfg(feature = "cloned_types")]
    //impl<R: crate::ir::Ir<Type = S>, S: Cloned> Ir for Option<R> {
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

// TODO: Find a way to implement for all recursively wrapped
// `Option<R>` where `R` has multiple niche values
impl Niche for Option<bool> {
    const NICHE_VALUE: <u8 as ExternC>::CType = 3;
}

unsafe impl<R: Niche + Transmute> Optional for Option<R> {
    type Inner = R::CType;
}

unsafe impl<R: Optional> Optional for &R {
    type Inner = *const R::Inner;
}

unsafe impl<R: Optional> Optional for &mut R {
    type Inner = *mut R::Inner;
}

#[cfg(feature = "owned_types")]
unsafe impl<R: Optional> Optional for Box<R> {
    type Inner = *mut R::Inner;
}

unsafe impl<R: Optional, const N: usize> Optional for [R; N] {
    type Inner = [R::Inner; N];
}
