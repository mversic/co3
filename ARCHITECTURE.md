# Architecture

## 1. Mental Model

Conversion is built from four layers:

1. **Rust spec** (`RustSpec`): describe the Rust-specified type properties used by conversion.
2. **ABI mapping** (`ReprC`): map the Rust type into an exposed C-compatible value representation or an opaque type used behind indirection.
3. **Value conversion** (`Encode`/`Decode`): convert Rust values and references using the corresponding ABI representations.
4. **Post-call writeback** (`Store::sync`): apply deferred updates for mutable reference paths.

- Pointer reads that duplicate borrowed values require `Copy`, preventing duplication of mutable references or ownership. Ownership-consuming reads may move a non-`Copy` value exactly once, provided the source is never used or dropped afterwards.
- `CType` includes opaque types, which may contain mutable references or have Rust drop behavior. `CType` alone does not justify copying a value or inspecting shared interior-mutable contents.
- Conversion and validation may inspect `UnsafeCell<T>`/`Cell<T>` contents when the cell is owned or exclusively accessible. Shared interior-mutable paths must never inspect their contents, including nested cells; a temporary shared reference within an exclusive-access operation does not change that access contract.
- The contents of `MaybeUninit<T>` MUST NEVER be read during conversion or validation.
