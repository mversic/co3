use co3::{option::abi, reference::SoftRefMut};

#[test]
fn decoding_none_has_no_active_store() {
    let mut untagged = Default::default();
    let decoded = unsafe { co3::soft_decode::<Option<u8>>(abi::Option::Some(7), &mut untagged) };
    assert!(matches!(decoded, Some(Some(7))));
    assert!(untagged.is_some());

    let decoded = unsafe { co3::soft_decode::<Option<u8>>(abi::Option::None(), &mut untagged) };
    assert!(matches!(decoded, Some(None)));
    assert!(untagged.is_none());
    assert_eq!(co3::convert::Store::sync(untagged), Some(()));

    let mut niche = Default::default();
    let mut source = co3::primitive::abi::bool::TRUE;
    let encoded = unsafe { SoftRefMut::<<bool as co3::ReprC>::CType>::from_raw(&mut source) };
    let decoded = unsafe {
        co3::soft_decode::<Option<&mut bool>>(co3::option::abi::OptionSome(encoded), &mut niche)
    };
    assert!(matches!(decoded, Some(Some(true))));
    assert!(niche.is_some());

    let decoded = unsafe {
        co3::soft_decode::<Option<&mut bool>>(
            co3::option::abi::OptionSome(SoftRefMut::<<bool as co3::ReprC>::CType>::null()),
            &mut niche,
        )
    };
    assert!(matches!(decoded, Some(None)));
    assert!(niche.is_none());
    assert_eq!(co3::convert::Store::sync(niche), Some(()));
}
