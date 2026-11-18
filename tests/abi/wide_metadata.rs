use core::cell::Cell;

use co3::{
    ReprC, ffi,
    ops::CFn3,
    pack::{Pack2, Unpack2},
    rust_spec::RustSpec,
};

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(identity)]
struct Bytes([u8]);

mod exported {
    use super::*;

    fn shared(value: &Bytes, expected: *const u8) -> usize {
        assert_eq!(value.0.as_ptr(), expected);
        assert_eq!(&value.0, &[1, 2, 3]);
        value.0.len()
    }

    fn mutable(value: &mut Bytes, expected: *mut u8) -> usize {
        assert_eq!(value.0.as_mut_ptr(), expected);
        assert_eq!(&value.0, &[1, 2, 3]);
        value.0[0] = 9;
        value.0.len()
    }

    fn owned(value: Box<[u8]>, expected: *const u8) -> Box<[u8]> {
        assert_eq!(value.as_ptr(), expected);
        value
    }

    fn cells(value: Box<[Cell<u8>]>, expected: *const Cell<u8>) -> Box<[Cell<u8>]> {
        assert_eq!(value.as_ptr(), expected);
        value
    }

    co3::raw! {
        fn shared(#[unpack(*const u8, usize)] value: &Bytes, expected: *const u8) -> usize;
        fn mutable(#[unpack(*mut u8, usize)] value: &mut Bytes, expected: *mut u8) -> usize;
        fn owned(#[unpack(_, _)] value: move Box<[u8]>, expected: *const u8) -> move Box<[u8]>;
        fn cells(#[unpack(_, _)] value: move Box<[Cell<u8>]>, expected: *const Cell<u8>) -> move Box<[Cell<u8>]>;
    }

    ffi! {
        #![unsafe(export("C"))]

        #[symbol_name = "wide_metadata__shared"]
        unsafe extern "C" fn shared_raw(data: *const u8, len: usize, expected: *const u8) -> usize;
        #[symbol_name = "wide_metadata__mutable"]
        unsafe extern "C" fn mutable_raw(data: *mut u8, len: usize, expected: *mut u8) -> usize;
        #[symbol_name = "wide_metadata__owned"]
        unsafe extern "C" fn owned_raw(data: *mut u8, len: usize, expected: *const u8) -> <Box<[u8]> as ReprC>::CType;
        #[symbol_name = "wide_metadata__cells"]
        unsafe extern "C" fn cells_raw(data: *mut Cell<u8>, len: usize, expected: *const Cell<u8>) -> <Box<[Cell<u8>]> as ReprC>::CType;
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "wide_metadata"]

    fn shared(#[unpack(*const u8, usize)] value: &Bytes, expected: *const u8) -> usize;
    fn mutable(#[unpack(*mut u8, usize)] value: &mut Bytes, expected: *mut u8) -> usize;
    fn owned(#[unpack(_, _)] value: move Box<[u8]>, expected: *const u8) -> move Box<[u8]>;
    fn cells(#[unpack(_, _)] value: move Box<[Cell<u8>]>, expected: *const Cell<u8>) -> move Box<[Cell<u8>]>;
}

mod raw {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "wide_metadata"]

        pub(super) raw fn shared(#[unpack(*const u8, usize)] value: &Bytes, expected: *const u8) -> usize;
        pub(super) raw fn mutable(#[unpack(*mut u8, usize)] value: &mut Bytes, expected: *mut u8) -> usize;
    }
}

#[test]
fn wide_reference_unpack_preserves_pointer_length_and_mutation() {
    let mut bytes = [1_u8, 2, 3];
    let expected = bytes.as_ptr();
    let value = unsafe { &*(bytes.as_slice() as *const [u8] as *const Bytes) };
    assert_eq!(shared(value, expected), 3);

    let expected = bytes.as_mut_ptr();
    let value = unsafe { &mut *(bytes.as_mut_slice() as *mut [u8] as *mut Bytes) };
    assert_eq!(mutable(value, expected), 3);
    assert_eq!(bytes, [9, 2, 3]);
}

#[test]
fn wide_reference_pack_reconstructs_unpacked_arguments() {
    let mut bytes = [1_u8, 2, 3];
    let callback: unsafe extern "C" fn(_, _, _) -> _ = raw::shared;
    assert_eq!(
        unsafe { callback.call(bytes.as_ptr(), bytes.len(), bytes.as_ptr()) },
        Some(3usize)
    );
    let callback: unsafe extern "C" fn(_, _, _) -> _ = raw::mutable;
    assert_eq!(
        unsafe { callback.call(bytes.as_mut_ptr(), bytes.len(), bytes.as_mut_ptr()) },
        Some(3usize)
    );
    assert_eq!(bytes, [9, 2, 3]);
}

#[test]
fn owned_slice_unpack_round_trip_handles_both_mutability_kinds() {
    let values = vec![3_u8, 5].into_boxed_slice();
    let expected = values.as_ptr();
    let values = owned(values, expected);
    assert_eq!(values.as_ptr(), expected);
    assert_eq!(&*values, &[3, 5]);

    let values = vec![Cell::new(3_u8), Cell::new(5)].into_boxed_slice();
    let expected = values.as_ptr();
    let mut values = cells(values, expected);
    assert_eq!(values.as_ptr(), expected);
    assert_eq!(
        values
            .iter_mut()
            .map(|value| *value.get_mut())
            .collect::<Vec<_>>(),
        [3, 5]
    );

    // A vector's capacity cannot be represented by a pointer/length pair.
    static_assertions::assert_not_impl_any!(Vec<u8>:
        Unpack2<*mut u8, usize>, Pack2<*mut u8, usize>);
    static_assertions::assert_not_impl_any!(Vec<Cell<u8>>:
        Unpack2<*mut Cell<u8>, usize>, Pack2<*mut Cell<u8>, usize>);
}
