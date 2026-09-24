//! _Rust-native declarations_ for importing and exporting C ABI interfaces.
//!
//! **[`ffi!`] is the main entry point.** It generates ABI-facing wrappers and conversion glue for
//! statics, functions, impl blocks, and opaque types. Each block can either _export_ declarations
//! from Rust with `#![unsafe(export("ABI"))]` or _import_ them from a foreign library with
//! `#![unsafe(extern("ABI"))]`.
//!
//! **[`ReprC`] derive generates the C-compatbile type and the corresponding conversions.** Value
//! representations are checked for traps, including pointees, and ownership transfer is _opt-in_.
//!
//! Except for statics, **export declarations are interchangeable with import declarations**.
//!
//! # Safety
//!
//! An import declaration is an unsafe assertion that the **linked provider matches the declared**
//! ABI contract, including symbol names, calling convention, complete item signatures, ownership,
//! lifetime, mutability, and aliasing requirements.
//!
//! Foreign callers must ensure that non-null pointer-backed arguments satisfy Rust's reference
//! rules for the types declared by the exported function for the duration of the call.
//!
//! The exporting crate must ensure that **exported symbol names are unique among linked definitions**.
//! Multiple `ffi!` blocks can generate colliding names, so prefer a single export block per crate
//! and prefer exporting **only** symbols defined in that crate.
//!
//! # Import from an external library
//!
//! Use **`#![unsafe(extern("ABI"))]`** to declare the Rust-facing interface implemented by an
//! external library:
//!
//! ```rust
//! use co3::{ffi, ReprC};
//!
//! // `ReprC` generates a stable C-compatible companion even without an explicit `#[repr(...)]`.
//! // If given, an explicit representation would be leveraged to produce a more optimal mapping.
//! #[derive(Clone, Copy, ReprC)]
//! struct Value(u32);
//!
//! trait Counter {
//!    fn increment(&mut self, by: Value);
//! }
//!
//! ffi! {
//!    #![unsafe(extern("C"))]
//!
//!    type CounterHandle;
//!
//!    impl Counter for CounterHandle {
//!        fn increment(&mut self, by: Value);
//!    }
//! }
//! # fn main() {}
//! ```
//!
//! # Export from Rust
//!
//! Use **`#![unsafe(export("ABI"))]`** to expose existing Rust items. Although an ABI can be
//! exported on its own, the provider will often also provide a Rust client as well. In the common
//! case, export declarations are naturally paired with matching import declarations via a shared
//! crate that defines the application interface.
//!
//! ```rust
//! use co3::ffi;
//!
//! #[cfg(not(feature = "import"))]
//! pub struct CounterHandle(u32);
//!
//! #[cfg(not(feature = "import"))]
//! impl core::ops::AddAssign<u32> for CounterHandle {
//!    fn add_assign(&mut self, rhs: u32) {
//!        self.0 += rhs;
//!    }
//! }
//!
//! #[cfg(not(feature = "import"))]
//! fn increment<T: core::ops::AddAssign<u32>>(value: &mut T) {
//!    *value += 1;
//! }
//!
//! ffi! {
//!    #![cfg_attr(not(feature = "import"), unsafe(export("C")))]
//!    #![cfg_attr(feature = "import", unsafe(extern("C")))]
//!
//!    type CounterHandle;
//!
//!    fn increment(value: &mut CounterHandle);
//! }
//! # fn main() {}
//! ```
//!
//! # Naming convention
//!
//! ABI symbols follow a stable naming convention:
//!
//! - Static items: `{prefix}__{static_name}`
//! - Free functions: `{prefix}__{function_name}`
//! - Inherent methods: `{prefix}__{SelfType}__{method_name}`
//! - Trait methods: `{prefix}__{TraitPath}__{SelfType}__{method_name}`
//!
//! By default, `ffi!` uses `CARGO_CRATE_NAME` as the symbol prefix. `#![symbol_prefix = "..."]`
//! overrides the prefix for the entire scope, whereas `#[symbol_name = "..."]` overrides the name
//! for the declaration it is applied to:
//!
//! ```rust
//! use co3::ffi;
//!
//! trait Operations {
//!    fn add(&self, left: u32, right: u32) -> u32;
//! }
//!
//! ffi! {
//!    #![unsafe(extern("C"))]
//!    #![symbol_prefix = "my_lib"]
//!
//!    type Calculator;
//!
//!    // Linked as `my_lib__subtract`.
//!    fn subtract(left: u32, right: u32) -> u32;
//!
//!    impl Calculator {
//!        // Linked as `my_lib__Calculator__multiply`.
//!        fn multiply(&self, left: u32, right: u32) -> u32;
//!    }
//!
//!    impl Operations for Calculator {
//!        // Linked as `library_add`.
//!        #[symbol_name = "library_add"]
//!        fn add(&self, left: u32, right: u32) -> u32;
//!    }
//! }
//! # fn main() {}
//! ```
//!
//! # Static parameter interpolation
//!
//! A generic parameter is selected at compile time with `where use<Param, ...> @ (ConcreteTy | ...)`
//! where each selection of a static parameter generates a specialized function export or import with
//! a distinct ABI symbol.
//!
//! An explicit `#[symbol_name]` **MUST** interpolate every selected static parameter exactly once
//! using `{Parameter}`. `#![symbol_fragments { ... }]` defines the interpolated string value.
//!
//! ```rust
//! use co3::ffi;
//!
//! pub struct SQLCHAR;
//! pub struct SQLWCHAR;
//!
//! ffi! {
//!    #![unsafe(extern("C"))]
//!    #![symbol_fragments {
//!        SQLCHAR = "A",
//!        SQLWCHAR = "W",
//!    }]
//!
//!    #[symbol_name = "convert_{T}"]
//!    fn convert<T>()
//!    where
//!        use<T> @ (<SQLCHAR> | <SQLWCHAR>);
//! }
//! # fn main() {}
//! ```
//!
//! This synthesizes 2 function imports: `convert_A` and `convert_W`. Primitive types have stable
//! built-in fragments, while const arguments are converted directly into valid symbol fragments.
//!
//! **Runtime-dispatched `dyn(...)` parameters use tags instead and are not interpolated.**
//!
//! # Tagged dispatch
//!
//! Tagged dispatch is a dynamic dispatch over a closed set of concrete implementations commonly used
//! in C APIs. The concrete type is erased at the FFI boundary and carried as a shared representation
//! accompanied by a tag that identifies the concrete implementation to invoke. The tag position is
//! inferred at the start of the function parameter list but can also be specified explicitly. Every
//! tag-dispatched concrete instantiation is compile-time checked to have a C-compatible representation
//! with the same size and alignment of the declared shared ABI type.
//!
//! In the following example:
//! - `T` is a tag-dispatched type parameter
//! - The tag type of parameter `T` is chosen as `u8`
//! - `u16` specifies the shared ABI representation used in the place of every concrete `T`.
//! - `where use<T> @ (...)` defines the set of concrete instantiations of dispatched types.
//! - `dyn Self` dispatches the `Self` parameter, but is only allowed for extern/opaque types.
//!
//! The following example demonstrates both forms of tagged dispatch:
//!
//! ```rust
//! use co3::{ffi, Tag, ReprC, rust_spec::RustSpec};
//! use co3::tag::Tagged;
//!
//! #[derive(RustSpec, Tag, ReprC)]
//! #[tag(u8, unsafe(1))]
//! #[repr(transparent)]
//! struct LocalCounter(u16);
//!
//! #[derive(RustSpec, Tag, ReprC)]
//! #[tag(u8, unsafe(2))]
//! #[repr(transparent)]
//! struct RemoteCounter(u16);
//!
//! trait Counter {
//!    fn increment(&mut self, by: u8);
//! }
//!
//! trait Reset {
//!    fn reset(&mut self);
//! }
//!
//! unsafe impl Tagged for CounterHandle<i16> {
//!    const TAG: u8 = 3;
//! }
//!
//! unsafe impl Tagged for CounterHandle<u16> {
//!    const TAG: u8 = 4;
//! }
//!
//! ffi! {
//!    #![unsafe(extern("C"))]
//!
//!    #[tag(u8)]
//!    type CounterHandle<T>;
//!
//!    impl<T> Drop for CounterHandle<T> {
//!        fn drop(&mut self);
//!    }
//!
//!    // Declare `T` as tag dispatched
//!    impl<dyn(u8) T = u16> Counter for T
//!    where
//!        // Select concrete types with the shared `u16` ABI representation.
//!        use<T> @ (<LocalCounter> | <RemoteCounter>)
//!    {
//!        // Make the tag-carrying argument position explicit.
//!        fn increment(t_tag: <dyn T>::TAG, &mut self, by: u8);
//!    }
//!
//!    // Dispatch the extern type itself
//!    impl<T> Reset for dyn CounterHandle<T>
//!    where
//!        use<T> @ (<i16> | <u16>)
//!    {
//!        fn reset(self_tag: <dyn Self>::TAG, &mut self);
//!    }
//! }
//! # fn main() {}
//! ```
//!
//! # Ownership transfer
//!
//! In Rust, passing a value to a function transfers its ownership. Across an FFI boundary, however,
//! ownership transfer carries additional requirements which are a common cause of UB, such as
//! agreeing on the allocator and who is responsible for freeing the allocation.
//!
//! Because it is designed to eliminate common footguns in FFI, `CO3` takes an opinionated stance here.
//! By default, all owned values (e.g. `Vec<T>`) are exported as references and immediately cloned on
//! the importing side. The universal guarantee is that any reference be valid for the duration of
//! the function call; past that, it is the user's responsibility to ensure reference validity.
//!
//! Apply `move` to transfer ownership of an argument or return value:
//!
//! ```rust
//! use co3::ffi;
//!
//! fn passthrough(input: Vec<u8>) -> Vec<u8> {
//!    input
//! }
//!
//! fn clone_into(input: Vec<u8>) {
//!    drop(input);
//! }
//!
//! ffi! {
//!    #![unsafe(export("C"))]
//!
//!    // `input` and the return both transfer ownership
//!    fn passthrough(input: move Vec<u8>) -> move Vec<u8>;
//!
//!    // `input` is passed by reference
//!    fn clone_into(input: Vec<u8>);
//! }
//! # fn main() {}
//! ```
//! # Soft references
//!
//! `#[soft]` opts a function argument into conversion through temporary backing storage. This enables
//! mutable references to non-robust values (e.g. `&mut bool`) or values that cannot be represented
//! in place (e.g. `&(u32, u32)`). The caveat is that the referent is converted into a temporary
//! C-compatible representation instead of reusing its original address.
//! **Pointer identity is not preserved.**
//!
//! Changes made through mutable references are synchronized back to the original value after the
//! call. A synchronization failure follows the configured failure mode.
//!
//! Apply `#[soft]` to a function argument to opt into this kind of conversion:
//!
//! ```rust
//! use co3::ffi;
//!
//! fn increment(value: (&(u8, u32), u32)) -> u8 {
//!    value.0.0 + 1
//! }
//!
//! ffi! {
//!    #![unsafe(export("C"))]
//!
//!    fn increment(#[soft] value: (&(u8, u32), u32)) -> u8;
//! }
//! # fn main() {}
//! ```
//!
//! # Unpacking at the ABI boundary
//!
//! Rust slices are lowered into [`slice::CSlice`]/[`slice::CSliceMut`]
//! which are C-ABI containers holding a data pointer and a length. However, it is common for FFI APIs to instead accept those components as separate function arguments.
//! Mark an argument with `#[unpack(T1, T2)]` to import its two ABI parts:
//!
//! ```rust
//! # use co3::ffi;
//!
//! ffi! {
//!    #![unsafe(extern("system"))]
//!
//!    // - imported as `sum(*const u32, usize)`
//!    fn sum(#[unpack(_, usize)] values: &[u32]) -> u32;
//! }
//! ```
//!
//! **This pattern is not limited to slices**; it applies to every type implementing the
//! [`slice::Unpack2`] trait.
//!
//! # Failure modes
//!
//! Select how failures are reported over the FFI boundary:
//! - `#![failure = "error"]`: return type must implement [`Error`].
//! - `#![failure = "panic"]`: panic on failure (the default).
//!
//! ```rust
//! use co3::{ffi, Error, ReprC, rust_spec::RustSpec};
//!
//! #[derive(RustSpec, ReprC)]
//! #[repr(u8)]
//! enum ApiError { InvalidRepresentation, UnknownHandle, SoftSync }
//!
//! impl Error for ApiError {
//!    fn trap_value() -> Self { Self::InvalidRepresentation }
//!    fn unknown_tag() -> Self { Self::UnknownHandle }
//!    fn soft_sync_error() -> Self { Self::SoftSync }
//! }
//!
//! fn check_error(value: u8) -> ApiError {
//!    let _ = value;
//!    ApiError::InvalidRepresentation
//! }
//!
//! ffi! {
//!    #![unsafe(export("C"))]
//!    #![failure = "error"]
//!
//!    fn check_error(value: u8) -> ApiError;
//! }
//!
//! ffi! {
//!    #![unsafe(extern("C"))]
//!
//!    fn check_panic(value: u8) -> u32;
//! }
//! # fn main() {}
//! ```
//!
//! # Callbacks
//!
//! Function pointer parameters are not converted by [`ffi!`] which limits the available signatures:
//!
//! ```rust
//! use co3::{ReprC, ffi, rust_spec::RustSpec};
//!
//! #[derive(Clone, Copy, RustSpec, ReprC)]
//! // Without `#[reprC(identity)]`, `ReprC` derive produces `CValue`, in which case you'd
//! // have to use that type (or `<Value as ExternC>::CType`) in your callback instead
//! #[reprC(identity)]
//! #[repr(transparent)]
//! struct Value(u8);
//!
//! // Fn pointer **MUST BE** C-compatible in itself
//! type Callback = extern "C" fn(Value, u8) -> Value;
//!
//! fn apply_callback(callback: Callback, value: Value) -> Value {
//!     callback(value, 1)
//! }
//!
//! ffi! {
//!     #![unsafe(export("C"))]
//!
//!     fn apply_callback(callback: Callback, value: Value) -> Value;
//! }
//! # fn main() {}
//! ```
//!
//! Otherwise, perform the conversion manually:
//!
//! ```rust
//! use co3::{ReprC, ffi, rust_spec::RustSpec};
//!
//! #[derive(Clone, Copy, RustSpec, ReprC)]
//! struct Value(u8);
//!
//! type Callback = extern "C" fn(CValue, u8) -> <Value as co3::ExternC>::CType;
//!
//! fn apply_callback(callback: Callback, value: Value) -> Value {
//!     // If the type contains soft references use:
//!     //     let mut store = Default::default();
//!     //     co3::soft_encode(value, &mut store)
//!     let value = co3::encode(value);
//!     let output = callback(value, 1);
//!
//!     unsafe { co3::decode(output) }.unwrap()
//! }
//!
//! ffi! {
//!     #![unsafe(export("C"))]
//!
//!     fn apply_callback(callback: Callback, value: Value) -> Value;
//! }
//! # fn main() {}
//! ```
//!
//! # Raw Functions
//!
//! A `raw` import declaration synthesizes a C-compatible function under the name `{fn_name}_raw`.
//! The raw function decodes inputs, calls the Rust function, encodes the output and returns. The
//! following example shows how this helps with callbacks:
//!
//! ```rust
//! use co3::{ExternC, ReprC, ffi, rust_spec::RustSpec};
//! #
//! # #[unsafe(export_name = "doc_register_callback")]
//! # extern "C" fn callback_receiver(_: Callback) {}
//! #
//! # #[unsafe(export_name = "doc_register_method_callback")]
//! # extern "C" fn method_callback_receiver(_: MethodCallback) {}
//!
//! #[derive(RustSpec, ReprC)]
//! #[repr(transparent)]
//! struct Value(u8);
//!
//! impl Value {
//!     // If you have an existing method
//!     fn doubled(&self) -> Self {
//!         Self(self.0 * 2)
//!     }
//! }
//!
//! // If you have an existing function
//! fn increment(value: Box<u8>) -> Value {
//!     Value(*value + 1)
//! }
//!
//! ffi! {
//!     #![unsafe(extern("C"))]
//!
//!     type Callback = raw fn(move Box<u8>) -> Value;
//!     type MethodCallback = raw fn(&Value) -> Value;
//!
//!     impl Value {
//!         // Synthesize its C companion method:
//!         //    extern "C" fn doubled(_self: *const CValue) -> CValue;
//!         raw fn doubled(&self) -> Value;
//!     }
//!
//!     // Synthesize its C companion function:
//!     //    extern "C" fn increment(value: CBox<u8>) -> CValue;
//!     raw fn increment(value: move Box<u8>) -> Value;
//!
//!     #[symbol_name = "doc_register_callback"]
//!     fn register_callback(callback: Callback);
//!     #[symbol_name = "doc_register_method_callback"]
//!     fn register_method_callback(callback: MethodCallback);
//! }
//!
//! register_callback(increment_raw);
//! register_method_callback(Value::doubled_raw);
//! ```
//!
//! # Opaque type variance
//!
//! Lifetime and type parameters of declared opaque types are invariant by default. A lifetime
//! parameter can explicitly be declared covariant with `#[unsafe(covariant(...))]`:
//!
//! ```rust
//! use co3::ffi;
//!
//! ffi! {
//!    #![unsafe(extern("C"))]
//!
//!    #[unsafe(covariant('parent))]
//!    type Child<'parent, T>;
//!
//!    impl<'parent, T> Drop for Child<'parent, T> {
//!        fn drop(&mut self);
//!    }
//! }
//! ```
//!
//! The attribute is unsafe because it asserts that the provider's real type is covariant over every
//! listed lifetime. Type parameters cannot be listed and remain invariant.
//!
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;
extern crate self as co3;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};

