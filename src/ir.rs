//! Internal Representation (IR) of Rust types during conversion into FFI types.
//!
//! While you can implement [`crate::FfiType`] directly on your type, it is often
//! preferable to map it into IR by implementing [`Ir`]. This approach gives you
//! automatic, correct, and zero-cost conversions from IR to the equivalent C type.
use alloc::{boxed::Box, vec::Vec};

#[cfg(not(feature = "non_robust_ref_mut"))]
use crate::transmute::InfallibleTransmute;
use crate::{Extern, LocalRef, LocalSlice, repr_c::Cloned};

/// Designates a type that can be converted to and from an internal representation (IR).
///
/// Predefined IR types automatically implement [`crate::FfiType`] and related conversion traits.
pub trait Ir {
    /// The internal representation (i.e. type family) of the type
    ///
    /// - If `Self` is [`crate::ReprC`], set [`Ir::Type`] to [`Robust`].
    ///   The type is passed to FFI functions as-is, without conversion.
    ///
    /// - If [`Ir::Type`] is [`Transparent`], `Self` automatically implements [`crate::FfiType`]
    ///   by delegating to its inner type via [`core::mem::transmute`].
    ///   If the inner type supports zero-copy conversion, then [`Transparent`] is also zero-copy.
    ///   See [`crate::Transmute`] for more details.
    ///
    /// - If [`Ir::Type`] is [`Opaque`], `T` is serialized as an opaque pointer.
    ///   Note that the type will be heap allocated during conversion if not already.
    ///   [`Opaque`] is the only family of types that transfer ownership across FFI.
    ///
    /// - If [`Ir::Type`] is [`Extern`], represents the pointee on the far side of an [`Opaque`] pointer
    ///
    /// - If [`Ir::Type`] is [`Option<T>`], `Option<T>` is transmuted into the inner type,
    ///   using its *niche value* to represent [`None`].
    ///
    /// - If [`Ir::Type`] is [`Option<WithoutNiche>`], serialization is delegated to the
    ///   inner type, but represented explicitly as a `(discriminant, value)` tuple.
    ///
    /// - In the common case, set [`Ir::Type`] to `Self` and implement [`Cloned`].
    ///   This provides a default [`crate::FfiType`] implementation, but note that it will clone the type.
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

/// Represents the pointee on the far side of an exported opaque pointer at the FFI boundary.
///
/// # Safety
///
/// Implementors must guarantee that:
/// - `Self` has the same representation as `*mut` [`Extern`].
/// - [`External::RefType`] has the same representation as `*const` [`Extern`].
/// - [`External::RefMutType`] has the same representation as `*mut` [`Extern`].
pub unsafe trait External {
    /// Type that replaces `&T` when imported over FFI.
    type RefType<'itm>;

    /// Type that replaces `&mut T` when imported over FFI.
    type RefMutType<'itm>;

    /// Returns a shared opaque pointer.
    fn as_extern_ptr(&self) -> *const Extern;

    /// Returns a mutable opaque pointer.
    fn as_extern_ptr_mut(&mut self) -> *mut Extern;

    /// Constructs `Self` from an opaque pointer.
    ///
    /// # Safety
    ///
    /// The pointer argument must be valid.
    unsafe fn from_extern_ptr(source: *mut Extern) -> Self;
}

/// Marker for a type exported as an opaque pointer over FFI.
pub enum Opaque {}

/// Marker for a type that is transparent with respect to its wrapped type.
pub enum Transparent {}

/// Marker for a robust [`crate::ReprC`] type that does not require conversion
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
    type RefMut<'itm>
        = void::Void
    where
        Self: 'itm;
    type RefSlice<'itm>
        = &'itm [Self]
    where
        Self: 'itm;
    type RefMutSlice<'itm>
        = void::Void
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
