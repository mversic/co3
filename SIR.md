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
| **Robustness** | There are no trap representations. |
| **No drop glue** | Dropping the carrier runs no Rust cleanup. |
| **`Copy`** | The carrier implements Rust’s `Copy` trait. |

* **Opaque pointees** have private representations and may not carry any of the above guarantees.
* **Interior-mutable types** don't implement `Copy`. This also affects aggregates containing them.

These properties apply recursively to fields and pointees, subject to the exceptions above.

## Stable Rust counterparts

Since Rust does not guarantee a stable layout for most standard-library types:

* CO3 offers stable counterparts at the exact same path, with `co3` as the crate prefix.
* Each counterpart’s carrier keeps the same type name and resides in the `abi` submodule.
* The `Native` wrapper identifies the native Rust form as opposed to `CO3` counterpart.

Consider the following non-exhaustive examples:

| Rust type | Stable counterpart | Lowered Rust type | Lowered stable counterpart |
| --- | --- | --- | --- |
| `(A, ...)` | `co3::tuple::TupleN<A, ...>` | `Native<abi::TupleN<C(A), ...>>` | `abi::TupleN<C(A), ...>` |
| `Vec<T>` | `co3::vec::Vec<T>` | `Native<abi::Vec<C(T)>>` | `abi::Vec<C(T)>` |
| `String` | `co3::string::String` | `Native<abi::String>` | `abi::String` |
| `Option<u8>` | `co3::option::Option<u8>` | `Native<abi::Option<u8>>` | `abi::Option<u8>` |

## Type lowering

The types are lowered as follows:

| Rust source form | Lowering rule | Example |
| --- | --- | --- |
| Numeric primitives | Integers and floating-point values retain their primitive type. | `C(u32) = u32` |
| Non-robust primitives | Use the corresponding robust scalar carrier. | `C(bool) = abi::bool` |
| C scalars | Named C scalars retain their type and C spelling. | `C(ffi::c_int) = ffi::c_int` |
| Unit | The empty tuple is its own zero-sized carrier. | `C(()) = ()` |
| Phantom markers | Apply the phantom marker to the lowered payload. | `C(PhantomData<T>) = PhantomData<C(T)>` |
| `[T; N]` | Lower the element type and preserve the length. | `C([T; N]) = [C(T); N]` |
| `[T]` | Lower the element type and preserve the unsized slice form. | `C([T]) = [C(T)]` |
| `str` | Use `abi::str` as the pointee of a wide pointer. | `C(str) = abi::str` |
| `CStr` | Use `ffi::c_char` as the pointee of a thin pointer. | `C(CStr) = ffi::c_char` |
| `(A, ...)` | Select the tuple carrier by arity and lower each element in order. | `C((A, ...)) = Native<abi::TupleN<C(A), ...>>` |
| Raw pointers | Preserve mutability and lower the pointee. | `C(*mut T) = *mut C(T)` |
| References | Use `Ref`, `RefMut`, `WideRef` or `WideRefMut`. | `C(&[u8]) = abi::WideRef<'_, [u8]>` |
| Owned pointers | Use `Box` or `WideBox` over the lowered pointee. | `C(Box<[T]>) = abi::WideBox<[C(T)]>` |
| Transparent wrappers | Apply the corresponding wrapper to the lowered payload. | `C(UnsafeCell<T>) = UnsafeCell<C(T)>` |
| `std` containers | Use the ABI carrier of the same name with parameters lowered. | `C(Vec<T>) = Native<abi::Vec<C(T)>>` |
| Fieldless enums | Use the corresponding scalar carrier. | `C(Ordering) = abi::Ordering` |
| Tagged enums | Use the named ABI carrier with an explicit discriminant. | `C(Option<T>) = Native<abi::Option<C(T)>>` |
| Niche-optimized enums | Use a carrier named for the payload variant. | `C(Option<T>) = Native<abi::OptionSome<C(T)>>` |
| Function pointers | Wrap already ABI-compatible `fn` pointer into `Option`. | `C(extern "C" fn(u8)) = Option<extern "C" fn(u8)>` |
| Custom derived types | Append `CType` and retain the original Rust parameters. | `C(Frame<T>) = FrameCType<T>` |
| `#[repr_c(identity)]` | Use the source type itself as its carrier. | `C(Frame<T>) = Frame<T>` |
| Opaque types | Preserve the opaque pointee behind indirection. | `C(OwnedHandle) = *mut Handle` |

* Soft reference carriers include `SoftRef<C(T)>`, `SoftRefMut<C(T)>`, `SoftWideRef<C(T)>` and `SoftWideRefMut<C(T)>`.
* The rules compose to form the final lowered type. For example, `C(Vec<Frame<T>>) = Native<abi::Vec<FrameCType<T>>>`.

## Ownership transfer

`Borrowed<C(T)>` marks borrowed conversion for a function parameter or return where the owner retains its resources across the boundary:

| Rust parameter type, `T` | Carrier, `C(T)` | Default signature type | Explicit `move` |
| --- | --- | --- | --- |
| `std::vec::Vec<u8>` | `Native<abi::Vec<u8>>` | `Borrowed<C(T)>` | `C(T)` |
| `co3::vec::Vec<u8>` | `abi::Vec<u8>` | `Borrowed<C(T)>` | `C(T)` |
| `ManuallyDrop<Vec<u8>>` | `ManuallyDrop<Native<abi::Vec<u8>>>` | `Borrowed<C(T)>` | `C(T)` |
| `&'a mut Box<u32>` | `abi::SoftRefMut<abi::Box<u32>>` | `Borrowed<C(T)>` | `C(T)` |
| `&'a UnsafeCell<Box<u32>>` | `abi::Ref<'a, UnsafeCell<abi::Box<u32>>>` | `Borrowed<C(T)>` | `C(T)` |
| `u32` | `u32` | `C(T)` | `C(T)` |

For mutable references holding an owned type, the foreign callee **MUST NOT swap out** the owned type.

## From an export to a C declaration

Given a compliant C header generator, the following example:

```rust
use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
pub struct Message<'a> {
    urgent: bool,
    payload: &'a [u8],
}

pub fn send(message: Message<'_>) -> usize {
    // Process the message here.
    message.payload.len()
}

ffi! {
    #![unsafe(export("C"))]
    #[symbol_name = "send_message"]
    fn send<'a>(message: Message<'a>) -> usize;
}
```

should roughly correspond to:

```c
#include <stddef.h>
#include <stdint.h>

typedef struct {
    const uint8_t *data;
    size_t len;
} Slice_u8;

typedef struct {
    uint8_t urgent; /* 0 = false, 1 = true */
    Slice_u8 payload;
} Message;

typedef Message Borrowed_Message;

size_t send_message(Borrowed_Message message);
```