#[cfg(feature = "derive")]
pub use co3_derive::*;
use disjoint_impls::disjoint_impls;
pub use impls::impls;
#[doc(hidden)]
pub use rust_spec;
use rust_spec::{
    One, RustSpec,
    mutability::{Exclusive, Interior},
    niche::{NicheStabilityKind, WithNiche, WithoutNiche},
    size::{ExternTypeLike, MetaSized, SizedKind, SliceLike, Zero},
};
#[cfg(feature = "alloc")]
use rust_spec::{Stable, Unstable, size::MetadataKind};

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    option::ReprCOption,
    result::ReprCResult,
    slice::{CSlice, CSliceMut},
    stored::{DecodeOwned, EmptyStore, EncodeOwned, Store},
    wide::Wide,
};

pub mod borrow;
#[cfg(feature = "alloc")]
pub mod boxed;
pub mod cell;
#[doc(hidden)]
pub mod either;
mod ffi;
pub mod niche;
pub mod option;
mod primitives;
pub mod result;
pub mod slice;
mod std_impls;
pub mod stored;
pub mod tag;
pub mod transmute;
pub mod tuple;
pub mod wide;

trait Thin {}
impl<K: SizedKind> Thin for rust_spec::size::Sized<K> {}
impl Thin for ExternTypeLike {}

