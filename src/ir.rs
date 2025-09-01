//! Internal representation, a.k.a IR of `Rust` types during conversion into FFI types.
//! While you can implement [`FfiType`] on your `Rust` type directly, it is encouraged
//! that you map your type into IR by providing the implementation of [`Ir`] and benefit
//! from automatic, correct and performant conversions from IR to C type equivalent.
use alloc::{boxed::Box, vec::Vec};

#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::InfallibleTransmute;
use crate::{Extern, LocalRef, LocalSlice, repr_c::Cloned};

/// Designates a type that can be converted to/from internal representation. Predefined IR
/// types are given automatic implementation of [`FfiType`] and other conversion traits.
pub trait Ir {
    /// Internal representation of the type.
    ///
    /// If the `Self` is [`ReprC`], [`Ir::Type`] should be set to [`Robust`] in which
    /// case the type will be used in the FFI function as given without any conversion.
    ///
    /// If the [`Ir::Type`] is set to [`Transparent`], `Self` will get an automatic
    /// implementation of [`FfiType`] that delegates to the inner type. If the conversion
    /// of the inner type is zero-copy conversion of [`Transparent`] will also be.
    ///
    /// If the [`Ir::Type`] is set to [`Opaque`], `T` will be serialized as an
    /// opaque pointer (the type is heap allocated during conversion). Except for `Vec<T>`,
    /// [`Opaque`] is currently the only family of types that transfer ownership over FFI.
    ///
    /// Otherwise, in the common case, [`Ir::Type`] should be set to `Self` and implement
    /// [`Cloned`] to benefit from the default implementation of [`FfiType`]. Be warned
    /// that in this case [`FfiType`] implementation will clone the given type.
    type Type;
}

/// When implemented for a type, defines how dependent types are mapped into [`Ir`]
pub trait IrTypeFamily {
    /// [`Ir`] type that `&T` is mapped into
    type Ref<'itm>
    where
        Self: 'itm;
    /// [`Ir`] type that `&mut T` is mapped into
    type RefMut<'itm>
    where
        Self: 'itm;
    /// [`Ir`] type that `&[T]` is mapped into
    type RefSlice<'itm>
    where
        Self: 'itm;
    /// [`Ir`] type that `&mut [T]` is mapped into
    type RefMutSlice<'itm>
    where
        Self: 'itm;
    /// [`Ir`] type that [`Box<T>`] is mapped into for any `T: Sized`
    type Box;
    /// [`Ir`] type that `Box<[T]>` is mapped into
    type BoxedSlice;
    /// [`Ir`] type that [`Vec<T>`] is mapped into
    type Vec;
    /// [`Ir`] type that `[T; N]` is mapped into
    type Arr<const N: usize>;
}

/// Type which is replaced by an opaque pointer on FFI import
///
/// # Safety
///
/// Type implementing must have the same representation as [`*mut Extern`]
/// `Self::RefType` must have the same representation as [`*const Extern`]
/// `Self::RefMutType` must have the same representation as [`*mut Extern`]
pub unsafe trait External {
    /// Type which replaces `&T` on FFI import
    type RefType<'itm>;
    /// Type which replaces `&mut T` on FFI import
    type RefMutType<'itm>;

    /// Return shared opaque pointer
    fn as_extern_ptr(&self) -> *const Extern;
    /// Return mutable opaque pointer
    fn as_extern_ptr_mut(&mut self) -> *mut Extern;
    /// Construct type from an opaque pointer
    ///
    /// # Safety
    ///
    /// The given opaque pointer must be non-null and valid
    unsafe fn from_extern_ptr(source: *mut Extern) -> Self;
}

/// Marker for a type that is transferred as an opaque pointer over FFI
#[derive(Debug, Clone, Copy)]
pub enum Opaque {}

/// Marker for a type that is transparent with respect to the wrapped type
#[derive(Debug, Clone, Copy)]
pub enum Transparent {}

/// Marker for a type that is a robust [`ReprC`] type and doesn't require conversion
#[derive(Debug, Clone, Copy)]
pub enum Robust {}

