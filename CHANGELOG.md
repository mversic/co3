# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

### Fixed

- Lower one-part `#[unpack(A)]` imports using `<A as ReprC>::CType` as the foreign parameter type.
- Emit `CFnArg` and `CFnReturn` assertions for each statically selected ordinary import declaration.
- Require `move` when passing `CBox` or `CBoxedSlice` through imported function arguments.
- Check exported associated types and consts definitions against the existing Rust impl.

### Changed

- Rename the ABI traits from `ExternC` to `ReprC` and `ReprC` to `CType`.
- Reject explicit `<dyn T>::TAG` arguments in `ffi!` export declarations.
- Disallow defining statics in export blocks; they can still be declared.
- Rename the derive helper attribute from `reprC` to `repr_c`.

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