#[cfg(feature = "alloc")]
trait Dst {}
#[cfg(feature = "alloc")]
impl Dst for ExternTypeLike {}
#[cfg(feature = "alloc")]
impl<K: MetadataKind> Dst for MetaSized<K> {}

/// Produces return values for invariant violations in the conversion layer.
///
/// With `#![failure = "error"]`, an [`ffi!`] declaration returns an error value instead of panicking
/// when the conversion layer encounters an invariant violation. This failure mode expects that the
/// return type of the declared function implements this trait.
pub trait Error {
    /// Produces a return value for an invalid input or output value representation.
    fn trap_value() -> Self;

    /// Produces a return value when no tag-dispatched candidate matches a runtime tag.
    fn unknown_tag() -> Self;

    /// Produces a return value when changes through a `#[soft]` reference cannot be synchronized.
    fn soft_sync_error() -> Self;
}

/// Robust type that conforms to C ABI and can be safely shared across FFI boundaries.
///
/// Note that, for raw pointers, ABI compatibility of referent is not guaranteed. Dereferencing
/// opaque/extern type pointers which don't also implement `ReprC` is very likely to cause UB.
///
/// # Safety
///
/// Type implementing the trait must have a guaranteed C ABI and no trap representations.
pub unsafe trait ReprC {}

