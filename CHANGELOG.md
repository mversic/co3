# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
