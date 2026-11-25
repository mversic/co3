use co3::{ReprC, rust_spec::RustSpec};

type L = u8;
type Flag<'a> = &'a mut bool;

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct Store(L);

mod export {
    use co3::ffi;

    use super::{Flag, L, Store};

    #[unsafe(export_name = "rust_language__LEVEL")]
    static LEVEL: L = 3;

    fn first(values: Vec<Store>) -> L {
        values[0].0
    }

    fn flag(value: Flag<'_>) -> Flag<'_> {
        value
    }

    ffi! {
        #![unsafe(export("C"))]
        #![lang = "Rust"]
        #![symbol_prefix = "rust_language"]
        static LEVEL: L;
        fn first(#[soft] values: Vec<Store>) -> L;
        fn flag(value: Flag<'_>) -> Flag<'_>;
    }
}

mod import {
    use co3::ffi;

    use super::{Flag, L, Store};

    ffi! {
        #![unsafe(extern("C"))]
        #![lang = "Rust"]
        #![symbol_prefix = "rust_language"]
        pub(super) static LEVEL: L;
        pub(super) fn first(#[soft] values: Vec<Store>) -> L;
        pub(super) fn flag(value: Flag<'_>) -> Flag<'_>;
    }
}

#[test]
fn rust_boundary_uses_unchecked_codecs_for_soft_arguments_returns_and_statics() {
    assert_eq!(import::first(vec![Store(7)]), 7);
    assert_eq!(import::LEVEL.get(), Some(&3));
    assert_eq!(import::LEVEL.read(), Some(3));

    let mut value = false;
    *import::flag(&mut value) = true;
    assert!(value);
}

#[test]
fn codec_names_in_source_keep_their_meaning() {
    type __Co3Lang = u8;
    mod domain {
        pub type Decode = bool;
    }

    #[derive(Debug, PartialEq, RustSpec, ReprC)]
    #[repr(C)]
    struct Record {
        store: __Co3Lang,
        owner: domain::Decode,
    }

    let record = || Record {
        store: 7,
        owner: true,
    };
    let mut owner = Default::default();
    let carrier = unsafe { co3::borrow::encode::<_, co3::lang::Cpp>(record(), &mut owner) };
    assert_eq!(
        unsafe { co3::borrow::decode::<_, co3::lang::Cpp>(carrier) },
        Some(record())
    );
    let carrier = unsafe { co3::encode_unchecked(record()) };
    assert_eq!(
        unsafe { co3::decode_unchecked::<Record>(carrier) },
        record()
    );
}
