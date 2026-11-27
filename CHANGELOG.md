# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.1] - 2026-11-27

### Fixed

- properly handle lifetimes on raw functions

## [0.9.0] - 2026-11-25

### Added

- Gate unspecified-layout conversions behind the default-enabled `repr-rust` feature.
- Add `#![lang = "..."]` to `ffi!` for language-specific conversion contracts.
- Add an explicit language parameter to checked conversion functions.
- Accept unit-like zero-sized types as C-compatible function returns.
- Add stable counterparts for vectors, strings, and wide pointers.
- Support `#[unpack(_, _)]` for wide `NonNull` pointers.
- Support recursive types in `#[derive(ReprC)]`.

### Changed

- Make `encode` and `soft_encode` unsafe.
- Allow `#[soft]` on `move` arguments and receivers.
- Wrap native Rust type carriers in `marker::Native`.
- Lower `*const T` and `*mut T` to pointers to `T::CType`.
- Use independent checked and unchecked conversion traits and stores.
- Move built-in carriers into each type's `abi` module and parameterize them by lowered field types.
- Generate `NameCType<T>` for `Name<T>` and expose the companion through `ReprC::CType`.
- Represent vectors with pointer, capacity, and length, and UTF-8 strings with byte vectors.
- Rename `ReprCTupleN` to `tuple::TupleN` and lower native tuples to `Native<tuple::abi::TupleN<...>>`.
- Add lifetimes to direct reference carriers and use shared carriers for interior-mutable pointees.
- Lower `PhantomData<T>` to `PhantomData<T::CType>`, which now requires `T: ReprC`.
- Replace `ReprCOption`/`ReprCResult` with the stable Rust enums `option::Option`/`result::Result`
- Separate tagged `Option` and `Result` carriers from transparent niche payload wrappers.
- Represent nonzero integers and `NonNull<T>` with `marker::NonZero` around the underlying type.
- Mark references such as `&mut Box<T>` and `&UnsafeCell<Box<T>>` as `Borrowed`.
- Require `move` for by-value transfer of derived types with custom `Drop`.
- Make `call` and `soft_call` honor borrowed and moved ABI carriers.
- Replace `CheckedTransmute` with `marker::AbiCompatible` and `convert::AbiValidate`.
- Expose value and reference codecs in `convert` and move packing traits from `slice` to `pack`.
- Require `Niche::is_niche` and matching `RustSpec` classifications for custom niches and validators.
- Make null reference and zero-valued `marker::NonZero` carriers compare unequal to themselves.
- Limit `cfg_attr` expansion by `ffi!` and `raw!` to leading inner attributes.

### Removed

- Replace `stored::{DirectEncodeOwned, EncodeOwned, DecodeOwned}` with checked and unchecked value codecs.
- Replace `Borrow`, `BorrowCast`, `BorrowCastMut`, and `FromBorrow` with explicit borrowed conversion codecs.
- Remove the `#[repr_c(transparent)]` derive option for delegating a wrapper's representation to its inner field.
- Require `Copy` for imported static reads and shared-access validation for read-only accessors.
- Replace `utf8::Utf8Str` with `primitive::abi::str`.

### Fixed

- Validate shared records with robust interior-mutable fields without reading them.
- Allow qualified primitive aliases such as `shadow::i32` in `raw!` signatures.

## [0.8.2] - 2026-10-21

### Fixed

- don't duplicate concretized bounds on `DispatchSet` impls

## [0.8.1] - 2026-10-21

### Added

- Derive `Wide` and `NulTerminatedBuf` for `#[repr(transparent)]` structs with multiple fields

### Changed

- Emit special-type identity checks once per derive or FFI expansion.
- Rename `NulTerminatedBuf::Data` to `NulTerminatedBuf::Header`.
- lower `ManuallyDrop<T>` as `T::CType` when `T` is `NoDrop`.
- Require `Wide` implementors to be dynamically sized types.

### Removed

- remove `UnsafeCell` impls which relied on unsychronized access.
- remove `#[repr_c(as(T))]` feature which relied on false `RustSpec`.

## [0.8.0] - 2026-10-17

### Added

- Parameterize wide pointer carriers by unsized `Wide` pointee types.
- Support `UnsafeCell<T>` and `Cell<T>` in C-compatible representations.
- Implement `CType` for `ManuallyDrop<T>` and represent it as `ManuallyDrop<T::CType>`.

### Changed

- Rename `CSlice`/`CSliceMut`/`CBoxedSlice` to `CWideRef`/`CWideRefMut`/`CWideBox`/`CWideBoxCell`.
- Rename the mutable carrier const param from `RESTRICTED` to `EXCLUSIVE` and remove its default.
- Require opaque types to have the opaque `RustSpec` with `#[rust_spec(opaque)]`.
- Set the MSRV to 1.99 because of the `UnsafeCell` transmutation guarantees.
- Remove the `Copy` supertrait requirement from `CFnArg` and `CFnReturn`.

