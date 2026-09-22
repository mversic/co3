# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
