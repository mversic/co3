# Rust FFI framework comparisons

## Export matrix

<a id="versions-and-scope"></a>

Each row shows whether six FFI frameworks can export the Rust declaration in the linked example. Separate comparisons below cover abi_stable and stabby. The examples were verified with rustc 1.100.0 on `aarch64-apple-darwin`.

| Framework | Pinned dependency |
| --- | --- |
| CO3 | Parent checkout (`path = ".."`) |
| UniFFI | 0.32.2, git revision `2124d553b73444890fd913b90d8fdd98620def1d` |
| Diplomat | 0.17.0, diplomat-runtime 0.16.1 |
| Interoptopus | 0.16.6 |
| safer-ffi | 0.1.13 |
| BoltFFI | 0.32.1 |
| abi_stable | 0.11.3 |
| stabby | 36.2.2 |

| Color | Meaning |
| --- | --- |
| 🟩 | The Rust example builds. |
| 🟨 | The Rust example builds with a limitation. |
| 🟥 | The compiler rejects the declaration. |
| ⬜ | No equivalent fixture for this framework. |

<a id="equivalent-type-spelling-experiments"></a>

<!-- matrix:start -->
| Case | CO3 | UniFFI | Diplomat | Interoptopus | safer-ffi | BoltFFI |
| --- | --- | --- | --- | --- | --- | --- |
| [syntax: conditional parameter](cases/syntax/conditional_parameter.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: byte alias](cases/syntax/byte_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: qualified byte](cases/syntax/qualified_byte.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: qualified impl](cases/syntax/qualified_impl.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | 🟩 BUILD[^6] | ⬜ — | 🟩 BUILD |
| [syntax: renamed import](cases/syntax/renamed_import.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: reexport](cases/syntax/reexport.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: associated projection](cases/syntax/associated_projection.rs) | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: generic alias](cases/syntax/generic_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via renamed import](cases/syntax/slice_renamed_import.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via reexport](cases/syntax/slice_reexport.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via associated projection](cases/syntax/slice_associated_projection.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via type macro](cases/syntax/slice_type_macro.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: nested alias](cases/syntax/nested_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: external slice alias](cases/syntax/external_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^2][^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: slice alias](cases/syntax/slice_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^2][^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: slice reference alias](cases/syntax/slice_reference_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^2][^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: string alias](cases/syntax/string_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^2][^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: type macro](cases/syntax/type_macro.rs) | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: unit return alias](cases/syntax/unit_return_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: mutable scalar](cases/borrowing/mutable_scalar.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^5] |
| [borrowing: mutable record](cases/borrowing/mutable_record.rs) | 🟩 BUILD[^13] | 🟥 REJECT | 🟩 BUILD[^8] | 🟩 BUILD | 🟩 BUILD[^7] | 🟩 BUILD[^9] |
| [unstable layout: `&Point`](cases/borrowing/unstable_shared.rs) | 🟩 BUILD[^21] | 🟩 BUILD[^22] | 🟨 BUILD[^23] | 🟨 BUILD[^23] | 🟥 REJECT | 🟨 BUILD[^9][^23] |
| [unstable layout: `&mut Point`](cases/borrowing/unstable_exclusive.rs) | 🟩 BUILD[^21] | 🟥 REJECT | 🟨 BUILD[^8][^23] | 🟨 BUILD[^23] | 🟥 REJECT | 🟨 BUILD[^9][^23] |
| [borrowing: mutable slice](cases/borrowing/mutable_slice.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [borrowing: `&mut self` on a public record](cases/borrowing/mutable_receiver.rs) | 🟩 BUILD[^13] | 🟥 REJECT | 🟩 BUILD[^8] | 🟥 REJECT | ⬜ — | 🟩 BUILD[^9][^10] |
| [borrowing: `&mut self` on an opaque object](cases/borrowing/opaque_mutable_receiver.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD[^11] | 🟩 BUILD[^6] | ⬜ — | 🟨 BUILD[^1] |
| [borrowing: borrowed return](cases/borrowing/borrowed_return.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: borrowed record](cases/borrowing/borrowed_record.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD[^7] | 🟥 REJECT |
| [borrowing: optional reference](cases/borrowing/optional_reference.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: nested reference](cases/borrowing/nested_reference.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [borrowing: nested reference with explicit outlives bound](cases/borrowing/nested_reference_named.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟥 REJECT | 🟩 BUILD[^12] | 🟥 REJECT |
| [borrowing: borrowed halves](cases/borrowing/borrowed_halves.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [borrowing: nonzero byte array](cases/borrowing/nonzero_u8_array.rs) | 🟩 BUILD[^19] | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [generics: generic function](cases/generics/generic_function.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [generics: const generic](cases/generics/const_generic.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [control: `Option<Box<u32>>` field](cases/controls/control_optional_box.rs) | 🟩 BUILD[^14] | 🟩 BUILD | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [recursive list: `Option<Box<Node>>` field](cases/types/recursive_optional_box.rs) | 🟩 BUILD[^14] | 🟩 BUILD | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [control: `Vec<u32>` field](cases/controls/control_vector_field.rs) | 🟩 BUILD[^14] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [recursive tree: `Vec<Node>` field, `#[repr(C)]`](cases/types/recursive_vector_field.rs) | 🟩 BUILD[^14] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [recursive tree: `Vec<Node>` field, no explicit repr](cases/types/recursive_vector_default_repr.rs) | 🟩 BUILD[^14] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [types: array](cases/types/array.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [types: tuple](cases/types/tuple.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [types: pointer sized](cases/types/pointer_sized.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [types: u128](cases/types/u128.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: character](cases/types/character.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [stable niche: `Option<NonZeroU8>`](cases/types/niche_nonzero.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4][^17] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [unstable niche: `Option<bool>`](cases/types/niche_bool.rs) | 🟩 BUILD | 🟨 BUILD[^17] | 🟨 BUILD[^17] | 🟥 REJECT | 🟥 REJECT | 🟨 BUILD[^17] |
| [types: opaque object](cases/types/opaque_type.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [types: custom dst](cases/types/custom_dst.rs) | 🟩 BUILD[^18] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: function pointer](cases/types/function_pointer.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [traits: associated type](cases/traits/associated_type.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | ⬜ — | ⬜ — | 🟥 REJECT |
| [traits: default trait method](cases/traits/default_trait_method.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | ⬜ — | ⬜ — | 🟥 REJECT |
| [traits: thread confined](cases/traits/thread_confined.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD[^6] | ⬜ — | 🟨 BUILD[^1] |
| [controls: owned alias](cases/controls/control_owned_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^4] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [controls: mutable bytes](cases/controls/control_mutable_bytes.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [controls: scalar](cases/controls/control_scalar.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [interior mutability: exclusive cell](cases/interior_mutability/exclusive_cell.rs) | 🟩 BUILD[^19] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [interior mutability: shared cell](cases/interior_mutability/shared_cell.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [validity: mutable nonzero byte](cases/validity/mutable_nonzero_u8.rs) | 🟩 BUILD[^20] | 🟥 REJECT | 🟨 BUILD[^4] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [traits: projected return](cases/traits/projected_return.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [traits: call a method on a supplied receiver](cases/traits/trait_object.rs) | 🟥 REJECT | 🟩 BUILD[^15] | 🟩 BUILD | 🟩 BUILD[^3] | 🟩 BUILD[^16] | 🟩 BUILD |
| [capabilities: async export](cases/capabilities/async_export.rs) | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | ⬜ — | 🟩 BUILD |
<!-- matrix:end -->

[^1]: BoltFFI requires `#[export(single_threaded)]`. The caller must guarantee single-threaded access.
[^2]: ABI unsoundness: these Diplomat exports retain a Rust slice reference or a Rust `String` in the C signature.
[^3]: Interoptopus uses a named callback with receiver state for this single-method interface.
[^4]: Diplomat’s binding generator rejects this declaration. [Diagnostics](expectations.json)
[^5]: BoltFFI lowers `&mut u32` to `u32`, so the increment never reaches the caller.
[^6]: Interoptopus requires `#[ffi(service)]` on the type to export its methods.
[^7]: The safer-ffi and abi_stable derives require an explicit representation for these records. The fixtures use `#[repr(C)]`.
[^8]: Diplomat requires `#[diplomat::attr(auto, mut_struct_ref)]` on the record to generate bindings for a mutable borrow.
[^9]: BoltFFI requires the record to implement `Copy`. The fixture uses `#[derive(Clone, Copy)]`.
[^10]: BoltFFI exports methods on a data record with `#[boltffi::data(impl)]` on the `impl` block.
[^11]: Diplomat requires `#[diplomat::opaque_mut]` on the object for an `&mut self` receiver.
[^12]: safer-ffi requires the explicit bound `'inner: 'outer` on this nested reference.
[^13]: Every bit pattern is valid for the `u32` fields of `Point` and `Counter`, so both records are robust. With `#[repr(C)]` their layout is also stable and CO3 borrows them in place. With Rust’s default layout, these exports require `#[soft] point: &mut Point` or `#[soft] &mut self` so CO3 can use a temporary and write back the changed fields.
[^14]: CO3 requires `-> move Node` to return this owning record.
[^15]: UniFFI requires `#[uniffi::export(callback_interface)]` on the trait to accept this boxed callback.
[^16]: safer-ffi requires `#[safer_ffi::derive_ReprC(dyn)]` on the trait used with `VirtualPtr`.
[^17]: The export uses a separate presence tag.
[^18]: CO3 borrows the slice-tailed `Packet` in place with `#[repr(C)]`. With Rust’s default layout the derive succeeds. Both the ordinary and `#[soft]` exports fail to decode `&Packet`.
[^19]: The `#[soft]` on this CO3 export is optional. CO3 can validate the nonzero bytes on entry and borrow the caller’s storage in place.
[^20]: The CO3 export accepts an ordinary `&mut NonZeroU8`. In the reverse direction, a checked `ffi!` import requires `#[soft]` on the parameter so CO3 can validate the foreign write before updating the Rust value. The fixture’s raw function pointer selects this behavior with `soft_call`.
[^21]: CO3 requires `#[soft]` to borrow the default-layout record. The mutable export writes both fields back.
[^22]: UniFFI deserializes a temporary `Point` and lends it to the function.
[^23]: The framework implicitly adds `#[repr(C)]` to `Point` itself.

## Importing existing C interfaces

* **CO3 can bind an existing C symbol** by describing its signature in Rust inside `ffi!` through the same API as exports.
* No other framework has an equivalent feature. Users can still declare foreign functions in an `unsafe extern "C"` block.
* Interoptopus supports exporting Rust-to-foreign interface through `plugin!` but still cannot bind arbitrary C symbols.

## Syntactic vs semantic type resolution

* CO3 **selects the ABI on the types rustc resolves**, so different spellings like aliases and projections resolve correctly.
* In every other framework compared here, a **type’s spelling can affect whether the framework accepts it** or how it's lowered.

This gap between other frameworks and `CO3` is structural and should not be expected to be closed any time soon if ever.

## Niche optimization

CO3 uses one byte for both exports. The fixtures call the exports with all 256 byte values for the nonzero case and all three boolean option states. [Rust’s representation guarantees](https://doc.rust-lang.org/std/option/index.html#representation) · [Nonzero fixture](cases/types/niche_nonzero.rs) · [Boolean fixture](cases/types/niche_bool.rs)

Diplomat uses a two-byte tagged value for both Rust exports. Its binding generator also rejects the nonzero fixture’s `core::num::NonZeroU8` import. UniFFI and BoltFFI serialize the boolean option into a buffer with one byte for `None` and two for `Some`.

| Option passed and returned | abi_stable | stabby |
| --- | --- | --- |
| [`Option<NonZeroU8>`](cases/types/niche_nonzero.rs) | 🟩 BUILD · 1 byte | 🟩 BUILD · 1 byte |
| [`Option<bool>`](cases/types/niche_bool.rs) | 🟥 REJECT | 🟥 REJECT |
| [Framework option around `bool`](cases/types/framework_option_bool.rs) | 🟨 BUILD[^17] · 2 bytes (`ROption`) | 🟩 BUILD · 1 byte (`stabby::abi::option::Option`) |

## Foreign writes to `&mut NonZeroU8`

Diplomat and Interoptopus accept the mutable nonzero reference. When Rust lends that reference to a foreign callback, the foreign code must preserve the nonzero invariant. A write of zero followed by a Rust read as `NonZeroU8` produces an invalid value, which is undefined behavior. [Export and writeback fixture](cases/validity/mutable_nonzero_u8.rs)

The CO3 fixture calls the raw function pointer with `soft_call`, which gives the foreign function a temporary byte and validates it before updating the Rust value. The fixture starts with 1. Writing 7 succeeds and updates the original to 7. Writing 0 makes `soft_call` return `None` and leaves the original at 1. CO3 also offers an explicitly unsafe unchecked conversion for callers that can guarantee validity. [Runnable writeback example](cases/validity/mutable_nonzero_u8.rs)

## Sharing an `UnsafeCell` across the boundary

CO3, abi_stable 0.11.3, and stabby 36.2.2 build the direct export taking `&UnsafeCell<u32>`. Its fixture checks that two shared references identify the same storage and that a foreign write to that storage is visible to Rust. The example runs on one thread. `UnsafeCell` permits mutation through a shared reference. Concurrent accesses still require synchronization when one of them writes. UniFFI, Diplomat, Interoptopus, safer-ffi, and BoltFFI reject this declaration for different reasons. [Shared-cell fixture](cases/interior_mutability/shared_cell.rs) · [Compiler diagnostics](expectations.json)

abi_stable and stabby also build the [exclusive-cell export](cases/interior_mutability/exclusive_cell.rs) taking `&mut UnsafeCell<NonZeroU8>`. Their cell support requires the payload to implement `StableAbi` or `IStable`, respectively. They describe and check an existing layout rather than converting the payload to a different representation. Supporting these references therefore does not require inspecting shared cell contents or synchronizing converted temporary storage. CO3's runtime address and writeback checks in the shared-cell fixture remain specific to CO3; the peer checks establish build support only.

## Async exports and trait objects

* CO3 **currently lacks** support for trait-objects and async exports.
* Some of the other tested framworks provide these functionalities

This is not a structural inability, but an evidence of a lack of time and can be implemented upon request.

## abi_stable and stabby syntax comparisons

Both frameworks accept aliases, associated projections, and re-exports of supported types. Both reject native slices regardless of spelling, while accepting aliases of their own stable slice types. Those native-slice failures establish a type-support limitation, not dependence on spelling. abi_stable separately rejects type-position macros by default because its derive cannot inspect lifetimes inside them.

| Case | abi_stable | stabby |
| --- | --- | --- |
| [byte alias](cases/syntax/byte_alias.rs) | 🟥 REJECT | 🟥 REJECT |
| [qualified byte](cases/syntax/qualified_byte.rs) | 🟥 REJECT | 🟥 REJECT |
| [control mutable bytes](cases/controls/control_mutable_bytes.rs) | 🟥 REJECT | 🟥 REJECT |
| [control scalar](cases/controls/control_scalar.rs) | 🟩 BUILD | 🟩 BUILD |
| [renamed import](cases/syntax/renamed_import.rs) | 🟩 BUILD | 🟩 BUILD |
| [reexport](cases/syntax/reexport.rs) | 🟩 BUILD | 🟩 BUILD |
| [associated projection](cases/syntax/associated_projection.rs) | 🟩 BUILD | 🟩 BUILD |
| [generic alias](cases/syntax/generic_alias.rs) | 🟥 REJECT | 🟥 REJECT |
| [mutable bytes via renamed import](cases/syntax/slice_renamed_import.rs) | 🟥 REJECT | 🟥 REJECT |
| [mutable bytes via reexport](cases/syntax/slice_reexport.rs) | 🟥 REJECT | 🟥 REJECT |
| [mutable bytes via associated projection](cases/syntax/slice_associated_projection.rs) | 🟥 REJECT | 🟥 REJECT |
| [mutable bytes via type macro](cases/syntax/slice_type_macro.rs) | 🟥 REJECT | 🟥 REJECT |
| [nested alias](cases/syntax/nested_alias.rs) | 🟩 BUILD | 🟩 BUILD |
| [external slice alias](cases/syntax/external_alias.rs) | 🟥 REJECT | 🟥 REJECT |
| [slice alias](cases/syntax/slice_alias.rs) | 🟥 REJECT | 🟥 REJECT |
| [slice reference alias](cases/syntax/slice_reference_alias.rs) | 🟥 REJECT | 🟥 REJECT |
| [string alias](cases/syntax/string_alias.rs) | 🟥 REJECT | 🟥 REJECT |
| [type macro](cases/syntax/type_macro.rs) | 🟥 REJECT | 🟩 BUILD |
| [unit return alias](cases/syntax/unit_return_alias.rs) | 🟩 BUILD | 🟩 BUILD |
| [control stable slice](cases/controls/control_stable_slice.rs) | 🟩 BUILD | 🟩 BUILD |
| [stable slice alias](cases/syntax/stable_slice_alias.rs) | 🟩 BUILD | 🟩 BUILD |

The existing fixtures keep the same native Rust signatures. abi_stable uses `#[sabi_extern_fn]` plus a `StableAbi`-derived interface containing the function pointer: the export attribute alone does not establish ABI stability. stabby uses `#[stabby::stabby]` with `#[no_mangle]` to check and export the signature. In the pinned stabby version, `#[stabby::export]` without an explicit calling convention fails even the scalar control.

The [stable-slice control](cases/controls/control_stable_slice.rs) and its [alias](cases/syntax/stable_slice_alias.rs) use `RSliceMut` for abi_stable and `SliceMut` for stabby. Neither substitutes a stable counterpart into the native-slice fixtures.

```sh
python3 comparisons/check.py --offline --framework abi_stable --framework stabby
```

### Function-pointer field aliases

abi_stable's derive accepts a directly written function-pointer field but rejects an alias of exactly the same type. Its derive recognizes function-pointer syntax to produce the layout metadata; the aliased form instead requires a `StableAbi` implementation that the function-pointer type does not have. This is a concrete syntax-dependent limitation, despite abi_stable using trait-based layout information elsewhere. CO3 and stabby accept both forms.

```rust
pub type Callback = extern "C" fn(u32) -> u32;

// With abi_stable::StableAbi derived, the direct field is accepted:
pub struct DirectCallbacks {
    pub apply: extern "C" fn(u32) -> u32,
}

// With the same derive, the aliased field is rejected:
pub struct AliasedCallbacks {
    pub apply: Callback,
}
```

| Record derive | CO3 | abi_stable | stabby |
| --- | --- | --- | --- |
| [Direct function-pointer field](cases/controls/control_callback_field.rs) | 🟩 BUILD | 🟩 BUILD[^7] | 🟩 BUILD |
| [Aliased function-pointer field](cases/syntax/callback_field_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD |
| [Type macro in the field](cases/syntax/callback_field_macro.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD |

The type macro expands to the same function-pointer type. abi_stable rejects type macros by default, and CO3 and stabby accept it.

These fixtures compare record derives, rather than the function exports in the main matrix. abi_stable reports that `extern "C" fn(u32) -> u32` does not implement `StableAbi` for the aliased field.

## Custom dynamically sized records

The export `fn kind(packet: &Packet) -> u32` borrows a `#[repr(C)]` record with a `u32` header and a trailing `[u8]` payload. The export reads only the header, but the DST reference must still carry the tail length.

| Payload field | CO3 | abi_stable 0.11.3 | stabby 36.2.2 |
| --- | --- | --- | --- |
| [Unsized tail: `[u8]`](cases/types/custom_dst.rs) | 🟩 BUILD[^18] | 🟥 REJECT | 🟥 REJECT |

abi_stable reports that `[u8]` does not implement `StableAbi`; stabby reports that it does not implement `IStable`. These results establish a limitation of their checked direct interfaces for this custom DST, not an inability to expose the packet through a redesigned interface. A stable slice field or an opaque handle would change the record or borrowing representation. [Recorded diagnostics](expectations.json)

```sh
python3 comparisons/check.py --offline --framework co3 --framework abi_stable --framework stabby custom_dst
```

## Recursive records

The owned fixtures export `fn echo(node: Node) -> Node`. Each control uses the same record with `u32` in place of the child node. UniFFI builds both the boxed list and the vector tree. Diplomat compiles the boxed list in Rust, then rejects the boxed input field during binding type analysis. BoltFFI builds the vector tree. Each rejects the other container even in the nonrecursive control. The CO3 and UniFFI fixtures round-trip a list and a tree through their exported functions and check the child values. The BoltFFI tree fixture also checks the returned child after decoding the output buffer. [Export matrix](#rust-ffi-framework-comparisons) · [Compiler diagnostics](expectations.json)

CO3 builds the native boxed list and the native vector tree. The tree builds with both Rust’s default record layout and `#[repr(C)]`. The stable-vector rows use `co3::vec::Vec`, `abi_stable::std_types::RVec`, and `stabby::vec::Vec` respectively.

| Owned field in the export | CO3 | abi_stable | stabby |
| --- | --- | --- | --- |
| [control: `Option<Box<u32>>` field](cases/controls/control_optional_box.rs) | 🟩 BUILD[^14] | 🟥 REJECT | 🟥 REJECT |
| [recursive list: `Option<Box<Node>>` field](cases/types/recursive_optional_box.rs) | 🟩 BUILD[^14] | 🟥 REJECT | 🟥 REJECT |
| [control: `Vec<u32>` field](cases/controls/control_vector_field.rs) | 🟩 BUILD[^14] | 🟥 REJECT | 🟥 REJECT |
| [recursive tree: `Vec<Node>` field, `#[repr(C)]`](cases/types/recursive_vector_field.rs) | 🟩 BUILD[^14] | 🟥 REJECT | 🟥 REJECT |
| [recursive tree: `Vec<Node>` field, no explicit repr](cases/types/recursive_vector_default_repr.rs) | 🟩 BUILD[^14] | 🟥 REJECT | 🟥 REJECT |
| [control: stable vector of `u32`](cases/controls/control_stable_vector_field.rs) | 🟩 BUILD[^14] | 🟩 BUILD[^7] | 🟩 BUILD |
| [recursive tree: stable vector of `Node`](cases/types/recursive_stable_vector_field.rs) | 🟥 REJECT | 🟩 BUILD[^7] | 🟩 BUILD |

abi_stable and stabby export the tree with their stable vectors. CO3’s `RustSpec` derive requires a recursive `Vec<Node>` to resolve to the standard vector. With an explicit `RustSpec` implementation, CO3’s `ReprC` derive gives the stable-vector tree with a `u32` payload an `AbiCompatible` implementation. [Export fixture](cases/types/recursive_stable_vector_field.rs) · [ABI assertion and round trip](../tests/abi/recursive.rs)

The following fixtures exercise record derives alone. Each alias case changes only the pointee spelling through `type LinkTarget = Node`.

| Recursive field in the derive | CO3 | abi_stable | stabby |
| --- | --- | --- | --- |
| [`*const Node`](cases/controls/control_recursive_pointer.rs) | 🟩 BUILD | 🟩 BUILD[^7] | 🟩 BUILD |
| [`*const LinkTarget`](cases/syntax/recursive_pointer_alias.rs) | 🟥 REJECT | 🟩 BUILD[^7] | 🟥 REJECT |
| [`&'static Node`](cases/controls/control_recursive_reference.rs) | 🟥 REJECT | 🟩 BUILD[^7] | 🟩 BUILD |
| [`&'static LinkTarget`](cases/syntax/recursive_reference_alias.rs) | 🟥 REJECT | 🟩 BUILD[^7] | 🟥 REJECT |

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

## ABI-stable Rust-to-Rust interop

| Default-layout record in the source | abi_stable | stabby |
| --- | --- | --- |
| [`&Point`](cases/borrowing/unstable_shared.rs) | 🟥 REJECT | 🟨 BUILD[^23] |
| [`&mut Point`](cases/borrowing/unstable_exclusive.rs) | 🟥 REJECT | 🟨 BUILD[^23] |

abi_stable and stabby can hide a concrete Rust type behind a borrowed trait object. The caller can invoke `value()` through abi_stable’s `Source_TO` with `TD_Opaque` or stabby’s `DynRef`. Both pass the runtime check. The direct `&Counter` signature is a separate case. It requires the pointee to implement `StableAbi` or `IStable`, respectively, and the private Rust type in the fixture implements neither. [Borrowed trait object](cases/traits/trait_object.rs) · [Direct reference](cases/types/opaque_type.rs)

abi_stable and stabby check ABI compatibility, but do not validate argument or return values at each call. These checks do not protect against data corruption that violates Rust validity invariants: a zero byte supplied as `NonZeroU8` is undefined behavior rather than a decode error. The same responsibility applies to foreign writes through references, including references to `UnsafeCell<NonZeroU8>`. Their acceptance of a signature establishes layout support, not checked decoding of its contents. [abi_stable entry-point transformation](https://docs.rs/abi_stable/0.11.3/abi_stable/attr.sabi_extern_fn.html) · [stabby ABI checks](https://docs.rs/crate/stabby/36.2.2)

Consider a Rust host and a separately compiled Rust library that exchange a vector. With CO3, the Rust-facing signature can use `Vec<u32>`. Both sides agree on the C-compatible representation selected by `ReprC`, so each side can use its own standard library's vector layout internally. The same symbol can serve a C caller that follows the representation and ownership contract. With `abi_stable`, the shared interface can instead use its ABI-stable vector type, `RVec<u32>`. [CO3 vector round trip](../tests/abi/ownership_call.rs) · [abi_stable's RVec](https://docs.rs/abi_stable/latest/abi_stable/std_types/struct.RVec.html)

**Dynamic library loading** lets a host select a library at runtime. With **abi_stable**, the host can load a dynamic library by path and check its version and root module layout. Its prefix types let an interface gain trailing fields while older callers keep using the prefix they understand. **stabby** provides stable layouts for annotated types, including enums and trait-object vtables, and provides checked symbol lookup through its optional `libloading` integration. These guarantees matter when the host and library ship independently. [abi_stable loading sequence](https://docs.rs/abi_stable/latest/abi_stable/library/index.html) · [Prefix types](https://docs.rs/abi_stable/latest/abi_stable/docs/prefix_types/index.html) · [stabby](https://github.com/ZettaScaleLabs/stabby)

**CO3’s conversions also work with dynamically loaded symbols.** An application can open a library, resolve a function pointer with the agreed ABI, encode its arguments with `co3::encode`, call it, and decode its result with `co3::decode`. CO3’s `raw fn` aliases can express the expected function-pointer type. The application keeps the library loaded while the pointer is in use and handles any agreement about the exporting library’s ABI version. Decoding validates returned values under the declared signature; a compatibility check establishes whether the host may call that signature in the first place. [CO3 conversion functions](../src/lib.rs) · [Raw function-pointer aliases](../tests/ui_pass/raw_companion_signatures.rs)

## Conditional compilation

The fixture puts both an optional parameter and its type behind the `tracing` feature. With tracing disabled, Rust removes both and the exported function should take a single `u32`. Only CO3 builds that declaration across the six pinned frameworks. [Conditional parameter fixture](cases/syntax/conditional_parameter.rs)

## Run the comparisons

From the repository root, run the full suite or select a framework and case.

```sh
python3 comparisons/check.py
python3 comparisons/check.py --offline --framework co3 byte_alias
```

Logs and `report.json` are in `comparisons/results/`. Expected outcomes are in [expectations.json](expectations.json).
