# Specification

## 1. Design Goal

`co3` is a Rust-side framework for _safely_ exporting/importing C ABI functions by:

- Mapping Rust types to C-compatible types via `ReprC` derive macro.
- Writing export/extern FFI declarations via `ffi!` fn-like macro.

### 1.1. Hard Guarantees

These guarantees define the contract of this library:

1. **Native-Rust ergonomics**
- Ergonomics of APIs and generated wrappers **MUST** remain idiomatic to Rust users.
- FFI boundary mechanics **SHOULD** stay encapsulated in conversion traits and generated glue.

2. **Soundness-first FFI interoperability**
- Soundness **MUST NOT** be weakened for performance or memory footprint in the default configuration.
- If preserving soundness requires additional validation, temporary storage, or cloning, that cost is accepted.

3. **Zero-cost abstraction by default**
- Zero-cost abstraction **MUST** be preserved unless it directly conflicts with the soundness guarantee.
- Only explicit opt-in modes **MAY** prioritize performance by shifting soundness responsibility to the user.

### 1.2. FFI Conversion Modes

Conversion modes define how values cross the FFI boundary, including ownership behavior, pointer-identity semantics, and validation strictness.
Each mode makes explicit tradeoffs and is selected through compile-time configuration:

1. **`move` (opt-in, on fn argument and return types)**
- `Drop` types are borrowed instead of transferring ownership unless the argument/return is `move`d
- Owned backing storage is kept alive in a type-specific store while exposing borrowed FFI views.
- `move` removes the cost of cloning owned types in decode paths that is incurred by default.

2. **`#[soft]` (opt-in, on fn arguments)**
- Enables additional reference conversion paths that rely on cloning the referent (e.g. `&(u8,)`).
- Uses intermediate owned/cloned values and store synchronization for mutable writeback paths.
- Pointer identity is not preserved and pointer equality for these types **MUST NOT** be relied on.

3. **Tagged dispatch (opt-in, on impl blocks, free functions, and inherent methods)**
- Enables tagged generic dispatch where type's C-compatible representation is erased into a shared type and reinterpreted back via the tag value.
- Dispatched generics are defined by `<dyn({TagTy}) T = {ErasedTy}>` where `ErasedTy::CType` constrains size and alignment of erased types.
- A `use<T, ...> @ (<Param1> | ...)` where predicate declares the concrete types dispatched that have a tag value (use `#[tag(Type, unsafe(Val))]`).

## 2. Public API

Public API constitutes user-facing macros only (not the public trait and type exports).
Any generated glue code **MUST** remain private and **MUST NOT** leak into the public API.
Any conversion written manually against traits of this crate **DOES NOT** constitute public API.

### 2.1. `ffi!`

`ffi!` is a fn-like macro that enables writing export/extern declarations of types, statics, methods and impl blocks.
It must always start with a declaration of direction and ABI (e.g. `#![unsafe(export("system"))]`/`#![unsafe(extern("system"))]`).

