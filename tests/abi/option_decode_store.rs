use co3::{
    option::COption,
    primitives::CBool,
    reference::CRefMut,
    stored::{DecodeMove, Store},
};

#[test]
fn decoding_none_has_no_active_store() {
    let mut untagged = Default::default();
    let decoded =
        unsafe { <Option<u8> as DecodeMove>::soft_decode(COption::Some(7), &mut untagged) };
    assert!(matches!(decoded, Some(Some(7))));
    assert!(untagged.is_some());

    let decoded =
        unsafe { <Option<u8> as DecodeMove>::soft_decode(COption::None(), &mut untagged) };
    assert!(matches!(decoded, Some(None)));
    assert!(untagged.is_none());
    assert_eq!(untagged.sync(), Some(()));

    let mut niche = Default::default();
    let mut source = CBool::TRUE;
    let encoded = unsafe { CRefMut::<CBool, true>::from_raw(&mut source) };
    let decoded = unsafe { <Option<&mut bool> as DecodeMove>::soft_decode(encoded, &mut niche) };
    assert!(matches!(decoded, Some(Some(true))));
    assert!(niche.is_some());

    let decoded = unsafe {
        <Option<&mut bool> as DecodeMove>::soft_decode(CRefMut::<CBool, true>::null(), &mut niche)
    };
    assert!(matches!(decoded, Some(None)));
    assert!(niche.is_none());
    assert_eq!(niche.sync(), Some(()));
}
