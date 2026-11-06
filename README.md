# CO3

[<img alt="crates.io" src="https://img.shields.io/crates/v/co3.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/co3)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-co3-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/co3)
[<img alt="CI" src="https://img.shields.io/github/actions/workflow/status/mversic/co3/main.yaml?style=for-the-badge&label=CI" height="20">](https://github.com/mversic/co3/actions/workflows/main.yaml)

Cross-language Rust interop through a stable ABI

1. **Native-Rust ergonomics**
   - Ergonomics of APIs and generated wrappers are idiomatic to Rust users.
   - Language boundary mechanics follow the zero-cost abstraction principle.
   - **No custom conversion code** should be required beyond `CO3` declarations.
   - High-level Rust features, such as generics, and custom DSTs, are supported.

2. **Soundness-first design**
   - By default, **soundness is never relaxed** for performance or memory usage.
   - If preserving soundness requires validation or cloning, that cost is accepted.
   - Explicit opt-in performace modes delegate soundness responsibility to the user.
   - Control over **ownership transfer** across the FFI boundary **is made explicit**.

Check the [release article](https://mversic.github.io/co3/) for the motivation behind `CO3` and in-depth analysis of its features.

## ABI Stability

Although this crate is pre-1.0.0, its **ABI is considered stable**. This does not imply the API (aside from declaration syntax) of this crate is stable. In practice:

- **ABI stability** means FFI contracts (symbol names, calling conventions, and data layout expectations) are intended to remain compatible across updates.
- **API instability** means Rust-facing items (fns, trait shapes, modules, and type signatures) may still change and require source updates when upgrading.

In other words, external binaries that integrate through the defined ABI should keep working, while Rust code using this crate directly may need refactoring between releases.

## Exports

Typically, the crate is developed without concern for FFI, with export declarations added separately when ready. To expose its API:
- **mark Rust types** that cross the boundary with `#[derive(ReprC)]`
- describe the boundary API with `ffi!` (fns, impls and types)

_Opaque types_ allow implementation details to remain on the exporting side, as with `Camera` below:

```rust
use co3::{ffi, rust_spec::RustSpec, ReprC};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct Vec2 {
    x: f32,
    y: f32,
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Camera {
    position: Vec2,
}

impl Camera {
    fn translate(&mut self, by: Vec2) {
        self.position.x += by.x;
        self.position.y += by.y;
    }

    fn position(&self) -> Vec2 {
        self.position
    }
}

fn distance(from: Vec2, to: Vec2) -> f32 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    (dx * dx + dy * dy).sqrt()
}

ffi! {
    // Declares exports of existing Rust items (fns, impls, types)
    // Change to `#![unsafe(extern("C"))]` to declare imports
    #![unsafe(export("C"))]

    // Define the prefix of FFI symbols
    // The default is `CARGO_CRATE_NAME`
    #![symbol_prefix = "geometry"]

    // Opaque type
    type Camera;

    impl Camera {
        // Exported as `translate`
        #[symbol_name = "translate"]
        fn translate(&mut self, by: Vec2);

        // Exported as `geometry__Camera__position`
        fn position(&self) -> Vec2;
    }

    // Exported as `geometry__distance`
    fn distance(from: Vec2, to: Vec2) -> f32;
}
```

Types deriving `ReprC` are **not required to have a stable representation**, but a stable representation enables optimized conversions.

## Imports

**Export declarations are fully interchangeable as import declarations**, providing the Rust client API alongside the `cdylib`:
* `CO3` strives to **keep the API uniform** whether crate is linked as an `rlib` or a `cdylib`.
* Switch between static and dynamic linking should require as few changes as possible.

In the previous example just change `#![unsafe(export("C"))]` to `#![unsafe(extern("C"))]` to use the declarations as imports. As simple as that.

## Tagged Dispatch

Beyond native Rust features, `CO3` provides **tagged dispatch** to customize the boundary ABI.

It is common in FFI for several concrete types to share one C representation. Think of FFI functions
like [SQLAllocHandle](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlallochandle-function)
which works for different tag types
or [`SQLSetEnvAttr`](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlsetenvattr-function)
where an attribute's concrete type determines the accepted value representation.

```rust
use co3::{ffi, rust_spec::RustSpec, Tag, ReprC};

trait Calibrate {
    fn calibrate(&mut self, by: u16);
}

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct Celsius(u16);

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(2))]
#[repr(transparent)]
struct Fahrenheit(u16);

impl Calibrate for Celsius {
    fn calibrate(&mut self, by: u16) {
        self.0 += by;
    }
}

impl Calibrate for Fahrenheit {
    fn calibrate(&mut self, by: u16) {
        self.0 += by;
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "thermometer"]

    // - T is declared as tag-dispatched
    // - u8 is the type of the tag argument
    // - every concrete `T` is erased as `u16`
    impl<dyn(u8) T = u16> Calibrate for T
    where
        // `T` is instantiated as either
        use<T> @ (<Celsius> | <Fahrenheit>),
    {
        fn calibrate(&mut self, by: u16);
    }
}
```

To see what `CO3` is capable of and what using it looks like in practice, check out [rs-odbc](https://github.com/mversic/rs-odbc).

## Related Projects

### ABI-stable Rust-to-Rust interop

* [abi_stable](https://docs.rs/abi_stable/latest/abi_stable/)
* [stabby](https://github.com/ZettaScaleLabs/stabby)

Define stable interfaces between separately compiled Rust components, with ABI-stable types, dynamic library loading, and compatibility checks.

### Cross-language interface generators

* [safer_ffi](https://github.com/getditto/safer_ffi)
* [Diplomat](https://github.com/rust-diplomat/diplomat)
* [Interoptopus](https://github.com/ralfbiedert/interoptopus)
* [BoltFFI](https://github.com/boltffi/boltffi)
* [UniFFI](https://github.com/mozilla/uniffi-rs)

These projects primarily **expose Rust-defined interfaces to other languages**.

`Interoptopus` also provides plugin imports with dynamic library loading and interface compatibility checks, generating Rust wrappers for foreign implementations of a declared interface, though this feature remains experimental.

`CO3` treats imports as interchangeable with exports, supporting both existing foreign APIs and Rust-defined interfaces.

### Importing C APIs into Rust

* [bindgen](https://github.com/rust-lang/rust-bindgen)

Generates **`Rust` bindings from `C` headers**.

Generated bindings still require manually written wrappers to express ownership, lifetimes, and other invariants that C headers leave implicit.
Considering this, the effort of using `bindgen` can be **comparable to writing import declarations directly with `CO3`**.

### Generating C headers

* [cbindgen](https://github.com/mozilla/cbindgen)
* [cheadergen](https://docs.rs/cheadergen/latest/cheadergen/)

Generate **C headers from Rust code**.

With some adjustments, they **could be used to generate headers from `CO3`** export declarations.
