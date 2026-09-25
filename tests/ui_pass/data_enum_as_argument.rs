//! A data-carrying enum must be usable as an exported function argument.
//!
//! Decoding an owned argument requires `EncodeOwned::Store: EmptyStore`. The
//! store of a data-carrying enum is an `EitherN`, one slot per variant, so
//! `EitherN` has to forward `EmptyStore` the way `Result` and the tuples
//! already do. Without that impl the `ffi!` block below fails to resolve
//! `Either2<(), ((), (), (), (), ())>: EmptyStore`, even though both of its
//! constituents are empty stores.
//!
//! The enum here has one unit variant and one struct variant carrying
//! borrowed data, which is the shape that motivated the impl: configuration
//! that belongs to exactly one of several backends.

use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
pub enum Backend<'a> {
    Managed,
    Manual {
        conf_dir: Option<&'a str>,
        country: Option<&'a str>,
        timeout_secs: u32,
    },
}

pub fn timeout_of(backend: Backend<'_>) -> u32 {
    match backend {
        Backend::Managed => 0,
        Backend::Manual { timeout_secs, .. } => timeout_secs,
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "data_enum_as_argument"]

    fn timeout_of(backend: Backend<'_>) -> u32;
}

fn main() {}
