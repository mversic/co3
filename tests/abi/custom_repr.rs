use co3::{ReprC, rust_spec::RustSpec};
use static_assertions::{assert_impl_all, assert_not_impl_any};
use std::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct FieldHandle(u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[repr_c(transparent)]
struct CFieldHandle(u32);

impl From<u32> for FieldHandle {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct TransparentFieldHandle(u32);

impl From<u32> for TransparentFieldHandle {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(is_valid = |value| *value > 10)]
struct ValidatedByte(u8);

#[repr(transparent)]
#[derive(Clone, RustSpec, ReprC)]
#[repr_c(is_valid = |value| *value)]
struct ValidatedBool(bool);

#[derive(RustSpec, ReprC)]
#[repr_c(is_valid = |value| *value > 10)]
struct ValidatedOwned(u8);

#[derive(RustSpec, ReprC)]
enum ValidatedVariant {
    #[repr_c(is_valid = |value| *value > 10)]
    Value(u8),
    Empty,
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed custom is_valid")]
fn checked_owned_encoding_asserts_custom_validity() {
    let _ = co3::encode(ValidatedOwned(10));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed custom is_valid")]
fn checked_variant_encoding_asserts_custom_validity() {
    let _ = co3::encode(ValidatedVariant::Value(10));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed custom is_valid")]
fn checked_field_encoding_asserts_custom_validity() {
    let _ = co3::encode(ValidatedByte(10));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed CheckedTransmute::is_valid")]
fn checked_reference_encoding_asserts_pointee_validity() {
    let _ = co3::encode(&ValidatedByte(10));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed CheckedTransmute::is_valid")]
fn checked_mut_reference_encoding_asserts_pointee_validity() {
    let mut value = ValidatedByte(10);
    let _ = co3::encode(&mut value);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed CheckedTransmute::is_valid")]
fn checked_wide_reference_encoding_asserts_each_pointee() {
    let values = [ValidatedByte(11), ValidatedByte(10)];
    let _ = co3::encode(&values[..]);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed CheckedTransmute::is_valid")]
fn checked_mut_wide_reference_encoding_asserts_each_pointee() {
    let mut values = [ValidatedByte(11), ValidatedByte(10)];
    let _ = co3::encode(&mut values[..]);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed CheckedTransmute::is_valid")]
fn checked_interior_wide_reference_encoding_asserts_each_pointee() {
    let values = [
        UnsafeCell::new(ValidatedByte(11)),
        UnsafeCell::new(ValidatedByte(10)),
    ];
    let _ = co3::encode(&values[..]);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed CheckedTransmute::is_valid")]
fn checked_mut_interior_wide_reference_encoding_asserts_each_pointee() {
    let mut values = [
        UnsafeCell::new(ValidatedByte(11)),
        UnsafeCell::new(ValidatedByte(10)),
    ];
    let _ = co3::encode(&mut values[..]);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed custom is_valid")]
fn checked_mut_nonrobust_wide_reference_asserts_original_pointee_validity() {
    let mut values = [ValidatedBool(true), ValidatedBool(false)];
    let mut store = Default::default();
    let _ = co3::stored::EncodeOwned::soft_encode(&mut values[..], &mut store);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed custom is_valid")]
fn checked_wide_box_encoding_asserts_each_pointee() {
    let values = [ValidatedByte(11), ValidatedByte(10)];
    let _ = co3::stored::encode_owned(Vec::from(values).into_boxed_slice());
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "encoding failed custom is_valid")]
fn checked_box_encoding_asserts_pointee_validity() {
    let _ = co3::encode(Box::new(ValidatedByte(10)));
}

impl From<u8> for ValidatedByte {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct NonZeroField(core::num::NonZeroU8);

impl From<core::num::NonZeroU8> for NonZeroField {
    fn from(value: core::num::NonZeroU8) -> Self {
        Self(value)
    }
}

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct AutoDropBox {
    inner: Box<u32>,
}

impl From<Box<u32>> for AutoDropBox {
    fn from(value: Box<u32>) -> Self {
        Self { inner: value }
    }
}

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct GenericField<T>(T);

impl<T> From<T> for GenericField<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

static DROP_BOX_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
struct DropBox(Box<u32>);

impl Drop for DropBox {
    fn drop(&mut self) {
        DROP_BOX_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct FieldSlice<T>([T]);

#[test]
fn field_representation_maps_owned_conversion_and_transmutation() {
    assert_impl_all!(FieldHandle: ReprC<CType = u32>);
    assert_not_impl_any!(FieldHandle: co3::transmute::CheckedTransmute);
    assert_not_impl_any!(CFieldHandle: co3::transmute::CheckedTransmute);
    assert_impl_all!(TransparentFieldHandle: co3::transmute::CheckedTransmute);
    assert_impl_all!(FieldHandle: co3::stored::DirectEncodeOwned);
    assert_impl_all!(CFieldHandle: co3::stored::DirectEncodeOwned);
    assert_impl_all!(TransparentFieldHandle: co3::stored::DirectEncodeOwned);
    assert!(!unsafe {
        <ValidatedByte as co3::transmute::CheckedTransmute>::is_valid(&CValidatedByte::from_fields(
            10,
        ))
    });
    assert!(unsafe {
        <ValidatedByte as co3::transmute::CheckedTransmute>::is_valid(&CValidatedByte::from_fields(
            11,
        ))
    });
    assert!(unsafe { co3::decode::<ValidatedByte>(CValidatedByte::from_fields(10)) }.is_none());
    assert_eq!(
        unsafe { co3::decode::<ValidatedByte>(CValidatedByte::from_fields(11)) }
            .unwrap()
            .0,
        11
    );
    assert!(!unsafe { <NonZeroField as co3::transmute::CheckedTransmute>::is_valid(&0) });
    assert!(unsafe { <NonZeroField as co3::transmute::CheckedTransmute>::is_valid(&1) });
    assert_impl_all!(FieldSlice<u8>: ReprC<CType = [u8]>);
    assert_impl_all!(FieldSlice<u8>: co3::transmute::CheckedTransmute);
    assert_impl_all!(FieldSlice<core::mem::MaybeUninit<u8>>: ReprC<CType = [core::mem::MaybeUninit<u8>]>);
    assert_impl_all!(FieldSlice<core::mem::MaybeUninit<u8>>: co3::transmute::CheckedTransmute);
    assert_eq!(co3::encode(FieldHandle(7)), 7);
    assert_eq!(unsafe { co3::decode::<FieldHandle>(7) }.unwrap().0, 7);
}

#[test]
fn field_representation_preserves_inner_niche() {
    assert_impl_all!(NonZeroField: co3::niche::Niche<CType = u8>);
    assert_eq!(<NonZeroField as co3::niche::Niche>::NICHE, 0_u8);
    assert!(matches!(
        unsafe { co3::decode::<Option<NonZeroField>>(0) },
        Some(None)
    ));
}

#[test]
fn field_representation_transfers_custom_drop_field() {
    let before = DROP_BOX_COUNT.load(Ordering::Relaxed);
    let encoded = co3::encode(DropBox(Box::new(7)));
    assert_eq!(DROP_BOX_COUNT.load(Ordering::Relaxed), before);

    let decoded: DropBox = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.0, 7);
    drop(decoded);
    assert_eq!(DROP_BOX_COUNT.load(Ordering::Relaxed), before + 1);
}

#[test]
fn field_representation_moves_auto_drop_field() {
    let encoded = co3::encode(AutoDropBox {
        inner: Box::new(11),
    });
    let decoded: AutoDropBox = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.inner, 11);
}

#[test]
fn field_representation_selects_generic_drop_behavior() {
    assert_eq!(co3::encode(GenericField(13_u32)), 13);
    let encoded = co3::encode(GenericField(Box::new(17_u32)));
    let decoded: GenericField<Box<u32>> = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.0, 17);
}

#[test]
fn cells_rebuild_transparent_slice_wrappers() {
    use co3::wide::Wide;

    let mut bytes = [1_u8, 2, 3];
    let slice = core::ptr::slice_from_raw_parts_mut(bytes.as_mut_ptr(), bytes.len());
    let unsafe_cell = slice as *mut UnsafeCell<FieldSlice<u8>>;
    let header = <UnsafeCell<FieldSlice<u8>> as Wide>::as_mut_ptr(unsafe_cell);
    let len = <UnsafeCell<FieldSlice<u8>> as Wide>::metadata(unsafe_cell);
    let rebuilt = unsafe { <UnsafeCell<FieldSlice<u8>> as Wide>::from_raw_parts_mut(header, len) };
    assert!(core::ptr::eq(unsafe_cell, rebuilt));
    assert_eq!(len, 3);

    let cell = slice as *mut Cell<FieldSlice<u8>>;
    let header = <Cell<FieldSlice<u8>> as Wide>::as_mut_ptr(cell);
    let rebuilt = unsafe { <Cell<FieldSlice<u8>> as Wide>::from_raw_parts_mut(header, 3) };
    assert!(core::ptr::eq(cell, rebuilt));
}
