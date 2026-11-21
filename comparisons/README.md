# Rust FFI framework comparisons

Each row shows whether six FFI frameworks can export the Rust declaration in the linked example.

<!-- matrix:start -->
| Case | CO3 | UniFFI | Diplomat | Interoptopus | safer-ffi | BoltFFI |
| --- | --- | --- | --- | --- | --- | --- |
| [syntax: conditional parameter](cases/syntax/conditional_parameter.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: byte alias](cases/syntax/byte_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: qualified byte](cases/syntax/qualified_byte.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: qualified impl](cases/syntax/qualified_impl.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | 🟩 BUILD | ⬜ — | 🟩 BUILD |
| [syntax: renamed import](cases/syntax/renamed_import.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: reexport](cases/syntax/reexport.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: associated projection](cases/syntax/associated_projection.rs) | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: generic alias](cases/syntax/generic_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: nested alias](cases/syntax/nested_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: external alias](cases/syntax/external_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: slice alias](cases/syntax/slice_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟧 BUILD‡ | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: slice reference alias](cases/syntax/slice_reference_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: string alias](cases/syntax/string_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: type macro](cases/syntax/type_macro.rs) | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: unit return alias](cases/syntax/unit_return_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: mutable scalar](cases/borrowing/mutable_scalar.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [borrowing: mutable record](cases/borrowing/mutable_record.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [borrowing: mutable slice](cases/borrowing/mutable_slice.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [borrowing: mutable receiver](cases/borrowing/mutable_receiver.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | ⬜ — | 🟨 BUILD† |
| [borrowing: borrowed return](cases/borrowing/borrowed_return.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: borrowed record](cases/borrowing/borrowed_record.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: optional reference](cases/borrowing/optional_reference.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: nested reference](cases/borrowing/nested_reference.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [borrowing: borrowed halves](cases/borrowing/borrowed_halves.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [borrowing: nonzero byte array](cases/borrowing/nonzero_u8_array.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [generics: generic function](cases/generics/generic_function.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [generics: const generic](cases/generics/const_generic.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: array](cases/types/array.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [types: tuple](cases/types/tuple.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [types: pointer sized](cases/types/pointer_sized.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [types: u128](cases/types/u128.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: character](cases/types/character.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: custom dst](cases/types/custom_dst.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: function pointer](cases/types/function_pointer.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [traits: associated type](cases/traits/associated_type.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | ⬜ — | ⬜ — | 🟥 REJECT |
| [traits: default trait method](cases/traits/default_trait_method.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | ⬜ — | ⬜ — | 🟥 REJECT |
| [traits: thread confined](cases/traits/thread_confined.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | ⬜ — | 🟨 BUILD† |
| [controls: owned alias](cases/controls/control_owned_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [controls: mutable bytes](cases/controls/control_mutable_bytes.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [controls: scalar](cases/controls/control_scalar.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [interior mutability: exclusive cell](cases/interior_mutability/exclusive_cell.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [interior mutability: shared cell](cases/interior_mutability/shared_cell.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [validity: mutable nonzero byte](cases/validity/mutable_nonzero_u8.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [traits: projected return](cases/traits/projected_return.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [traits: call a method on a supplied receiver](cases/traits/trait_object.rs) | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD¹ | 🟩 BUILD | 🟩 BUILD |
| [capabilities: async export](cases/capabilities/async_export.rs) | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | ⬜ — | 🟩 BUILD |
<!-- matrix:end -->

¹ Interoptopus uses a named callback with receiver state for this single-method interface.

† BoltFFI builds the mutable-receiver and thread-confined examples with `#[export(single_threaded)]`. Its default export annotation rejects both.

‡ Diplomat lowers a directly spelled slice to its FFI representation. With a type alias, its generated entry point retains the Rust slice reference. Both versions compile. [Slice alias analysis](../COMPARISONS.md#what-changes-when-a-slice-gets-a-name)

## Export matrix

| Result | Meaning |
| --- | --- |
| 🟩 BUILD | The Rust example compiled and linked. |
| 🟥 REJECT | The compiler reported the expected diagnostic for this declaration. |
| ⬜ — | The fixture has no equivalent declaration for this framework. |
| 🟨 BUILD† | The export requires the caller to guarantee single-threaded access. |
| 🟧 BUILD‡ | The example builds, but the generated wrapper omits the slice conversion. |

<a id="equivalent-type-spelling-experiments"></a>

## Syntactic VS semantic type resolution

UniFFI, Diplomat, and BoltFFI make some ABI decisions from a type’s spelling. UniFFI and BoltFFI accept `&mut [u8]` but reject `&mut Bytes` when `type Bytes = [u8]`. Diplomat compiles both spellings but omits the slice conversion for the alias. Interoptopus and safer-ffi reject both spellings of the native slice.

**CO3 defines each conversion once for the underlying Rust type.** Rust resolves `Bytes` to `[u8]`, so CO3 exposes the same lowered stable C type with either spelling. Every alias of a supported type gets its conversion automatically.

Frameworks that choose the ABI from the signature’s spelling are unlikely to close that gap without significant structural redesign.

## Generating a C header from SIR

CO3 uses the Stable Intermediate Representation (SIR) to describe **the complete exported Rust interface as seen by a C caller**. It consists of the lowered declarations and their compiler-resolved, C-compatible types. A header generator should be able to derive every exported function signature, static declaration, and required C type definition or opaque forward declaration from that representation alone. CO3 has no header generator yet, so this is a design goal awaiting end-to-end verification. [SIR reference for C header generators](../SIR.md)

Consider `type Bytes = [u8]` and a function taking `&mut Bytes`. By the time a generator sees the lowered parameter, Rust has resolved the alias and CO3 has selected the slice representation. The generator’s job is to express that representation in C. If CO3 later supports another Rust type using the same lowered types, the generator already has everything it needs. This keeps the Rust-to-ABI conversion rules in one place. Conditions on how a caller uses a value, such as how long it may retain a borrow, may still need documentation alongside the C declaration. [Slice alias fixture](cases/syntax/slice_alias.rs) · [ABI mapping](../ARCHITECTURE.md)

How much can a generator learn from the other frameworks’ lowered declarations? A C prototype can describe a buffer exactly while leaving open whether its bytes encode a string, a sequence, or a record. That distinction matters to anyone writing a caller.

| Framework | What the lowered types reveal and what the generator uses |
| --- | --- |
| safer-ffi | The closest parallel. Each `ReprC` type has an associated `CLayout: CType`, and exported parameters use that layout. Its working C header generator enumerates registered exports and invokes their definition emitters. This demonstrates type-directed header generation. Whether an independent generator could reproduce the complete header from the resolved declarations alone is open. [Type mapping](https://docs.rs/safer-ffi/0.1.13/safer_ffi/layout/trait.ReprC.html) · [Header builder](https://docs.rs/crate/safer-ffi/0.1.13/source/src/headers/_mod.rs) |
| Diplomat | A lowered `DiplomatSliceMut<u8>` exposes the data pointer and length a C caller passes. Diplomat’s generator also has a high-level interface model for concepts such as methods and borrows. Its existence alone tells us little about whether an independent generator could recover the raw C declarations. [Slice example](cases/borrowing/mutable_slice.rs) · [Diplomat HIR](https://docs.rs/diplomat_core/latest/diplomat_core/hir/index.html) |
| Interoptopus | A directly represented record exposes its fields. Its inventory adds descriptions of higher-level patterns, such as a named callback and its signature. Backends consume that inventory. [Inventory and backends](https://interoptopus.rs/architecture/) |
| UniFFI | Multiple source types lower to the same `RustBuffer`. Its declaration describes the buffer, while the payload schema comes from interface metadata that a generator can read from the compiled library. [Lowering and serialization](https://mozilla.github.io/uniffi-rs/next/internals/lifting_and_lowering.html) · [Bindings IR](https://mozilla.github.io/uniffi-rs/next/internals/bindings_ir.html) |
| BoltFFI | Both direct-vector and encoded return paths can emit `FfiBuf`. That return type alone cannot identify the element type or codec. BoltFFI can also emit declaration and type records in a dedicated binary section when source-record capture is enabled. [Direct-vector returns](https://docs.rs/crate/boltffi_macros/0.32.1/source/src/expansion/wrapper/returns/direct_vec.rs) · [Encoded returns](https://docs.rs/crate/boltffi_macros/0.32.1/source/src/expansion/wrapper/returns/encoded.rs) · [Capture records](https://docs.rs/crate/boltffi_macros/0.32.1/source/src/capture.rs) |

## Foreign writes to `&mut NonZeroU8`

Diplomat and Interoptopus accept the mutable nonzero reference. When Rust lends that reference to a foreign callback, the foreign code must preserve the nonzero invariant. A write of zero followed by a Rust read as `NonZeroU8` produces an invalid value, which is undefined behavior. [Callback signatures and generated wrappers](../COMPARISONS.md#who-checks-a-write-through-mut-nonzerou8)

CO3’s `#[soft]` import gives the foreign function a temporary byte and validates it before updating the Rust value. The fixture starts with 1. Writing 7 succeeds and updates the original to 7. Writing 0 makes `soft_call` return `None` and leaves the original at 1. CO3 also offers an explicitly unsafe unchecked conversion for callers that can guarantee validity. [Runnable writeback example](cases/validity/mutable_nonzero_u8.rs)

## Sharing an `UnsafeCell` across the boundary

Only CO3 builds the direct export taking `&UnsafeCell<u32>`. Its fixture checks that two shared references identify the same storage and that a foreign write to that storage is visible to Rust. The example runs on one thread. `UnsafeCell` permits mutation through a shared reference. Concurrent accesses still require synchronization when one of them writes. The five other frameworks reject this declaration for different reasons. [Shared-cell fixture](cases/interior_mutability/shared_cell.rs) · [Compiler diagnostics](expectations.json)

## Importing existing C interfaces

CO3 can bind an existing C symbol by describing its signature in Rust inside `ffi!`. The library only needs to provide the symbol with the agreed ABI. In the pinned versions of UniFFI, Diplomat, safer-ffi, and BoltFFI, we found no equivalent framework-provided import facility. Rust applications using any of them can still declare foreign functions in an `unsafe extern "C"` block. [CO3 import example](../tests/import/imported_abi.rs)

Interoptopus supports Rust-to-foreign calls through `plugin!`. Its loader expects the functions declared by the plugin **and** runtime symbols for registration and an interface check. An arbitrary existing C header and its library are therefore insufficient on their own. [Plugin guide](https://interoptopus.rs/constructs/reverse-interop/) · [Interoptopus 0.16.6 loader](https://docs.rs/crate/interoptopus_proc_impl/0.16.6/source/src/plugin/emit.rs)

## ABI-stable Rust-to-Rust interop

Consider a Rust host and a separately compiled Rust library that exchange a vector. With CO3, the Rust-facing signature can use `Vec<u32>`. Both sides agree on the C-compatible representation selected by `ReprC`, so each side can use its own standard library's vector layout internally. The same symbol can serve a C caller that follows the representation and ownership contract. With `abi_stable`, the shared interface can instead use its ABI-stable vector type, `RVec<u32>`. [CO3 vector round trip](../tests/abi/ownership_call.rs) · [abi_stable's RVec](https://docs.rs/abi_stable/latest/abi_stable/std_types/struct.RVec.html)

**Dynamic library loading** lets a host select a library at runtime. With **abi_stable**, the host can load a dynamic library by path and check its version and root module layout. Its prefix types let an interface gain trailing fields while older callers keep using the prefix they understand. **stabby** provides stable layouts for annotated types, including enums and trait-object vtables, and provides checked symbol lookup through its optional `libloading` integration. These guarantees matter when the host and library ship independently. [abi_stable loading sequence](https://docs.rs/abi_stable/latest/abi_stable/library/index.html) · [Prefix types](https://docs.rs/abi_stable/latest/abi_stable/docs/prefix_types/index.html) · [stabby](https://github.com/ZettaScaleLabs/stabby)

**CO3’s conversions also work with dynamically loaded symbols.** An application can open a library, resolve a function pointer with the agreed ABI, encode its arguments with `co3::encode`, call it, and decode its result with `co3::decode`. CO3’s `raw fn` aliases can express the expected function-pointer type. The application keeps the library loaded while the pointer is in use and handles any agreement about the exporting library’s ABI version. Decoding validates returned values under the declared signature; a compatibility check establishes whether the host may call that signature in the first place. [CO3 conversion functions](../src/lib.rs) · [Raw function-pointer aliases](../tests/ui_pass/raw_companion_signatures.rs)

## Conditional compilation

The fixture puts both an optional parameter and its type behind the `tracing` feature. With tracing disabled, Rust removes both and the exported function should take a single `u32`. Only CO3 builds that declaration across the six pinned frameworks. [Conditional parameter fixture](cases/syntax/conditional_parameter.rs)

## Current CO3 limitations

CO3 currently lacks direct async exports and generated trait-object adapters. UniFFI, Interoptopus, and BoltFFI build the async export. [Async export fixture](cases/capabilities/async_export.rs)

The trait-object fixture asks Rust to call `value()` on a supplied receiver. UniFFI and BoltFFI take ownership of a boxed callback trait. Diplomat generates a bridge for `impl Source`. With the `dyn-traits` feature, safer-ffi accepts a `VirtualPtr` borrowing the receiver. Interoptopus represents this single method with a named callback that captures the receiver. Each of these examples builds its export and calls the Rust entry point with two different receiver values. CO3’s direct `&dyn Source` export fails because the trait object lacks a `ReprC` implementation. [Runnable trait dispatch fixture](cases/traits/trait_object.rs)

Both features have straightforward paths within CO3’s existing conversion model. An async export could return a task handle with operations to poll and drop it. A trait object could cross as a data pointer paired with a C-compatible table of method pointers. These are prospective designs. [CO3 architecture](../ARCHITECTURE.md)

## Run the comparisons

Run these commands from the repository root.

```sh
# Verify every recorded result, including expected compiler failures.
python3 comparisons/check.py

# Once dependencies are downloaded:
python3 comparisons/check.py --offline

# Compare a single case across all frameworks:
python3 comparisons/check.py --offline tuple

# Check selected frameworks:
python3 comparisons/check.py --offline --framework co3 --framework uniffi byte_alias

# See one compiler result directly (a REJECT is expected to exit unsuccessfully):
cargo build --manifest-path comparisons/Cargo.toml --locked --example tuple --features uniffi
cargo build --manifest-path comparisons/Cargo.toml --locked --example tuple --features co3
```

With no case names, the checker builds every case.
It uses `cargo build` so the results include code generation and linking.
An expected rejection must match a compiler diagnostic from the example. A
network or dependency failure does not count. `OK ...: fail` means that the
expected rejection matched. `UNEXPECTED` means the build did not match the recorded outcome.
The checker exits unsuccessfully if any result is unexpected.

Full compiler logs are written to `results/<framework>/<case>.log`, and the latest
run's machine-readable outcomes to `results/report.json`. Results are ignored by
Git. Reviewed expected outcomes and diagnostic text live in
[expectations.json](expectations.json). Each build selects exactly one framework feature. A case marked — should be inspected through the checker.

## Versions and scope

| Framework | Pinned dependency |
| --- | --- |
| CO3 | Parent checkout (`path = ".."`) |
| UniFFI | 0.32.2, git revision `2124d553b73444890fd913b90d8fdd98620def1d` |
| Diplomat | 0.17.0, diplomat-runtime 0.16.1 |
| Interoptopus | 0.16.6 |
| safer-ffi | 0.1.13 |
| BoltFFI | 0.32.1 |

The article fixtures were verified with rustc 1.100.0 on `aarch64-apple-darwin`.
