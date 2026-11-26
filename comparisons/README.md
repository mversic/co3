# FFI framework comparisons

## Framework versions

<a id="versions-and-scope"></a>

| Framework | Pinned dependency |
| --- | --- |
| CO3 | Parent checkout (`path = ".."`) |
| UniFFI | 0.32.2, git revision `2124d553b73444890fd913b90d8fdd98620def1d` |
| Diplomat | 0.17.0, diplomat-runtime 0.16.1 |
| Interoptopus | 0.16.6 |
| safer-ffi | 0.1.13 |
| BoltFFI | 0.32.1 |

Tests are verified with rustc 1.100.0 on `aarch64-apple-darwin`.

## Running the checks

```sh
python3 comparisons/check.py
python3 comparisons/check.py --framework co3 borrowed_return
```

* The runner checks builds against [expectations.json](expectations.json).
* Logs and the report go to `comparisons/results/`.

## Export matrix

Each row shows whether six frameworks can export the Rust declaration in the linked fixture.

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
| [syntax: byte alias](cases/syntax/byte_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: qualified byte](cases/syntax/qualified_byte.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: qualified impl](cases/syntax/qualified_impl.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | 🟩 BUILD[^2] | ⬜ — | 🟩 BUILD |
| [syntax: renamed import](cases/syntax/renamed_import.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: reexport](cases/syntax/reexport.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: associated projection](cases/syntax/associated_projection.rs) | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: generic alias](cases/syntax/generic_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via renamed import](cases/syntax/slice_renamed_import.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via reexport](cases/syntax/slice_reexport.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via associated projection](cases/syntax/slice_associated_projection.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: mutable bytes via type macro](cases/syntax/slice_type_macro.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: nested alias](cases/syntax/nested_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: external slice alias](cases/syntax/external_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^3][^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: slice alias](cases/syntax/slice_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^3][^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: slice reference alias](cases/syntax/slice_reference_alias.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^3][^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: string alias](cases/syntax/string_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^3][^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [syntax: type macro](cases/syntax/type_macro.rs) | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: unit return alias](cases/syntax/unit_return_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [syntax: conditional parameter](cases/syntax/conditional_parameter.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [borrowing: mutable scalar](cases/borrowing/mutable_scalar.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^4] |
| [borrowing: mutable record](cases/borrowing/mutable_record.rs) | 🟩 BUILD[^5] | 🟥 REJECT | 🟩 BUILD[^6] | 🟩 BUILD | 🟩 BUILD[^7] | 🟩 BUILD[^8] |
| [unstable layout: `&Point`](cases/borrowing/unstable_shared.rs) | 🟩 BUILD[^9] | 🟩 BUILD[^10] | 🟨 BUILD[^11] | 🟨 BUILD[^11] | 🟥 REJECT | 🟨 BUILD[^8][^11] |
| [unstable layout: `&mut Point`](cases/borrowing/unstable_exclusive.rs) | 🟩 BUILD[^9] | 🟥 REJECT | 🟨 BUILD[^6][^11] | 🟨 BUILD[^11] | 🟥 REJECT | 🟨 BUILD[^8][^11] |
| [borrowing: mutable slice](cases/borrowing/mutable_slice.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [borrowing: `&mut self` on a public record](cases/borrowing/mutable_receiver.rs) | 🟩 BUILD[^5] | 🟥 REJECT | 🟩 BUILD[^6] | 🟥 REJECT | ⬜ — | 🟩 BUILD[^8][^12] |
| [borrowing: `&mut self` on an opaque object](cases/borrowing/opaque_mutable_receiver.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD[^13] | 🟩 BUILD[^2] | ⬜ — | 🟨 BUILD[^14] |
| [borrowing: borrowed return](cases/borrowing/borrowed_return.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: borrowed record](cases/borrowing/borrowed_record.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD[^7] | 🟥 REJECT |
| [borrowing: optional reference](cases/borrowing/optional_reference.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [borrowing: nested reference](cases/borrowing/nested_reference.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [borrowing: nested reference with explicit outlives bound](cases/borrowing/nested_reference_named.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟥 REJECT | 🟩 BUILD[^15] | 🟥 REJECT |
| [borrowing: borrowed halves](cases/borrowing/borrowed_halves.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [borrowing: nonzero byte array](cases/borrowing/nonzero_u8_array.rs) | 🟩 BUILD[^16] | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [generics: generic function](cases/generics/generic_function.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [generics: const generic](cases/generics/const_generic.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [control: `Option<Box<u32>>` field](cases/controls/control_optional_box.rs) | 🟩 BUILD[^17] | 🟩 BUILD | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [recursive list: `Option<Box<Node>>` field](cases/types/recursive_optional_box.rs) | 🟩 BUILD[^17] | 🟩 BUILD | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [control: `Vec<u32>` field](cases/controls/control_vector_field.rs) | 🟩 BUILD[^17] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [recursive tree: `Vec<Node>` field, `#[repr(C)]`](cases/types/recursive_vector_field.rs) | 🟩 BUILD[^17] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [recursive tree: `Vec<Node>` field, no explicit repr](cases/types/recursive_vector_default_repr.rs) | 🟩 BUILD[^17] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [types: array](cases/types/array.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [types: tuple](cases/types/tuple.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [types: pointer sized](cases/types/pointer_sized.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [types: u128](cases/types/u128.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: character](cases/types/character.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [stable niche: `Option<NonZeroU8>`](cases/types/niche_nonzero.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1][^18] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [unstable niche: `Option<bool>`](cases/types/niche_bool.rs) | 🟩 BUILD | 🟨 BUILD[^18] | 🟨 BUILD[^18] | 🟥 REJECT | 🟥 REJECT | 🟨 BUILD[^18] |
| [types: opaque object](cases/types/opaque_type.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [types: custom dst](cases/types/custom_dst.rs) | 🟩 BUILD[^19] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [types: function pointer](cases/types/function_pointer.rs) | 🟩 BUILD | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [traits: associated type](cases/traits/associated_type.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | ⬜ — | ⬜ — | 🟥 REJECT |
| [traits: default trait method](cases/traits/default_trait_method.rs) | 🟩 BUILD | 🟥 REJECT | ⬜ — | ⬜ — | ⬜ — | 🟥 REJECT |
| [traits: thread confined](cases/traits/thread_confined.rs) | 🟩 BUILD | 🟥 REJECT | 🟩 BUILD | 🟩 BUILD[^2] | ⬜ — | 🟨 BUILD[^14] |
| [controls: owned alias](cases/controls/control_owned_alias.rs) | 🟩 BUILD | 🟩 BUILD | 🟨 BUILD[^1] | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT |
| [controls: mutable bytes](cases/controls/control_mutable_bytes.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟩 BUILD |
| [controls: scalar](cases/controls/control_scalar.rs) | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD | 🟩 BUILD |
| [interior mutability: exclusive cell](cases/interior_mutability/exclusive_cell.rs) | 🟩 BUILD[^16] | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [interior mutability: shared cell](cases/interior_mutability/shared_cell.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [validity: mutable nonzero byte](cases/validity/mutable_nonzero_u8.rs) | 🟩 BUILD[^20] | 🟥 REJECT | 🟨 BUILD[^1] | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT |
| [traits: projected return](cases/traits/projected_return.rs) | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT | 🟥 REJECT |
| [traits: call a method on a supplied receiver](cases/traits/trait_object.rs) | 🟥 REJECT | 🟩 BUILD[^21] | 🟩 BUILD | 🟩 BUILD[^22] | 🟩 BUILD[^23] | 🟩 BUILD |
| [capabilities: async export](cases/capabilities/async_export.rs) | 🟥 REJECT | 🟩 BUILD | 🟥 REJECT | 🟥 REJECT | ⬜ — | 🟩 BUILD |
<!-- matrix:end -->

[^1]: Diplomat’s binding generator rejects this declaration. [Diagnostics](expectations.json)
[^2]: Interoptopus requires `#[ffi(service)]` on the type to export its methods.
[^3]: ABI unsoundness: these Diplomat exports retain a Rust slice reference or a Rust `String` in the C signature.
[^4]: BoltFFI lowers `&mut u32` to `u32`, so the increment never reaches the caller.
[^5]: CO3 borrows these records in place with `#[repr(C)]` while default layouts require `#[soft]`.
[^6]: Diplomat requires `#[diplomat::attr(auto, mut_struct_ref)]` on the record to generate bindings for a mutable borrow.
[^7]: The safer-ffi derives require an explicit representation for these records. The fixtures use `#[repr(C)]`.
[^8]: BoltFFI requires the record to implement `Copy`. The fixture uses `#[derive(Clone, Copy)]`.
[^9]: CO3 requires `#[soft]` to borrow the default-layout record. Callers import the exports with `ffi!` using a separate `#[repr(C)] CallerPoint`. Shared local validation checks reads and, for the mutable export, both fields after two successive calls.
[^10]: UniFFI deserializes a temporary `Point` and lends it to the function.
[^11]: The framework implicitly adds `#[repr(C)]` to `Point` itself.
[^12]: BoltFFI exports methods on a data record with `#[boltffi::data(impl)]` on the `impl` block.
[^13]: Diplomat requires `#[diplomat::opaque_mut]` on the object for an `&mut self` receiver.
[^14]: BoltFFI requires `#[export(single_threaded)]`. The caller must guarantee single-threaded access.
[^15]: safer-ffi requires the explicit bound `'inner: 'outer` on this nested reference.
[^16]: The `#[soft]` on this CO3 export is optional. CO3 can validate the nonzero bytes on entry and borrow the caller’s storage in place.
[^17]: CO3 requires `-> move Node` to return this owning record.
[^18]: The export uses a separate presence tag.
[^19]: CO3 borrows `#[repr(C)] Packet` in place; default-layout derives succeed, but decoding `&Packet` fails even with `#[soft]`.
[^20]: Checked imports use `#[soft]` to validate writes before writeback.
[^21]: UniFFI requires `#[uniffi::export(callback_interface)]` on the trait to accept this boxed callback.
[^22]: Interoptopus uses a named callback with receiver state for this single-method interface.
[^23]: safer-ffi requires `#[safer_ffi::derive_ReprC(dyn)]` on the trait used with `VirtualPtr`.

## Importing existing C interfaces

* **CO3 can bind an existing C symbol** by describing its signature in Rust inside `ffi!` through the same API as exports.
* No other framework has an equivalent feature. Users can still declare foreign functions in an `unsafe extern "C"` block.
* Interoptopus supports exporting Rust-to-foreign interface through `plugin!` but still cannot bind arbitrary C symbols.

## Syntactic vs semantic type resolution

* CO3 **selects the ABI on the types rustc resolves**, so different spellings like aliases and projections resolve correctly.
* In every other framework compared here, a **type’s spelling can affect whether the framework accepts it** or how it's lowered.

This gap between other frameworks and `CO3` is structural and should not be expected to be closed any time soon if ever.

## Niche optimization

* Tested frameworks don't apply niche optimization even when they support the type.
* **CO3 aggressively applies niche optimization** for both stable and unstable types.

## Foreign writes to non-robust `&mut T`

* Tested frameworks don't protect against invalid foreign writes even when they support the type.
* CO3’s checked `#[soft]` **imports validate foreign writes** before updating the Rust value.

## Sharing an `&UnsafeCell<T>` across the boundary

* All other tested export frameworks reject `&UnsafeCell<u32>`.
* **CO3 supports `&UnsafeCell<u32>`** without reading its contents.

## Custom DSTs

* All tested frameworks, including `stabby` and `abi_stable`, reject it.
* **CO3 supports custom DSTs** but currently requiring an explicit repr.

## Async exports and trait objects

* CO3 **currently lacks** support for trait-objects and async exports.
* Some of the other tested framworks provide these functionalities

This is not a structural inability, but an evidence of a lack of time and can be implemented upon request.