/// ABI marker types used by [`CFnArg`] and [`CFnReturn`].
pub mod abi {
    macro_rules! markers {
        ($($name:ident),* $(,)?) => { $(
            #[doc = concat!("The `", stringify!($name), "` calling convention.")]
            pub enum $name {}
        )* };
    }

    markers!(
        Rust,
        C,
        CUnwind,
        System,
        SystemUnwind,
        Cdecl,
        CdeclUnwind,
        Stdcall,
        StdcallUnwind,
        Fastcall,
        FastcallUnwind,
        Thiscall,
        ThiscallUnwind,
        Sysv64,
        Sysv64Unwind,
        Win64,
        Win64Unwind,
        Aapcs,
        AapcsUnwind,
        Efiapi,
    );
}

/// `ReprC` type that is allowed as a C function argument for `Abi`.
///
/// # Safety
///
/// Type must be allowed as a C function argument type.
pub unsafe trait CFnArg<Abi>: ReprC + Copy {}

/// `ReprC` type that is allowed as a C function return value for `Abi`.
///
/// # Safety
///
/// Type must be allowed as a C function return type.
pub unsafe trait CFnReturn<Abi>: ReprC + Copy {}

disjoint_impls! {
    /// A Rust type that has an `extern "C"` ABI
    pub trait ExternC {
        /// The C-compatible representation of this Rust type.
        type CType: ReprC + ?Sized;
    }

    impl<R: ExternC + ?Sized> ExternC for &R
    where
        R: RustSpec<Size: Thin, Mutability = Exclusive>,
    {
        type CType = *const R::CType;
    }
    impl<R: ExternC + ?Sized> ExternC for &R
    where
        R: RustSpec<Size: Thin, Mutability = Interior>,
    {
        type CType = *mut R::CType;
    }
    impl<R: Wide<Data: ExternC, Metadata = usize> + ?Sized> ExternC for &R
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSlice<<R::Data as ExternC>::CType>;
    }
    impl<R: Wide<Data: ExternC, Metadata = usize> + ?Sized> ExternC for &R
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = Interior>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSliceMut<<R::Data as ExternC>::CType>;
    }

    impl<R: ExternC + ?Sized> ExternC for &mut R
    where
        R: RustSpec<Size: Thin>,
    {
        type CType = *mut R::CType;
    }
    impl<R: Wide<Data: ExternC, Metadata = usize> + ?Sized> ExternC for &mut R
    where
        R: RustSpec<Size = MetaSized<SliceLike>>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CSliceMut<<R::Data as ExternC>::CType>;
    }

    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: SizedKind> ExternC for Box<R>
    where
        R: RustSpec<Size = rust_spec::size::Sized<S>>,
        <R as ExternC>::CType: Sized,
    {
        type CType = CBox<R::CType>;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized> ExternC for Box<R>
    where
        R: RustSpec<Size = MetaSized<SliceLike>>,
        R: Wide<Data: ExternC, Metadata = usize>,
        <<R as Wide>::Data as ExternC>::CType: Sized,
    {
        type CType = CBoxedSlice<<R::Data as ExternC>::CType>;
    }

    impl<R: ExternC> ExternC for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        <R as ExternC>::CType: Sized,
    {
        type CType = ReprCOption<R::CType>;
    }
    impl<R: ExternC, N: NicheStabilityKind> ExternC for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type CType = R::CType;
    }

    // TODO: The next 3 impls are the same
    impl<R: ExternC, E: ExternC, K: SizedKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<K>>,
        E: RustSpec<Size = rust_spec::size::Sized<K>>,
        // TODO: There is an error in disjoint_impls! that doesn't allow to use
        // `Extern<CType: Copy>` constraint. Fix that! Check other Result sites
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = ReprCResult<R::CType, E::CType>;
    }
    impl<R: ExternC, E: ExternC, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<
            Size = rust_spec::size::Sized<Zero>,
            Alignment = rust_spec::Gt<rust_spec::One>,
        >,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = ReprCResult<R::CType, E::CType>;
    }
    impl<R: ExternC, E: ExternC, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<
            Size = rust_spec::size::Sized<Zero>,
            Alignment = rust_spec::Gt<rust_spec::One>,
        >,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
        <R as ExternC>::CType: Copy,
        <E as ExternC>::CType: Copy,
    {
        type CType = ReprCResult<R::CType, E::CType>;
    }
    impl<R: ExternC, E, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = One>,
    {
        type CType = R::CType;
    }
    impl<R, E: ExternC, N: NicheStabilityKind> ExternC for Result<R, E>
    where
        R: RustSpec<Size = rust_spec::size::Sized<Zero>, Alignment = One>,
        E: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>, Niche = WithNiche<N>>,
    {
        type CType = E::CType;
    }
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    pub trait Encode: EncodeOwned {}

    #[cfg(feature = "alloc")]
    impl<R: ?Sized> Encode for Box<R>
    where
        Self: RustSpec<Layout = Stable> + EncodeOwned,
    {}
    #[cfg(feature = "alloc")]
    impl<R: RustSpec<Layout = Stable> + ?Sized> Encode for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + EncodeOwned,
    {}
}