### Fixed

- Corrected (de)serialization of `ManuallyDrop` and `MaybeUninit`

### Removed

- `cell::InteriorMut` was removed because using `UnsafeCell::get` is no longer required

## [0.7.4] - 2026-10-13

### Added

- delegate notable trait impls for `OwnedExternType` to matching impls on `ExternType`.

### Fixed

- Resolve associated item paths FFI function signatures.
- don't allow `#[derive]` on extern type declarations.

## [0.7.3] - 2026-10-11

### Added

- Add implied `Sized` bound to params in slice-like DST tails.

### Changed

- implement `RustSpec` for synthesized views manually.

## [0.7.2] - 2026-10-11

### Added

- implement `CType` for robust `Option`s, `Result`s and derived types with stable layout.
- Add the transparent `CRef<T>` carrier for shared references to thin values.

### Fixed

- Reject niches, validation, and custom `Drop` in repr delegation

## [0.7.1] - 2026-10-10

### Added

- Add conversions and derives for `co3::ffi` types and make their use portable

## [0.7.0] - 2026-10-10

### Added

- Add `decode_unchecked` to decode C-compatible values without validating them.
- Handle types with custom `Drop` implementations via `RustSpec::Drop` axis.
- Support encoding `CString` and custom nul-terminated aggregate types.
- Support `#[repr_c(transparent)]` which uses inner type's `CType`
- Generate C-compatible wrappers for `bool` and `Ordering`.

### Changed

- Model aliasing explicitly in C ABI carriers `CRefMut` and `CSliceMut`.
- Rename the niche derive setting from `NICHE_VALUE` to `NICHE`.
- Separate header and data handling for `Wide` types.

### Removed

- Restrict `Wide` implementations to custom slice-like DSTs to avoid undefined behavior.

## [0.6.0] - 2026-10-04

### Added

- Support `#[unpack]` on method receivers and raw pointers to wide types.
- Implement `Unpack2` for raw and `NonNull` pointers to wide types.
- Derive `Wide` for eligible `#[repr_c(identity)]` dynamically sized structs.

### Changed

- Change `Wide` API to take raw pointers instead of references.
- Move conversion store structs, including `ArrayStore`, from `stored` to `sync`.
- Use `CBoxCell` types as the C-compatible representations of interior mutable boxes.

### Fixed

- Correct conversion and borrowing of interior mutable slices and boxes.

## [0.5.3] - 2026-10-02

### Added

- Support `#[repr_c(as(T))]` to delegate owned conversions through `Into<T>` and `TryFrom<T>`.
- Generate an `OwnedType = Box<Type>` alias for opaque types in `ffi!` export declarations.

## [0.5.2] - 2026-10-01

### Fixed

- Represent `&CStr` as a thin pointer at the ABI boundary

## [0.5.1] - 2026-09-30

### Added

- Support `#[unpack]` inside `raw!` and on fn pointers

### Changed

- Hide export of `impls::impls!` from the documentation

## [0.5.0] - 2026-09-30

### Added

- Add `raw!` to generate visible C-ABI `{name}_raw` companions for existing functions

### Changed

- Rename the ABI traits from `ExternC` to `ReprC` and `ReprC` to `CType`.
- Reject explicit `<dyn T>::TAG` arguments in `ffi!` export declarations.
- Disallow defining statics in export blocks; they can still be declared.
- Rename the derive helper attribute from `reprC` to `repr_c`.

### Fixed

- Lower one-part `#[unpack(A)]` imports using `<A as ReprC>::CType` as the foreign parameter type.
- Emit `CFnArg` and `CFnReturn` assertions for each statically selected ordinary import declaration.
- Require `move` when passing `CBox` or `CBoxedSlice` through imported function arguments.
- Check exported associated types and consts definitions against the existing Rust impl.

## [0.4.1] - 2026-09-22

### Added

- enable constructing c slices from raw parts

## [0.4.0] - 2026-09-21

### Added

- support `ReprC` function pointers

## [0.3.1] - 2026-09-20

### Added

- implement `ReprC` for `CStr` and `CString`

## [0.3.0] - 2026-09-18

### Fixed

- use identity borrow casts for fieldless enum ctype
- compactly pack large field tuples in niche derive

## [0.2.0] - 2026-09-17

### Fixed

- Forward EmptyStore through EitherN

## [0.1.0] - 2026-09-16

### Added

- Initial `co3` and `co3-derive` crates for exporting and importing C ABI functions with `ffi!` and deriving C-compatible representations with `ReprC`.
- Opt-in `move` and `#[soft]` conversion modes, plus `alloc`, `derive`, and `allocator-api` features.
- Tagged dispatch for sharing one C ABI function across concrete Rust types.
