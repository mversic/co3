# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial `co3` and `co3-derive` crates for exporting and importing C ABI functions with `ffi!` and deriving C-compatible representations with `ReprC`.
- Opt-in `move` and `#[soft]` conversion modes, plus `alloc`, `derive`, and `allocator-api` features.
- Tagged dispatch for sharing one C ABI function across concrete Rust types.