disjoint_impls! {
    /// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
    pub trait Decode<'d>: DecodeOwned<'d> {}

    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized> Decode<'d> for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        Self: DecodeOwned<'d>,
    {}
    #[cfg(feature = "alloc")]
    impl<'d, R: ?Sized> Decode<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable>,
        Self: DecodeOwned<'d>,
    {}
}

impl<R: ?Sized> Encode for &R where Self: EncodeOwned {}
impl<R: ?Sized> Encode for &mut R where Self: EncodeOwned {}

impl<'d, R: ?Sized> Decode<'d> for &'d R where Self: DecodeOwned<'d> {}
impl<'d, R: ?Sized> Decode<'d> for &'d mut R where Self: DecodeOwned<'d> {}

impl<R: Encode, const N: usize> Encode for [R; N] where Self: EncodeOwned {}
impl<'d, R: Decode<'d>, const N: usize> Decode<'d> for [R; N] where Self: DecodeOwned<'d> {}

impl<R: Encode> Encode for Option<R> where Self: EncodeOwned {}
impl<'d, R: Decode<'d>> Decode<'d> for Option<R> where Self: DecodeOwned<'d> {}

impl<R: Encode, E: Encode> Encode for Result<R, E> where Self: EncodeOwned {}
impl<'d, R: Decode<'d>, E: Decode<'d>> Decode<'d> for Result<R, E> where Self: DecodeOwned<'d> {}

