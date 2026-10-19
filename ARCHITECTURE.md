# Architecture

## 1. Mental Model

Conversion is built from four layers:

1. **Rust spec** (`RustSpec`): describe the Rust-specified type properties used by conversion.
2. **ABI mapping** (`ReprC`): map the Rust type into a robust `repr(C)` type based on those properties.
3. **Value conversion** (`Encode`/`Decode`): convert values to and from that robust `repr(C)` type.
4. **Post-call writeback** (`Store::sync`): apply deferred updates for mutable reference paths.

Shared references to non-robust interior-mutable values cannot safely expose their storage directly for encoding, because that would allow unsynchronized access.
Unchecked direct encoding has its own `DirectEncodeOwned` trait and a separate store type. The direct store can be empty even when checked encoding needs write-back storage. `DirectEncode` is the corresponding marker for types accepted by the unchecked free functions; `Encode` implies `DirectEncode`.
`Encode`/`Decode` exclude owned types whose contents have unstable layout, because their allocation cannot be transferred to the C representation as-is. A `move Box<(u8,)>` would have to allocate a new box and convert its contents on encode, then rebuild a Rust box on decode. That cost would be hidden behind `move`, which implies ownership transfer. `Box<(u8,)>` therefore has owned conversion impls but no `Encode` or `Decode` marker, so it cannot use `move` even when `#[soft]` is present. Wrapping it in `ManuallyDrop` does not restore those markers.

The borrowed path can be requested with `#[soft]`: `Box<(u8,)>` is borrowed as `&(u8,)`. Encoding this reference clones the tuple into C-compatible storage, and decoding the borrowed value into a `Box` clones it again. `#[soft]` makes this conversion cost explicit.