- `#[cfg]` and `#[cfg_attr]` are fully supported in all attribute positions inside the `ffi` macro.
- `#![unsafe(export("ABI"))]` creates export declarations with the given ABI. The declared items must exist and be resolvable.
- `#![unsafe(extern("ABI"))]` creates import declarations with the given ABI. The macro is said to contain extern declarations.
- `#![feature(extern_types)]` opts into the corresponding unstable macro codegen path; no other feature names are supported.
- Static item symbol names are inferred as `{symbol_prefix}__{static_name}`.
- Free function symbol names are inferred as `{symbol_prefix}__{fn_name}`.
- Inherent method symbol names are inferred as `{symbol_prefix}__{SelfTy}__{method}`.
- Trait method symbol names are inferred as `{symbol_prefix}__{TraitPath}__{SelfTy}__{method}`.
- `#![symbol_prefix = "..."]` defines the symbol prefix (defaults to `CARGO_CRATE_NAME`).
- `#[symbol_name = "..."]` overrides the name mangling enforced by the `ffi` macro.
- `#![symbol_fragments(Ty = "frag", ...)]` declares symbol interpolation values for types used to concretize parameters.
- `#![failure = "panic" | "error"]` controls whether internal failures panic(default) or are returned.
- `type Type;` declares an opaque type (it's representation is unknown). This type should not be dereferenced.
- `#[tag(TagTy)]` on a type declaration defines its tag type; `#[tag(TagTy, unsafe(val))]` also assigns its tag value.
- `where use<T, ...> @ (<Type1> | ...)` opts into a kind of polymorphic dispatch where concrete types are known at compile time but erased at runtime.
- `#[unpack(_, _)]` on an imported function argument unpacks the compound type into two funcion arguments (facilitates useing `&[T]` in legacy APIs).
- `raw fn name(...) -> RetTy;` import declarations generate a C-compatible companion function named `name_raw`
- Using the `ffi` macro always carries a risk of UB as it relies on the correct user-provided argument types and lifetimes in the ABI.

### 2.2. `#[derive(ReprC)]`

`#[derive(ReprC)]` derives implementations required to convert a type to a corresponding generated C-compatible companion type.
A C-compatible companion type is a type with a defined C ABI and no trap representations, whose fields are themselves C-compatible companion types.

- By default, the derive defines a C-compatible companion type and conversions between the two types.
- `#[reprC(identity)]` uses a `#[repr(C)]` or `#[repr(transparent)]` struct directly as its companion.
- Conversion of types with explicit representation (i.e. `#[repr(C)]`/`repr(transmute)`) are optimized.
- `#[reprC(is_valid = |field0, ...| {...})]` provides additional validity invariant of a struct/variant.
- `#[reprC(NICHE_VALUE = <expr>)]` defines the struct's trap value that is used for niche optimization.

## 3. Tagged Dispatch

Tagged dispatch exposes one ABI function for multiple concrete Rust instantiations. At runtime, tags select the concrete instantiation to invoke.

### 3.1. Runtime-dispatched parameters

- `dyn(TagTy) T` declares a runtime tag-dispatched type parameter with a tag of type `TagTy`.
- `dyn(TagTy) T = ErasedTy` additionally declares the shared ABI representation `T` is erased to.
- A tag-dispatched parameter without an erased representation must always occur behind an indirection.

### 3.2 `dyn Self` dispatch

`impl [Trait for] dyn Self` dispatches an extern/opaque type:

- It is supported only for types declared inside the `ffi!` block scope.
- The type declaration **MUST** provide `#[tag(TagTy, unsafe(Value))]`.
- `dyn Self` tag-dispatched type must always occur behind an indirection.

### 3.3. `#[derive(Tag)]`
`#[derive(Tag)]` derives an implementation of `TagFamily` and, if a value is given, an implementation of the `Tagged` trait.

- `#[tag(TagType, unsafe(value))]` defines the tag that identifies the item when it is tag-dispatched.
- `#[tag(TagType)]` defines the tag type without assigning a tag value; `Tagged` trait is implemented by hand.
- Reusing a tag value for different types can cause an invalid type reinterpretation and is therefore `unsafe`.

### 3.4 Tags

- Static parameters, whether selected by a `where use<...>` predicate or not, **MUST NOT** inject a tag.
- Every runtime tag-dispatched parameter **MUST** inject exactly one tag as an ABI function argument.
- By default, tag-dispatched parameters synthesize tags at the function's start, in declaration order.
- In import declarations, a tag argument **MAY** be written explicitly in any position as `<dyn T>::TAG`.
- An explicit tag **MUST** refer to a declared runtime tag-dispatched parameter or the active `dyn Self`.

## 4. Parameter constraining
Every non-lifetime generic parameter **MUST** be constrained by a direct concrete selection (`where use<T, ...> @ (<Ty1> | ...)`), subject to the import exception below:

- If directly constrained, every generic parameter **MUST** be constrained in exactly one `use` predicate group.
- Exported runtime tag-dispatched generic type parameters **MUST** be directly constrained to concrete selections.
- An imported static generic parameter **CAN** remain unconstrained when its concrete type does not affect the ABI.

## 5. Parameter symbol interpolation

Symbol interpolation generates a unique function symbol for each combination of concrete parameter selections:

- Only a function-level static generic parameter constrained in a `use` predicate group **CAN** be interpolated.
- Every static generic parameter **MUST** be interpolated unless used exclusively within a runtime-dispatched type.
- If given, function symbol name **MUST** interpolate every eligible static generic parameter exactly once.
