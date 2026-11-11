use co3::{decoded::SoftDecodeRef, reference::CSoftRef, stored::Store, utf8::Utf8String, vec::CVec};

#[test]
fn shared_string_decodes_without_taking_source_ownership() {
    for text in ["", "héllo 🦀"] {
        let mut original = String::with_capacity(64);
        original.push_str(text);
        let source = Utf8String::from_string(original);
        let mut store = Default::default();
        let decoded =
            unsafe { String::soft_decode_ref(CSoftRef::from_ref(&source), &mut store) }.unwrap();
        assert_eq!(decoded, text);
        assert_eq!(decoded.capacity(), source.capacity());
        assert_ne!(decoded.as_ptr(), source.as_ptr());
        assert_eq!(store.sync(), Some(()));
        // The source allocation still belongs to its original owner.
        let original = unsafe { source.0.into_vec() }.unwrap();
        assert_eq!(original, text.as_bytes());
    }
}

#[test]
fn shared_string_rejects_invalid_utf8_without_consuming_source() {
    let source = Utf8String(CVec::from_vec(vec![0xff, 0xfe]));
    let mut store = Default::default();
    assert!(unsafe { String::soft_decode_ref(CSoftRef::from_ref(&source), &mut store) }.is_none());
    drop(store);
    let original = unsafe { source.0.into_vec() }.unwrap();
    assert_eq!(original, [0xff, 0xfe]);
}

#[test]
fn shared_string_validates_carrier_headers() {
    let mut store = Default::default();
    assert!(
        unsafe { String::soft_decode_ref(CSoftRef::from_raw(core::ptr::null()), &mut store) }.is_none()
    );
    let null = Utf8String::null();
    assert!(unsafe { String::soft_decode_ref(CSoftRef::from_ref(&null), &mut store) }.is_none());
    let bytes = [b'a'];
    let invalid = Utf8String(unsafe { CVec::from_raw_parts(bytes.as_ptr().cast_mut(), 2, 1) });
    assert!(unsafe { String::soft_decode_ref(CSoftRef::from_ref(&invalid), &mut store) }.is_none());
}

#[test]
fn shared_string_unchecked_decodes_without_taking_source_ownership() {
    let source = Utf8String::from_string(String::from("héllo"));
    let mut store = Default::default();
    let decoded = unsafe { String::soft_decode_ref_unchecked(CSoftRef::from_ref(&source), &mut store) };
    assert_eq!(decoded, "héllo");
    assert_ne!(decoded.as_ptr(), source.as_ptr());
    drop(store);
    assert_eq!(unsafe { source.0.into_vec() }.unwrap(), "héllo".as_bytes());
}

type Text = String;
fn string_len(input: &String) -> usize {
    input.len()
}
fn alias_len(input: &Text) -> usize {
    input.len()
}
co3::raw! {
    fn string_len(#[soft] input: &String) -> usize;
    fn alias_len(#[soft] input: &Text) -> usize;
}

#[test]
fn exported_shared_string_and_alias_use_the_decoder() {
    let source = Utf8String::from_string(String::from("héllo"));
    assert_eq!(
        unsafe { string_len_raw(CSoftRef::from_ref(&source)) },
        "héllo".len()
    );
    assert_eq!(
        unsafe { alias_len_raw(CSoftRef::from_ref(&source)) },
        "héllo".len()
    );
    assert_eq!(unsafe { source.0.into_vec() }.unwrap(), "héllo".as_bytes());
}
