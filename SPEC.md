# Specification

## 1. Design Goal

`co3` is a Rust-side framework for exporting/importing C ABI functions by:

- Mapping Rust types to C-compatible types via `ReprC` derive macro.
- Exporting functions and impl block methods via `export("ABI")` attribute.
- Declaring extern functions and impl block methods via `extern_!`/`extern_C!` macros.

### 1.1 Hard Guarantees

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

### 1.2 FFI Conversion Modes

Conversion modes define how values cross the FFI boundary, including ownership behavior, pointer-identity semantics, and validation strictness.
Each mode makes explicit tradeoffs and is selected through compile-time configuration:

1. **`owned-as-ref` (default, opt-out)**
- Represents `Drop` types (except `Opaque` kind) as borrowed instead of transferring ownership.
- Keeps owned backing storage alive in a type-specific store while exposing borrowed FFI views.
- Prevents ownership transfer at the expense of performance due to cloning in decode paths.

2. **`unstable-refs` (opt-in)**
- Enables additional reference conversion paths that rely on cloning the referent (e.g. `&(u8,)`).
- Uses intermediate owned/cloned values and store synchronization for mutable writeback paths.
- Pointer identity is not preserved and pointer equality for these types MUST NOT be relied on.

3. **`unsafe-optimizations` (opt-in)**
- Eliminates cloning in encode paths of mutable references to transmutable `Drop` types (e.g. `&mut Box<u32>`).
- Eliminates cloning in encode paths of transmutable mutable references to non-robust types (e.g. `&mut bool`).
- Responsibility to maintain soundness by avoiding trap values or ownership transfer is shifted to the callee.

## 2. Public API

Public API constitutes user-facing macro entry points.
Any conversion written manually against internal conversion traits of this crate **DOES NOT** constitute public API.

### 2.1 `ReprC` Derive Macro

`#[derive(ReprC)]` derives implementations required to convert a type to a corresponding C-compatible companion type.
A C-compatible companion type is a type with a defined C ABI and no trap representations, whose fields are themselves C-compatible companion types.
The only exception is raw pointers: their referents are not required to have a C-compatible companion type (unline references, which require a valid referent).

- By default, the derive defines a C-compatible companion type and conversions between the two types.
- For `#[repr(C)]` types, representation requirements are checked recursively at compile time for all field types.
- If a `#[repr(C)]` type is valid, conversion to/from its companion type is optimized to a no-op transmute.
- For `#[repr(transparent)]`, representation is delegated to the wrapped type (no companion type is defined).
- `#[reprC(unsafe(is_valid = |target| ...))]` defines custom validity invariants for a `#[repr(transparent)]` type.
- If a custom validity invariant is given, `#[reprC(NICHE_VALUE = <expr>)]` defines the trap value used in niche optimization.
- `#[reprC(opaque)]` overrides all other behavior and requires the type to be passed behind a pointer (no companion type).

### 2.2 The `#[export("ABI")]` Attribute

`#[export("ABI")]` generates Rust-inaccessible `extern "ABI"` companion functions and the symbols they will be exported under.
An `extern "ABI"` companion function is a function which has an `ABI`-compatible signature with `ABI`-compatible companion argument/return types.
The attribute **DOES NOT** modify the signature or behavior of the item it is attached to.

- By default, the attribute mangles export names as: `{crate_name}_{TraitName}_{trait_generic_args}_{SelfTy}_{self_ty_generic_args}_{method}`.
- `#[unsafe(no_mangle)]`/`#[unsafe(export_name = "...")]` override default name mangling with their own semantics.
- On an `impl` block, `#[export("ABI")]` generates a companion function for every eligible method in the block.
- On a `fn` item, `#[export("ABI")]` generates a companion function exported as `{crate_name}_{fn_name}`.
- `#[export(skip)]` on an impl method excludes that method from being processed by the attribute.
- Although not marked as `unsafe`, a low risk of symbol collision UB still exists.

### 2.3 The `extern_!` macro

`extern_!` declares Rust-inaccessible `extern "ABI"` companion functions that bodies of declared Rust code call into.
The macro **DOES NOT** modify the signatures or behavior of declared Rust items.

- By default, the macro infers `link_name` as: `{crate_name}_{TraitName}_{trait_generic_args}_{SelfTy}_{self_ty_generic_args}_{method}`.
- `extern_!` requires `#![abi = "..."]` that it applies to generated `extern "ABI"` companion function declarations.
- `extern_C!` is a specialization of `extern_!` with ABI fixed to `"C"` and is used for convenience.
- `#![link(crate = "...")]` defines the crate name prefix for the inferred `link_name`.
- `#[link_name = "..."]` overrides default name mangling with its own semantics.
- Although not declared `unsafe`, using extern symbols always carries a risk of UB.
