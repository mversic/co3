- Always consider consistency with SPEC.md and ARCHITECTURE.md before any change
- Using `disjoint_impls(remote)` is only allowed in the derive crate

- speculative bounds can be emitted only for concrete types and usually only for the last field.
- pointer reads must require `Copy`, preventing duplication of mutable references or ownership.
- `CType` includes opaque types, which may contain mutable references or have Rust drop behavior.
- `CType` alone does not justify copying a value or inspecting shared interior-mutable contents.
- Derived companions use the `CType` suffix and retain their original Rust type parameters.
- Inspecting contents of interior-mutable through a shared reference is not allowed.

# Integration tests

- Integration tests showcase the public API as user would write it.
- use conversion functions like `co3::encode`/`co3::decode`.
- use `raw fn` or `_` instead of lowered types in fn args.
- keep direct use of encode/decode traits in unit tests.
- refrain from mentioning lowered abi types directly.

# Imports

- Organize imports into three groups, separated by a blank line: standard library (`std`, `core`, `alloc`), external crates, then internal paths (`crate`, `super`, `self`).
- Combine imports from the same root into one `use` declaration; never add parallel imports from that root. Preserve conditional-compilation requirements when grouping imports.
- Import `Language` and the `lang` module; write `L: Language` and `lang::C` (or another language marker), rather than importing individual language markers.
- Within the library, use `crate::` for internal paths. Doctests and integration tests use `co3::` as external callers.
