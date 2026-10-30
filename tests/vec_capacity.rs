#![cfg(feature = "alloc")]

use co3::stored::Store;

#[test]
fn shared_headers_preserve_spare_capacity() {
    let mut native = Vec::with_capacity(19);
    native.extend([3u8, 7]);
    let mut stable = Vec::with_capacity(19);
    stable.extend([3u8, 7]);
    let stable: co3::vec::Vec<_> = stable.into();
    macro_rules! check {
        ($values:expr) => {{
            let values = &$values;
            let mut store = Default::default();
            let encoded = co3::soft_encode(values, &mut store);
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!(header.len(), values.len());
            assert_eq!(header.capacity(), values.capacity());
            assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
            store.sync().unwrap();
            let mut store = Default::default();
            let encoded = unsafe { co3::soft_encode_unchecked(values, &mut store) };
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!(header.capacity(), values.capacity());
            store.sync().unwrap();
        }};
    }
    check!(native);
    check!(stable);
}

#[test]
fn converted_vectors_preserve_spare_capacity() {
    for unchecked in [false, true] {
        let mut values = Vec::with_capacity(23);
        values.extend([(3u8,), (7,)]);
        let values: co3::vec::Vec<_> = values.into();
        let capacity = values.capacity();
        if unchecked {
            let mut store = Default::default();
            let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!((header.len(), header.capacity()), (2, capacity));
            store.sync().unwrap();
            let mut store = Default::default();
            let encoded = unsafe { co3::soft_encode_unchecked(values, &mut store) };
            assert_eq!((encoded.len(), encoded.capacity()), (2, capacity));
            let mut decode_store = Default::default();
            let decoded = unsafe {
                co3::soft_decode_unchecked::<co3::vec::Vec<(u8,)>>(encoded, &mut decode_store)
            };
            assert_eq!(decoded.capacity(), capacity);
            assert_eq!(&*decoded, &[(3,), (7,)]);
            decode_store.sync().unwrap();
            store.sync().unwrap();
        } else {
            let mut store = Default::default();
            let encoded = co3::soft_encode(&values, &mut store);
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!((header.len(), header.capacity()), (2, capacity));
            store.sync().unwrap();
            let mut store = Default::default();
            let encoded = co3::soft_encode(values, &mut store);
            assert_eq!((encoded.len(), encoded.capacity()), (2, capacity));
            let mut decode_store = Default::default();
            let decoded =
                unsafe { co3::soft_decode::<co3::vec::Vec<(u8,)>>(encoded, &mut decode_store) }
                    .unwrap();
            assert_eq!(decoded.capacity(), capacity);
            assert_eq!(&*decoded, &[(3,), (7,)]);
            decode_store.sync().unwrap();
            store.sync().unwrap();
        }
    }
}

#[test]
fn mutable_checked_conversion_rejects_length_changes() {
    let mut values = Vec::with_capacity(17);
    values.extend([true, false]);
    let mut values: co3::vec::Vec<_> = values.into();
    let capacity = values.capacity();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_eq!((header.len(), header.capacity()), (2, capacity));
    // The store retains the allocation and rejects foreign header changes.
    unsafe {
        *encoded.as_ptr() = co3::vec::CVec::from_raw_parts(header.as_ptr(), 1, header.capacity())
    };
    assert_eq!(store.sync(), None);
    assert_eq!(values.capacity(), capacity);
    assert_eq!(&*values, &[true, false]);
}

#[test]
fn empty_and_zst_shared_vectors_preserve_headers() {
    macro_rules! check {
        ($values:expr) => {{
            let values: co3::vec::Vec<_> = $values.into();
            let mut store = Default::default();
            let encoded = co3::soft_encode(&values, &mut store);
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!(
                (header.len(), header.capacity()),
                (values.len(), values.capacity())
            );
            store.sync().unwrap();
        }};
    }
    check!(Vec::<u8>::with_capacity(29));
    check!(vec![(); 3]);
    check!(Vec::<(u8,)>::with_capacity(29));
}
