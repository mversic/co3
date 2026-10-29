#![cfg(feature = "alloc")]

use co3::stored::{
    DecodeMove, EncodeMove, EncodeMoveUnchecked, EncodeMut, EncodeRef, EncodeRefUnchecked, Store,
};

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
            let encoded = values.encode_ref(&mut store);
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!(header.len(), values.len());
            assert_eq!(header.capacity(), values.capacity());
            assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
            store.sync().unwrap();
            let mut store = Default::default();
            let encoded = unsafe { values.encode_ref_unchecked(&mut store) };
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
            let encoded = unsafe { values.encode_ref_unchecked(&mut store) };
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!((header.len(), header.capacity()), (2, capacity));
            store.sync().unwrap();
            let mut store = Default::default();
            let encoded = unsafe { values.soft_encode_unchecked(&mut store) };
            assert_eq!((encoded.len(), encoded.capacity()), (2, capacity));
            let mut decode_store = Default::default();
            let decoded = unsafe {
                co3::vec::Vec::<(u8,)>::soft_decode_unchecked(encoded, &mut decode_store)
            };
            assert_eq!(decoded.capacity(), capacity);
            assert_eq!(&*decoded, &[(3,), (7,)]);
            decode_store.sync().unwrap();
            store.sync().unwrap();
        } else {
            let mut store = Default::default();
            let encoded = values.encode_ref(&mut store);
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!((header.len(), header.capacity()), (2, capacity));
            store.sync().unwrap();
            let mut store = Default::default();
            let encoded = values.soft_encode(&mut store);
            assert_eq!((encoded.len(), encoded.capacity()), (2, capacity));
            let mut decode_store = Default::default();
            let decoded =
                unsafe { co3::vec::Vec::<(u8,)>::soft_decode(encoded, &mut decode_store) }.unwrap();
            assert_eq!(decoded.capacity(), capacity);
            assert_eq!(&*decoded, &[(3,), (7,)]);
            decode_store.sync().unwrap();
            store.sync().unwrap();
        }
    }
}

#[test]
fn mutable_checked_conversion_preserves_capacity_and_writes_back_length() {
    let mut values = Vec::with_capacity(17);
    values.extend([true, false]);
    let mut values: co3::vec::Vec<_> = values.into();
    let capacity = values.capacity();
    let mut store = Default::default();
    let encoded = values.encode_mut(&mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_eq!((header.len(), header.capacity()), (2, capacity));
    // The carrier owns the converted allocation; simulate a foreign pop.
    unsafe {
        *encoded.as_ptr() = co3::vec::CVec::from_raw_parts(header.as_ptr(), 1, header.capacity())
    };
    store.sync().unwrap();
    assert_eq!(values.capacity(), capacity);
    assert_eq!(&*values, &[true]);
}

#[test]
fn empty_and_zst_shared_vectors_preserve_headers() {
    macro_rules! check {
        ($values:expr) => {{
            let values: co3::vec::Vec<_> = $values.into();
            let mut store = Default::default();
            let encoded = values.encode_ref(&mut store);
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