impl IrTypeFamily for Robust {
    type Ref<'itm> = Transparent;
    type RefMut<'itm> = Transparent;
    type RefSlice<'itm> = &'itm [Self];
    type RefMutSlice<'itm> = &'itm mut [Self];
    type Box = Box<Self>;
    type BoxedSlice = Box<[Self]>;
    type Vec = Vec<Self>;
    type Arr<const N: usize> = Self;
}
impl IrTypeFamily for Opaque {
    type Ref<'itm> = Transparent;
    type RefMut<'itm> = Transparent;
    type RefSlice<'itm> = &'itm [Self];
    type RefMutSlice<'itm> = &'itm mut [Self];
    type Box = Box<Self>;
    type BoxedSlice = Box<[Self]>;
    type Vec = Vec<Self>;
    type Arr<const N: usize> = [Self; N];
}
impl IrTypeFamily for Transparent {
    type Ref<'itm> = Self;
    type RefMut<'itm> = Self;
    type RefSlice<'itm> = &'itm [Self];
    type RefMutSlice<'itm> = &'itm mut [Self];
    type Box = Box<Self>;
    type BoxedSlice = Box<[Self]>;
    type Vec = Vec<Self>;
    type Arr<const N: usize> = Self;
}
impl<R: Cloned> IrTypeFamily for R {
    type Ref<'itm>
        = &'itm Self
    where
        Self: 'itm;
    // NOTE: Unused
    type RefMut<'itm>
        = ()
    where
        Self: 'itm;
    type RefSlice<'itm>
        = &'itm [Self]
    where
        Self: 'itm;
    // NOTE: Unused
    type RefMutSlice<'itm>
        = ()
    where
        Self: 'itm;
    type Box = Box<Self>;
    type BoxedSlice = Box<[Self]>;
    type Vec = Vec<Self>;
    type Arr<const N: usize> = [Self; N];
}
impl IrTypeFamily for &Extern {
    type Ref<'itm>
        = &'itm Self
    where
        Self: 'itm;
    type RefMut<'itm>
        = &'itm mut Self
    where
        Self: 'itm;
    type RefSlice<'itm>
        = &'itm [Self]
    where
        Self: 'itm;
    type RefMutSlice<'itm>
        = &'itm mut [Self]
    where
        Self: 'itm;
    type Box = Box<Self>;
    type BoxedSlice = Box<[Self]>;
    type Vec = Vec<Self>;
    type Arr<const N: usize> = [Self; N];
}
impl IrTypeFamily for &mut Extern {
    type Ref<'itm>
        = &'itm Self
    where
        Self: 'itm;
    type RefMut<'itm>
        = &'itm mut Self
    where
        Self: 'itm;
    type RefSlice<'itm>
        = &'itm [Self]
    where
        Self: 'itm;
    type RefMutSlice<'itm>
        = &'itm mut [Self]
    where
        Self: 'itm;
    type Box = Box<Self>;
    type BoxedSlice = Box<[Self]>;
    type Vec = Vec<Self>;
    type Arr<const N: usize> = [Self; N];
}

impl<R> Ir for *const R {
    type Type = Robust;
}
impl<R> Ir for *mut R {
    type Type = Robust;
}

impl<'itm, R: Ir> Ir for &'itm R
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::Ref<'itm>;
}
#[cfg(feature = "non_robust_ref_mut")]
impl<'itm, R: Ir> Ir for &'itm mut R
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::RefMut<'itm>;
}
#[cfg(not(feature = "non_robust_ref_mut"))]
impl<'itm, R: Ir + InfallibleTransmute> Ir for &'itm mut R
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::RefMut<'itm>;
}
impl<'itm, R: Ir> Ir for &'itm [R]
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::RefSlice<'itm>;
}
#[cfg(feature = "non_robust_ref_mut")]
impl<'itm, R: Ir> Ir for &'itm mut [R]
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::RefMutSlice<'itm>;
}
#[cfg(not(feature = "non_robust_ref_mut"))]
impl<'itm, R: Ir + InfallibleTransmute> Ir for &'itm mut [R]
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::RefMutSlice<'itm>;
}
impl<R: Ir> Ir for Box<R>
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::Box;
}
impl<R: Ir> Ir for Box<[R]>
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::BoxedSlice;
}
impl<R: Ir> Ir for Vec<R>
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::Vec;
}
impl<R: Ir, const N: usize> Ir for [R; N]
where
    R::Type: IrTypeFamily,
{
    type Type = <R::Type as IrTypeFamily>::Arr<N>;
}

impl<'itm, R: 'itm> Ir for LocalRef<'itm, R>
where
    &'itm R: Ir,
{
    type Type = <&'itm R as Ir>::Type;
}
impl<'itm, R: 'itm> Ir for LocalSlice<'itm, R>
where
    &'itm [R]: Ir,
{
    type Type = <&'itm [R] as Ir>::Type;
}
