#![cfg(feature = "derive")]

use core::cmp::Reverse;

use co3::{
    ReprC, convert::Store, decode, decode_unchecked, encode, encode_unchecked, lang,
    rust_spec::RustSpec, soft_encode,
};

#[derive(Debug, PartialEq, RustSpec, ReprC)]
#[repr(C)]
struct Record {
    number: u32,
    flag: bool,
}

#[derive(Debug, PartialEq, RustSpec, ReprC)]
#[repr(transparent)]
struct Wrapper<T>(T);

#[derive(Clone, Copy, Debug, PartialEq, RustSpec, ReprC)]
#[repr(u8)]
enum Choice {
    Number(u32) = 1,
    Flag(bool) = 2,
}

#[test]
fn derived_values_decode_with_specified_layouts() {
    let value = Record {
        number: 7,
        flag: true,
    };
    let carrier = unsafe { encode::<_, lang::C>(value) };
    assert_eq!(
        unsafe { decode::<Record, lang::C>(carrier) },
        Some(Record {
            number: 7,
            flag: true
        }),
    );

    let carrier = unsafe { encode_unchecked(Wrapper(true)) };
    assert_eq!(
        unsafe { decode_unchecked::<Wrapper<bool>>(carrier) },
        Wrapper(true)
    );

    for value in [Choice::Number(7), Choice::Flag(true)] {
        let carrier = unsafe { encode::<_, lang::C>(value) };
        assert_eq!(unsafe { decode::<Choice, lang::C>(carrier) }, Some(value));
    }
}

#[test]
fn stable_shared_references_decode_directly() {
    let value = Record {
        number: 7,
        flag: true,
    };
    let carrier = unsafe { encode::<_, lang::C>(&value) };
    let decoded = unsafe { decode::<&Record, lang::C>(carrier) }.unwrap();
    assert!(core::ptr::eq(decoded, &value));

    let value = Reverse(7_u32);
    let carrier = unsafe { encode::<_, lang::C>(&value) };
    let decoded = unsafe { decode::<&Reverse<u32>, lang::C>(carrier) }.unwrap();
    assert!(core::ptr::eq(decoded, &value));
}

#[test]
fn stable_nonrobust_mutable_references_decode_directly() {
    let mut value = false;
    let carrier = unsafe { encode_unchecked(&mut value) };
    let decoded = unsafe { decode::<&mut bool, lang::C>(carrier) }.unwrap();
    *decoded = true;
    assert!(value);

    let mut value = Wrapper(false);
    let carrier = unsafe { encode_unchecked(&mut value) };
    let decoded = unsafe { decode::<&mut Wrapper<bool>, lang::C>(carrier) }.unwrap();
    decoded.0 = true;
    assert!(value.0);
}

#[test]
fn stable_mutable_bool_encoding_validates_writeback() {
    let mut value = false;
    {
        let mut store = Default::default();
        let carrier = unsafe { soft_encode::<_, lang::C>(&mut value, &mut store) };
        unsafe { *carrier.as_ptr() = encode::<_, lang::C>(true) };
        assert_eq!(store.sync(), Some(()));
    }
    assert!(value);

    {
        let mut store = Default::default();
        let carrier = unsafe { soft_encode::<_, lang::C>(&mut value, &mut store) };
        // The foreign side can write any byte; checked writeback rejects invalid Rust bools.
        unsafe { carrier.as_ptr().cast::<u8>().write(2) };
        assert_eq!(store.sync(), None);
    }
    assert!(value);
}

#[test]
fn transparent_mutable_encoding_keeps_stable_writeback() {
    let mut value = Wrapper(false);
    {
        let mut store = Default::default();
        let carrier = unsafe { soft_encode::<_, lang::C>(&mut value, &mut store) };
        unsafe { *carrier.as_ptr() = encode::<_, lang::C>(Wrapper(true)) };
        assert_eq!(store.sync(), Some(()));
    }
    assert!(value.0);
}

#[cfg(feature = "alloc")]
#[test]
fn stable_vector_mutable_encoding_keeps_writeback() {
    let mut value: co3::vec::Vec<bool> = [false].into_iter().collect();
    {
        let mut store = Default::default();
        let carrier = unsafe { soft_encode::<_, lang::C>(&mut value, &mut store) };
        let view = unsafe { decode::<&mut co3::vec::Vec<bool>, lang::C>(carrier) }.unwrap();
        view[0] = true;
        assert_eq!(store.sync(), Some(()));
    }
    assert!(value[0]);
}
