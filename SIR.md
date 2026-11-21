# Stable Intermediate Representation

SIR specifies a **compiler-independent Rust interface with a stable ABI**.

## Quick Glossary

| Term | Meaning |
| --- | --- |
| **Source type, `T`** | The Rust type declared by the interface author. |
| **C carrier, `C(T)`** | The robust C-compatible value representation of source type `T`. |
| **Lowering** | Mapping a source type and its use in a declaration to its ABI representation. |

## Properties of the interface

The carrier `C(T)` has the following properties, with the exceptions listed below:

| Property | Guarantee |
| --- | --- |
| **Stable layout** | Layout is defined and field order is preserved. |
| **Robust representation** | There are no trap representations. |
| **No drop glue** | Dropping the carrier runs no Rust cleanup. |
| **`Copy`** | The carrier implements Rust’s `Copy` trait. |

* **Opaque pointees** have private representations and may not carry any of the above guarantees.
* **Interior-mutable types** don't implement `Copy`. This also affects aggregates containing them.

The above listed properties apply recursively to fields and pointees (e.g. `&bool` is lowered to `*const abi::bool`)

## Stable Rust counterparts

* Rust does not guarantee a stable layout for most standard-library types including `Vec<T>` and `String`.
* CO3 offers stable counterparts under the same module path and type name, with `co3` as the crate prefix.
* Each counterpart’s C carrier keeps the same type name and lives in the `abi` submodule.

| Rust type | Stable counterpart | Lowered Rust type | Lowered stable counterpart |
| --- | --- | --- | --- |
| `std::vec::Vec<T>` | `co3::vec::Vec<T>` | `Native<abi::Vec<C(T)>>` | `abi::Vec<C(T)>` |
| `std::string::String` | `co3::string::String` | `Native<abi::String>` | `abi::String` |

`Native` identifies the native Rust form as opposed to `CO3` counterpart.

## Carrier names and type arguments

| Carrier family | Naming rule | Example |
| --- | --- | --- |
| Named&nbsp;built-in | Keep the original name but lower the type parameters. | `C(Vec<bool>) = Native<abi::Vec<abi::bool>>`. |
| Derived&nbsp;companion | Append `CType` but keep the original parameters. | `C(Frame<bool>) = FrameCType<bool>`. |
| Identity&nbsp;derive | Use the source type directly with `#[repr_c(identity)]`. | `C(Frame<u32>) = Frame<u32>`. |
| Niche&#8209;optimized&nbsp;enum | Use the `EnumVariant` name. | `C(Option<NonZeroU8>) = Native<OptionSome<C(NonZeroU8)>>`. |

The rules compose: `C(Vec<Frame<bool>>) = Native<abi::Vec<FrameCType<bool>>>`.

## Representation table

Each row describes a family of carriers `C(T)`:

| Rust source, `T` | Carrier, `C(T)` | C representation |
| --- | --- | --- |
| Integers, `usize`, `isize`, `f32`, `f64` | Same primitive | Match width, signedness and target ABI. |
| CO3’s `ffi::c_int`, `ffi::c_long`, etc. | Same named type | Use the corresponding C spelling, e.g. `int` or `long`. |
| `bool`, `char`, `cmp::Ordering` | `abi::bool`, `abi::char`, `abi::Ordering` | Transparent over `u8`, `u32`, and `i8`, respectively. |
| `[T; N]`, `(A, B, …)` | `[C(T); N]`, `Native<TupleN<C(A), C(B), …>>` | Inline fixed array, or a C record with fields in tuple order (arities 1–12). |
| `*const T`, `*mut T`, `NonNull<T>` | `*const C(T)`, `*mut C(T)`, `NonZero<*mut C(T)>` | For a thin pointee, a pointer with the indicated mutability. |
| Thin `&T`, `&mut T` | `Ref`, `RefMut`, `SoftRef`, `SoftRefMut` over `C(T)` | Transparent over the carrier’s data pointer. `&CStr` uses a thin pointer to `ffi::c_char`. |
| References to slices, `str`, or slice-tailed records | `WideRef`, `WideRefMut`, `SoftWideRef`, `SoftWideRefMut` over `C(T)` | Record with `data`, then `metadata`. Slice metadata is an element count; string metadata is a byte count. Both use `size_t`. Preserve the resolved pointer type. |
| `Box<T>`, `Box<[T]>`, `CString` | `Box<C(T)>`, `WideBox<[C(T)]>`, `Box<ffi::c_char>` | Owned mutable pointer; wide boxes add metadata. C strings are NUL-terminated. |
| `std::vec::Vec<T>`, `std::string::String` | `Native<abi::Vec<C(T)>>`, `Native<abi::String>` | Vector fields in order: mutable data pointer, `capacity: usize`, `len: usize`. Strings wrap a byte vector transparently. |
| `std::option::Option<T>` | `Native<Option<C(T)>>` or `Native<OptionSome<C(T)>>` | Tagged: `u8` (0 None, 1 Some), then payload storage. Niche-optimized: payload alone, with its niche encoding None. |
| `std::result::Result<T, E>` | `Native<Result<C(T), C(E)>>`, `Native<ResultOk<C(T), C(E)>>`, or `Native<ResultErr<C(T), C(E)>>` | Tagged: union of records containing a `u8` tag (0 Ok, 1 Err) and payload storage. Niche-optimized: the nonzero-sized payload’s niche encodes the zero-sized arm. |
| Nonzero integers, `Reverse<T>`, `ManuallyDrop<T>`, `MaybeUninit<T>`, `Cell<T>`, `UnsafeCell<T>` | `NonZero<C(T)>` or the corresponding constructor over `C(T)` | Preserve the inner storage layout. `MaybeUninit` storage may be uninitialized. A cell retains its shared-access restrictions. |
| Function pointer | `core::option::Option<extern "ABI" fn(…) -> …>` | Nullable function pointer with the declared ABI and lowered signature. |
| Derived struct or enum | `NameCType<RustArgs…>` | Follow the generated record, union, or transparent layout, preserving alignment and discriminants. |
| Opaque or extern `type Handle;` | Original opaque type behind indirection | Incomplete C type. Imported `OwnedHandle` lowers to `*mut Handle`. |
| Unit and phantom fields | `()`, `PhantomData<T>` | Zero-sized fields are left out. Unit returns use `void`. |

## Ownership transfer

`Borrowed<C(T)>` marks borrowed conversion for a function parameter or return where the owner retains its resources across the boundary:

| Rust parameter type, `T` | Carrier, `C(T)` | Default signature type | Explicit `move` |
| --- | --- | --- | --- |
| `std::vec::Vec<u8>` | `Native<abi::Vec<u8>>` | `Borrowed<C(T)>` | `C(T)` |
| `co3::vec::Vec<u8>` | `abi::Vec<u8>` | `Borrowed<C(T)>` | `C(T)` |

## From an export to a C declaration

```rust
use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
pub struct Frame<T> {
    sequence: u32,
    value: T,
}

pub fn flip(mut frame: Frame<bool>) -> Frame<bool> {
    frame.value = !frame.value;
    frame
}

ffi! {
    #![unsafe(export("C"))]
    #[symbol_name = "flip_frame"]
    fn flip(frame: move Frame<bool>) -> move Frame<bool>;
}
```

Both sides of the function signature lower to `FrameCType<bool>`. Its fields resolve to `u32` and `abi::bool`. A generator can give this instantiation the C name `Frame_bool` and emit:

```c
#include <stdint.h>

typedef struct {
    uint32_t sequence;
    uint8_t value;
} Frame_bool;

Frame_bool flip_frame(Frame_bool frame);
```

Apply the same procedure to every exported symbol for the selected build. Use the final parameter list, including any dispatch tags, and retain nested function-pointer signatures. Emit each concrete record or union definition before its first by-value use. Pointer cycles need forward declarations. Ownership, borrow duration, and application-specific validity requirements belong alongside the declarations.
