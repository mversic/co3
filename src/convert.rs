//! Conversion functions and traits for C-compatible representations.

#[cfg(feature = "alloc")]
pub use crate::stored::{AssignFromOwned, Owned};
pub(crate) use crate::stored::{encode_ref_field, encode_ref_field_unchecked};
#[cfg(feature = "alloc")]
use crate::{RustSpec, StableLayout, Unstable, boxed::Box};
pub use crate::{
    // FIXME: This trait doesn't belong here
    abi::AbiCompatibleValidate,
    decoded::{SoftDecodeMut, SoftDecodeRef},
    mutable::{SoftEncodeMut, SoftEncodeMutUnchecked},
    shared::{SoftEncodeRef, SoftEncodeRefUnchecked},
    stored::{
        DecodeMove, EmptyStore, EncodeMove, EncodeMoveUnchecked, Store, decode_move, encode_move,
    },
};

/// Facilitates unchecked conversion from a Rust type into a C-compatible representation.
pub trait EncodeUnchecked: EncodeMoveUnchecked {}

/// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
pub trait Encode: EncodeMove + EncodeUnchecked {}

/// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
pub trait Decode<'d>: DecodeMove<'d> {}

#[cfg(feature = "alloc")]
const _: () = {
    // An unstable box representation can still transfer its allocation when the
    // pointee layout is stable (for example, a boxed slice).
    trait BoxConversionLayout<R: ?Sized> {}
    impl<R: ?Sized> BoxConversionLayout<R> for StableLayout {}
    impl<R: RustSpec<Layout = StableLayout> + ?Sized> BoxConversionLayout<R> for Unstable {}

    impl<R: ?Sized> EncodeUnchecked for Box<R> where
        Self: RustSpec<Layout: BoxConversionLayout<R>> + EncodeMoveUnchecked
    {
    }
    impl<R: ?Sized> Encode for Box<R> where Self: RustSpec<Layout: BoxConversionLayout<R>> + EncodeMove {}
    impl<'d, R: ?Sized> Decode<'d> for Box<R> where
        Self: RustSpec<Layout: BoxConversionLayout<R>> + DecodeMove<'d>
    {
    }
};

impl<R: ?Sized> Encode for &R where Self: EncodeMove {}
impl<R: ?Sized> Encode for &mut R where Self: EncodeMove {}
impl<R: ?Sized> EncodeUnchecked for &R where Self: EncodeMoveUnchecked {}
impl<R: ?Sized> EncodeUnchecked for &mut R where Self: EncodeMoveUnchecked {}

impl<'d, R: ?Sized> Decode<'d> for &'d R where Self: DecodeMove<'d> {}
impl<'d, R: ?Sized> Decode<'d> for &'d mut R where Self: DecodeMove<'d> {}

impl<R: Encode, const N: usize> Encode for [R; N] where Self: EncodeMove {}
impl<R: EncodeUnchecked, const N: usize> EncodeUnchecked for [R; N] where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>, const N: usize> Decode<'d> for [R; N] where Self: DecodeMove<'d> {}