/// Perform the conversion from `T` into [`ExternC::CType`] using external storage.
///
/// Prefer using [`encode`] whenever possible
pub fn soft_encode<T: Encode>(item: T, store: &mut T::Store) -> T::CType {
    item.soft_encode(store)
}

/// Perform the conversion from `T` into [`ExternC::CType`].
pub fn encode<T: Encode<Store: EmptyStore>>(item: T) -> T::CType {
    stored::encode_owned(item)
}

/// Perform the conversion from [`T::CType`](ExternC::CType) into `T` using external storage.
///
/// Prefer using [`decode`] whenever possible
///
/// # Safety
///
/// - All conversions from a pointer must ensure pointer validity beforehand
pub unsafe fn soft_decode<'d, T: Decode<'d>>(
    source: T::CType,
    store: &'d mut T::Store,
) -> Option<T> {
    unsafe { T::soft_decode(source, store) }
}

/// Perform the conversion from [`T::CType`](ExternC::CType) into `T`.
///
/// # Safety
///
/// - All conversions from a pointer must ensure pointer validity beforehand
pub unsafe fn decode<'d, T: Decode<'d, Store: EmptyStore> + 'd>(source: T::CType) -> Option<T> {
    unsafe { stored::decode_owned(source) }
}

#[cfg(feature = "alloc")]
impl<R: ExternC<CType: Sized>> ExternC for Vec<R> {
    type CType = CBoxedSlice<R::CType>;
}

