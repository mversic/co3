use co3::{option::COption, primitives::CBool, reference::CRefMut, stored::Store};

#[test]
fn decoding_none_has_no_active_store() {
    let mut untagged = Default::default();
    let decoded = unsafe { co3::soft_decode::<Option<u8>>(COption::Some(7), &mut untagged) };
    assert!(matches!(decoded, Some(Some(7))));
    assert!(untagged.is_some());

    let decoded = unsafe { co3::soft_decode::<Option<u8>>(COption::None(), &mut untagged) };
    assert!(matches!(decoded, Some(None)));
    assert!(untagged.is_none());
    assert_eq!(untagged.sync(), Some(()));

    let mut niche = Default::default();
    let mut source = CBool::TRUE;
    let encoded = unsafe { CRefMut::<bool, true>::from_raw(&mut source) };
    let decoded = unsafe {
        co3::soft_decode::<Option<&mut bool>>(co3::option::COptionSome::new(encoded), &mut niche)
    };
    assert!(matches!(decoded, Some(Some(true))));
    assert!(niche.is_some());

    let decoded = unsafe {
        co3::soft_decode::<Option<&mut bool>>(
            co3::option::COptionSome::new(CRefMut::<bool, true>::null()),
            &mut niche,
        )
    };
    assert!(matches!(decoded, Some(None)));
    assert!(niche.is_none());
    assert_eq!(niche.sync(), Some(()));
}
