# Architecture

## 1. Mental Model

Conversion is built from four layers:

1. **Rust spec** (`RustSpec`): describe the Rust-specified type properties used by conversion.
2. **ABI mapping** (`ReprC`): map the Rust type into a robust `repr(C)` type based on those properties.
3. **Value conversion** (`Encode`/`Decode`): convert values to and from that robust `repr(C)` type.
4. **Post-call writeback** (`Store::sync`): apply deferred updates for mutable reference paths.