#[cfg(feature = "alloc")]
impl<R> Encode for Vec<R>
where
    Self: EncodeOwned,
    Box<[R]>: Encode,
{
}
#[cfg(feature = "alloc")]
impl<'d, R> Decode<'d> for Vec<R>
where
    Self: DecodeOwned<'d>,
    Box<[R]>: Decode<'d>,
{
}

// TODO: Check https://github.com/mversic/co3/issues/13
const fn assert_arr_has_non_zero_len<const N: usize>() {
    assert!(N != 0, "empty array is a ZST");
}

#[cfg(test)]
#[cfg(feature = "alloc")]
mod tests {
    use super::*;

    #[test]
    fn encode_stored_mut_ref() {
        let inner = 8u8;
        let other = 42u8;
        let mut value = Some(inner);
        let value_mut_ref: &mut Option<u8> = &mut value;
        {
            let mut store = Box::default();
            let encoded = soft_encode(value_mut_ref, &mut *store);
            unsafe {
                *encoded = ReprCOption::Some(other);
            }
            store.sync().unwrap();
        }
        assert_eq!(value, Some(42u8));

        let mut slice = [Some(1u8)];
        let ref_mut: &mut [_] = &mut slice;
        {
            let mut store = Box::default();
            let encoded = soft_encode(ref_mut, &mut *store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };
            c_slice[0] = ReprCOption::Some(other);
            store.sync().unwrap();
        }
        assert_eq!(slice, [Some(42u8)]);
    }

