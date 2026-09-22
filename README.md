# CO3

[<img alt="crates.io" src="https://img.shields.io/crates/v/co3.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/co3)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-co3-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/co3)
[<img alt="CI" src="https://img.shields.io/github/actions/workflow/status/mversic/co3/main.yaml?style=for-the-badge&label=CI" height="20">](https://github.com/mversic/co3/actions/workflows/main.yaml)

_Safely_ export and/or import FFI bindings:

1. **Native-Rust ergonomics**

   - ergonomics of APIs and generated wrappers are idiomatic to Rust users.
   - FFI boundary mechanics are zero-cost abstracted yet remain configurable.
   - Except for statics, **export declarations can also be used for imports**.

2. **Soundness-first FFI interoperability**

   - **Soundness is never relaxed** for performance or memory footprint in the default configuration.
   - if preserving soundness requires additional validation, temporary storage, or cloning, that cost is accepted.
   - only explicit opt-in modes prioritize performance by explicitly shifting soundness responsibility to the user.

# Why Bother?

Ain't nobody got time for this, just give me a tldr:

1. You won't find anything quite as **ergonomic and expressive yet safe**

   - which crate allows using **native Rust types**? Those claiming so still require wrappers
   - while many claim expressivity, have you ever seen a **runtime tagged-dispatch** in Rust?

2. If you're exporting FFI from _Rust_, `CO3` makes it **unbelievably easy**

   - `CO3` aims to make the complex process of FFI generation completely painless
   - literally, just write down the export declarations as if writing native `Rust`

3. Although recent, `CO3` is already **incredibly feature rich and well tested**

   - the initial release of `CO3` required it reached feature parity with the ecosystem
   - which crate allows you to use **generics in FFI or custom DSTs**? yes, `CO3` does

Check the [release article](https://mversic.github.io/co3/) for the motivation behind `CO3` and in-depth analysis of its features.

# ABI Stability

Although this crate is pre-1.0.0, its **ABI is considered stable**. This does not mean the API is stable. In practice:

- **ABI stability** means FFI contracts (symbol names, calling conventions, and data layout expectations) are intended to remain compatible across updates.
- **API instability** means Rust-facing items (fns, trait shapes, modules, and type signatures) may still change and require source updates when upgrading.

In other words, external binaries that integrate through the defined ABI should keep working, while Rust code using this crate directly may need refactoring between releases.

# Example

Using `CO3` is super-duper **simple yet highly expressive**. In a nutshell:

- mark Rust types that cross the boundary with `#[derive(ReprC)]`
- describe the boundary API with `ffi!` (fns, impls and types)

```rust
use co3::{ffi, rust_spec::RustSpec, ReprC};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct Vec2 {
    x: f32,
    y: f32,
}

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

Note that type deriving `ReprC`, although recommended, **is not required to have a stable representation**.

## Tagged Dispatch

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

# Related Projects

## [cxx](https://github.com/dtolnay/cxx)

This crate generates a `C++` API and bridge code, with **compile-time checks** for supported declarations against `C++` headers.

If your FFI is `Rust <-> C++`, using this crate is preferable because it gives **deep cross-language integration**. As always, considering the author, this is a very well made crate. I think both crates could learn from each other.

## [bindgen](https://github.com/rust-lang/rust-bindgen)

This crate generates **`Rust` bindings from `C` headers**.

I maintain this is not a crate you would ever want to use because **`C` APIs communicate many invariants through informal ways** that automated translation into `Rust` bindings cannot capture.

Because `Rust`'s type system is much more expressive than `C`'s, it's much better to **write an authoritative layer in `Rust`**.

## [cbindgen](https://github.com/mozilla/cbindgen)

Use this crate to build **`C` headers from `CO3` export declarations**.

## [cheadergen](https://docs.rs/cheadergen/latest/cheadergen/)

The same as `cbindgen` but newer. **I'd prefer it** if it were at the same feature parity level.

## [safer_ffi](https://github.com/getditto/safer_ffi)

While I appreciate the work done, **this crate looks more like an attempt than a real solution**.

Given the existence of `CO3`, I fail to see why would anyone recommend using this crate. The API is too complicated while expressivity just isn't there.

If there is anything found missing in `CO3` that `safer-ffi` can do, report it and **the gap should be closed immediately**.

## [Diplomat](https://github.com/rust-diplomat/diplomat)

Generates not only C API glue for your `Rust` exports but also **bindings for supported target languages**.

I don't think `CO3` would aim to match that functionality, but I do think **they'd benefit a lot from building on top of `CO3`**.

## [Interoptopus](https://github.com/ralfbiedert/interoptopus)

Quite similar to `Diplomat` in scope.

Likewise, I believe this crate could build on top of `CO3`.
