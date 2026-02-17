# Specification

## 1. Design Goal

`co3` is a Rust-side framework for exporting/importing C ABI functions by:

- Mapping Rust types to FFI-safe types (`ExternC::CType`).
- Converting values to/from those FFI types (`Encode`/`Decode`).
- Generating extern functions (`carbonate`/`decarbonate` and derive macros).

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

Conversion modes define how values cross the FFI boundary, including ownership behavior, pointer-identity semantics, and validation strictness. Each mode makes explicit tradeoffs and is selected through compile-time configuration:

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
