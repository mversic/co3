# Specification

## 1. Design Goal

`co3` is a Rust-side framework for exporting/importing C ABI functions by:

- Mapping Rust types to FFI-safe types (`ExternC::CType`).
- Converting values to/from those FFI types (`Encode`/`Decode`).
- Generating extern functions (`carbonate`/`decarbonate` and derive macros).

### 1.1. Hard Guarantees

These guarantees define the contract of this library:

1. **Native-Rust ergonomics**
- Ergonomics of APIs and generated wrappers MUST remain idiomatic to Rust users.
- FFI boundary mechanics SHOULD stay encapsulated in conversion traits and generated glue.
2. **Soundness-first FFI interoperability**
- Soundness MUST NOT be weakened for performance in the default configuration.
- If preserving soundness requires additional validation, temporary storage, or cloning, that cost is accepted.
3. **Zero-cost abstraction by default**
- Zero-cost abstraction MUST be preserved unless it directly conflicts with the soundness guarantee.
- Only explicit opt-in modes MAY prioritize performance by shifting soundness responsibility to the user.

### 1.2. FFI Conversion Modes

Conversion modes define how values cross the FFI boundary, including ownership behavior, pointer-identity semantics, and validation strictness. Each mode makes explicit tradeoffs and is selected through compile-time configuration:

1. **`owned-as-ref` (default, opt-out)**
- Represents `Drop` types as borrowed instead of transferring ownership when crossing the FFI boundary.
- Keeps owned backing storage alive in a type-specific store while exposing borrowed FFI views.
- Prevents transferring ownership at the expense of performance due to cloning in decode path.

2. **`cloned-refs` (opt-in)**
- Enables additional reference conversion paths that rely on cloning the referent (e.g. `&(u8,)`).
- Uses intermediate owned/cloned values and store synchronization for mutable writeback paths.
- Pointer identity is not preserved and pointer equality for these types MUST NOT be relied on.

3. **`unsafe-optimizations` (opt-in)**
- Eliminates cloning in encode path of mutable references to transmutable `Drop` types (e.g. `&mut Box<u32>`).
- Eliminates cloning in encode path of transmutable mutable references to non-robust types (e.g. `&mut bool`).
- Responsibility to maintain soundness by avoiding trap values or ownership transfer is shifted to the calee.

## 2. Core Model

### 3.1 Repr Families

Every supported Rust type is classified via `ir::ReprFamily::Kind` into one of:

1. `Robust`: ABI-stable, trap-free representation (`ReprC`).
2. `Transmuted`: conversion delegated through `CheckedTransmute::Target`.
3. `Opaque`: carried as opaque pointers.
4. `Cloned` families: fallback conversion through clone-based paths.

### 3.2 External Type Contract

For any `T: ExternC`, `T::CType` MUST implement `ReprC`.

High-level mapping:

- `Kind = Robust` -> `CType = T`.
- `Kind = Transmuted` -> `CType = <Target as ExternC>::CType`.
- `Kind = Opaque` -> `CType = *mut T`.
- Container/reference mappings recurse according to `ReprFamily` impls.

## 3. Safety-Critical Traits

### 4.1 `ReprC`

`ReprC` marks types as C ABI compatible and robust. Implementors MUST satisfy:

- Stable C-compatible memory layout.
- No trap representations for value-level transport.
- `Copy` semantics (required by trait bound).

### 4.2 `CheckedTransmute`

`CheckedTransmute` is unsafe and MUST only be implemented when:

- `Self` and `Target` are mutually transmutable (including drop semantics).
- `is_valid` never returns true for trap representations (no false positives).

### 4.3 `Niche` / `StableNiche`

`Niche` defines a sentinel value used for `Option` compression in selected families.
`StableNiche` requires compiler-guaranteed niche semantics.

## 4. Canonical Representation Rules

The following rules define externally visible representation behavior.

### 5.1 References

- `&T` where `T` is robust/transmuted/opaque -> transmuted pointer-like `CType`.
- With `cloned-refs`, references to cloned families are passed as raw pointers to cloned `CType`.

### 5.2 Slices

- `&[R]` robust -> `CSlice<R>`.
- `&mut [R]` robust -> `CSliceMut<R>`.
- Transmuted slices recurse through target slice types.
- Opaque/cloned slice forms may use element pointer slices.

### 5.3 Owned Collections

When `owned-types` is enabled:

- `Box<[R]>`/`Vec<R>` robust:
  - with `owned-as-ref`: represented as `CSliceMut<R>` and borrowed through store.
  - without `owned-as-ref`: represented as owning `CBoxedSlice<R>`/`CVec<R>`.
- Opaque/cloned element variants use pointer or recursively encoded element storage.

### 5.4 Arrays

- `[R; N]` robust -> `[R::CType; N]` where applicable.
- Arrays with opaque family become arrays of pointers.
- Zero-length arrays are rejected in paths requiring non-zero length (`assert_arr_has_non_zero_len`).

### 5.5 `Option<T>`

Two canonical forms:

