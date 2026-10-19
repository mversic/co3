use co3::{
    option::ReprCOption,
    primitives::CBool,
    reference::CRefMut,
    stored::{DecodeOwned, Store},
};

#[test]
fn decoding_none_has_no_active_store() {
    let mut untagged = <Option<u8> as DecodeOwned>::Store::default();
    let decoded =
        unsafe { <Option<u8> as DecodeOwned>::soft_decode(ReprCOption::Some(7), &mut untagged) };
    assert!(matches!(decoded, Some(Some(7))));
    assert!(untagged.is_some());

    let decoded =
        unsafe { <Option<u8> as DecodeOwned>::soft_decode(ReprCOption::None(), &mut untagged) };
    assert!(matches!(decoded, Some(None)));
    assert!(untagged.is_none());
    assert_eq!(untagged.sync(), Some(()));

    let mut niche = <Option<&mut bool> as DecodeOwned>::Store::default();
    let mut source = CBool::TRUE;
    let encoded = unsafe { CRefMut::<CBool, true>::from_raw(&mut source) };
    let decoded = unsafe { <Option<&mut bool> as DecodeOwned>::soft_decode(encoded, &mut niche) };
    assert!(matches!(decoded, Some(Some(true))));
    assert!(niche.is_some());

    let decoded = unsafe {
        <Option<&mut bool> as DecodeOwned>::soft_decode(CRefMut::<CBool, true>::null(), &mut niche)
    };
    assert!(matches!(decoded, Some(None)));
    assert!(niche.is_none());
    assert_eq!(niche.sync(), Some(()));
}