    #[test]
    fn decode_stored_mut_ref() {
        let mut c_opt = ReprCOption::Some(1u8);
        let c_ptr: *mut _ = &mut c_opt;
        let new_val: u8 = 42;
        {
            let mut store = Box::default();
            let decoded = unsafe { soft_decode::<&mut _>(c_ptr, &mut *store) }.unwrap();

            *decoded = Some(new_val);
            store.sync().unwrap();
        }
        assert_eq!(c_opt, ReprCOption::Some(42u8));

        let mut c_opts = [ReprCOption::Some(1u8)];
        let c_slice = CSliceMut::from_slice(&mut c_opts);
        let x: u8 = 10;
        {
            let mut store = Box::default();
            let decoded = unsafe { soft_decode::<&mut [_]>(c_slice, &mut *store) }.unwrap();

            decoded[0] = Some(x);
            store.sync().unwrap();
        }
        assert_eq!(c_opts[0], ReprCOption::Some(10u8));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn encode_stored_ref_mut_slice() {
        use tuple::ReprCTuple2;

        let mut tuples = [(1_u32, true)];
        let slice_ref: &mut [_] = &mut tuples;

        {
            let mut store = Box::default();

            let encoded = soft_encode(slice_ref, &mut store);
            let c_slice = unsafe { encoded.into_rust().unwrap() };

            c_slice[0] = ReprCTuple2(100, 1);
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }

    #[test]
    #[cfg(feature = "alloc")]
    // FIXME: This test demonstrates that ownership of &mut Box<(u32,)> is leaked
    // from one side to the other: https://github.com/mversic/co3/issues/182
    fn encode_stored_mut_box_allows_pointer_replacement() {
        use boxed::CBox;
        use tuple::ReprCTuple2;

        let mut value = Box::new((1_u32, true));

        {
            let mut store = Box::default();
            let encoded = soft_encode(&mut value, &mut *store);
            let original_data = unsafe { (*encoded).data };
            let replacement = CBox::from_box(Box::new(ReprCTuple2(100, 1)));
            let replacement_data = replacement.data;

            unsafe {
                *encoded = replacement;
            }

            assert_ne!(original_data, replacement_data);
            store.sync().unwrap();
        }

        assert_eq!(*value, (100, true));
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn decode_stored_ref_mut_slice() {
        use tuple::ReprCTuple2;

        let mut tuples = [ReprCTuple2(10, 1)];
        let c_slice = CSliceMut::from_slice(&mut tuples);

        {
            let mut store = Box::default();
            let decoded =
                unsafe { soft_decode::<&mut [(u32, bool)]>(c_slice, &mut store) }.unwrap();

            decoded[0].0 = 100;
            store.sync().unwrap();
        }

        assert_eq!(tuples[0].0, 100);
    }
}