1. `Option<WithoutNiche>` -> `COption<T::CType>` tagged repr.
2. `Option<WithCustomNiche>` -> encoded directly as `T::CType`, `None` represented by `T::NICHE_VALUE`.

For transmuted + stable niche cases, compression may delegate to transmute chain.

### 5.6 `Result<T, E>`

`Result<T,E>` uses `CResult<T::CType, E::CType>` tagged union (`tag` values `0`/`1`, niche tag `2`).

### 5.7 Tuples

Tuples are mapped to `repr(C)` wrappers `CTuple1..CTuple12`.
Tuple option-niche behavior follows first-available niche-composition rules in `src/tuple.rs`.

## 5. Conversion Semantics

### 6.1 Encode

`Encode::encode(self, store)` MUST:

- Produce a valid `CType` for the active feature set.
- Preserve enough context in `Store` for post-call synchronization where required.

### 6.2 Decode

`Decode::decode(source, store)` is unsafe and MUST:

- Treat invalid input representations as failure (`None`).
- Decode pointer-backed values only when pointer validity assumptions hold.

### 6.3 Store Synchronization

`Store::sync(self)` applies post-call writeback effects.
Wrapper-generated call paths MUST run `sync()` for argument stores after FFI calls.

## 6. Out-Pointer Contract

`out_ptr` defines:

- `OutPtr`: ABI type used for method/function outputs.
- `OutPtrWrite`: writing Rust values to FFI out pointer.
- `OutPtrRead`: reconstructing Rust values from out representation.

Generated wrappers rely on `OutPtrWrite` on export side and `OutPtrRead` on import side.

## 7. FFI Return/Error Model

`FfiReturn` values:

- `Ok = 0`
- `UnknownHandle = -1`
- `TrapRepresentation = -2`
- `UnrecoverableError = -3`
- `ExecutionFail = -4`

Conversion decode failures MUST map to trap representation at wrapper boundaries.
Exported wrappers catch panics and return `UnrecoverableError`.

## 8. Macro and Derive Behavior

### 9.1 `#[derive(ExternC)]`

Derive generates family/niche/transmute impls based on input type shape and attributes under `#[mineral(...)]`.
Only public types are accepted for FFI derive/export paths.

### 9.2 `#[carbonate]`

For Rust function/impl methods, generates `unsafe extern "C"` wrappers that:

1. Decode inputs from FFI.
2. Call original Rust method/function.
3. Encode/write output via out-pointer.
4. Sync stores.
5. Return `FfiReturn`.

### 9.3 `#[decarbonate]`

Generates Rust wrappers around declared extern symbols that:

1. Encode Rust inputs.
2. Call FFI symbol.
3. Check `FfiReturn` status.
4. Sync stores.
5. Decode/read output.

### 9.4 `#[extern_type]`

For opaque-marked exported types, replaces body with transparent handle wrapper around `NonNull<external::Extern>`, plus generated trait wrappers for shared methods (clone/eq/ord/default/drop interop).

## 9. Configuration Mapping

This section maps operational modes and optional capabilities to current compile-time configuration controls.

| Config Control | Default | Maps To | Notes |
| --- | --- | --- | --- |
| `cloned-refs` | Disabled | `1.2` `cloned-refs` mode | Compatibility-focused mode for additional reference families. |
| `owned-as-ref` | Enabled | `1.2` `owned-as-ref` mode | Implies `owned-types`; ownership transfer of robust owned containers is avoided by default. |
| `unsafe-optimizations` | Disabled | `1.2` `unsafe-optimizations` mode | Only control in this set intended to relax default soundness guardrails. |
| `owned-types` | Enabled indirectly (via default `owned-as-ref`) | Capability toggle | Enables owned container conversion families used by modes. |
| `derive` | Disabled | Optional codegen capability | Enables proc-macro API from `co3_derive`. |
| `getset` | Disabled | Optional codegen capability | Enables `getset` integration in derive wrappers; implies `derive`. |

Rules:

- Mode/capability combinations MUST remain coherent with guarantees in section `1.1`.
- Operational semantics are defined in section `1.2`; this section only defines current control mapping.

## 10. Known Limits (Current Behavior)

Current implementation intentionally includes unfinished areas and constraints:

- Some TODO/FIXME-marked paths are unimplemented.
- Derive/getset path cannot resolve aliased derive macro names.
- Certain empty-array and advanced generic edge cases remain constrained.
- Error handling for `ExecutionFail` in generated wrappers is not fully finalized (contains `unimplemented!` paths).

These are implementation constraints, not guaranteed future behavior.

## 11. Conformance and Tests

Primary behavior coverage is currently in:

- `tests/carbonate/*` (export wrapper behavior).
- `tests/unambiguous.rs` (name generation/trait disambiguation).
- `tests/niche_value.rs` (niche/value mapping).
- `derive/tests/ui*.rs` and `derive/tests/ui_fail/*.stderr` (derive diagnostics/contracts).
- In-module tests in `src/lib.rs` and `src/niche.rs`.

Any change to representation or conversion behavior SHOULD update this spec and corresponding tests in the same change.
