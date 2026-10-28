use co3::{
    option::{COption, Option as StableOption},
    primitives::CBool,
    rust_spec::RustSpec,
    stored::{EncodeMove, EncodeMoveUnchecked, Store},
};

extern "C" fn first_callback() {}
extern "C" fn second_callback() {}

#[test]
fn mutable_non_robust_wrappers_validate_writeback() {
    use core::{
        cell::{Cell, UnsafeCell},
        mem::ManuallyDrop,
    };

    static_assertions::assert_not_impl_any!(Cell<bool>: co3::stored::EncodeRef<'static>);
    static_assertions::assert_not_impl_any!(UnsafeCell<bool>: co3::stored::EncodeRef<'static>);

    let mut manual = ManuallyDrop::new(true);
    let mut store = Default::default();
    let encoded = (&mut manual).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = co3::primitives::CBool(2) };
    assert_eq!(store.sync(), None);
    assert!(*manual);

    let mut manual_string = ManuallyDrop::new(String::from("held"));
    let mut store = Default::default();
    let _encoded = (&mut manual_string).soft_encode(&mut store);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(&*manual_string, "held");

    let mut cell = Cell::new(true);
    let mut store = Default::default();
    let encoded = (&mut cell).soft_encode(&mut store);
    unsafe { *(*encoded.as_ptr()).get() = co3::primitives::CBool::FALSE };
    assert_eq!(store.sync(), Some(()));
    assert!(!cell.get());

    let mut nonzero_cell = Cell::new(core::num::NonZeroU8::new(3).unwrap());
    let mut store = Default::default();
    let encoded = (&mut nonzero_cell).soft_encode(&mut store);
    unsafe { *(*encoded.as_ptr()).get() = 0 };
    assert_eq!(store.sync(), None);
    assert_eq!(nonzero_cell.get().get(), 3);

    let mut unsafe_cell = UnsafeCell::new(true);
    let mut store = Default::default();
    let encoded = (&mut unsafe_cell).soft_encode(&mut store);
    unsafe { *(*encoded.as_ptr()).get() = co3::primitives::CBool(2) };
    assert_eq!(store.sync(), None);
    assert!(unsafe { *unsafe_cell.get() });
}

#[test]
fn mutable_function_pointer_rejects_null_writeback() {
    let mut callback: extern "C" fn() = first_callback;
    let mut store = Default::default();
    let encoded = (&mut callback).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = None };
    assert_eq!(store.sync(), None);
    assert!(core::ptr::fn_addr_eq(
        callback,
        first_callback as extern "C" fn()
    ));

    let mut store = Default::default();
    let encoded = (&mut callback).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = Some(second_callback) };
    assert_eq!(store.sync(), Some(()));
    assert!(core::ptr::fn_addr_eq(
        callback,
        second_callback as extern "C" fn()
    ));
}

#[test]
fn mutable_repr_c_option_writes_back() {
    let mut value = StableOption::Some(7_u32);
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    let output = unsafe { &mut *encoded.as_ptr() };
    *output = COption::Some(11);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, StableOption::Some(11));

    let mut value = StableOption::<u32>::None;
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = COption::Some(13) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, StableOption::Some(13));
}

#[test]
fn mutable_copy_result_can_change_variant() {
    let mut value: Result<u32, bool> = Ok(7);
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = co3::result::CResult::Err(co3::primitives::CBool::TRUE) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, Err(true));

    let mut value: co3::result::Result<u32, bool> = co3::result::Result::Ok(4);
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = co3::result::CResult::Err(co3::primitives::CBool::TRUE) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, co3::result::Result::Err(true));
}

#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct Record(Vec<(u32,)>, Box<(u32,)>);

#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct DerivedMutable {
    values: Vec<u32>,
    flag: bool,
}

#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct GenericMutable<T>(T, bool);

#[derive(RustSpec, co3::ReprC)]
#[repr_c(is_valid = |value| *value != 0)]
struct ValidatedMutable(u32);

#[test]
fn derived_mutable_encoding_respects_custom_validation_boundary() {
    static_assertions::assert_not_impl_any!(ValidatedMutable:
        co3::stored::EncodeMutUnchecked<'static>, co3::stored::EncodeMut<'static>);
}

#[test]
fn mutable_vec_reuses_buffer_and_writes_back() {
    let mut values = Box::<[u32]>::from([7, 11]).into_vec();
    let original = values.as_ptr();
    let mut store = Default::default();
    let encoded = (&mut values).soft_encode(&mut store);
    let header = unsafe { &mut *encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), original);
    unsafe { header.as_ptr().write(17) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [17, 11]);
}

#[test]
fn mutable_vec_of_boxes_reuses_nested_allocations() {
    let mut values = vec![Box::new(3_u32)];
    let old_pointee = &*values[0] as *const u32;
    let mut store = Default::default();
    let encoded = (&mut values).soft_encode(&mut store);
    let header = unsafe { &mut *encoded.as_ptr() };
    let pointee = unsafe { (*header.as_ptr()).as_ptr() };
    assert_eq!(pointee.cast_const(), old_pointee);
    unsafe { pointee.write(9) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(*values[0], 9);
}

#[test]
fn mutable_boxed_slice_converts_unstable_elements() {
    let mut values: Box<[(u32,)]> = Box::new([(3,), (5,)]);
    let mut store = Default::default();
    let encoded = (&mut values).soft_encode(&mut store);
    let header = unsafe { &mut *encoded.as_ptr() };
    unsafe { (*header.as_ptr().add(1)).0 = 8 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(&*values, &[(3,), (8,)]);
}

#[test]
fn mutable_vec_converts_unstable_elements() {
    let mut values = vec![(3_u32,), (5,)];
    let mut store = Default::default();
    let encoded = (&mut values).soft_encode(&mut store);
    let header = unsafe { &mut *encoded.as_ptr() };
    unsafe { (*header.as_ptr().add(1)).0 = 8 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [(3,), (8,)]);
}

#[test]
fn stable_mutable_vec_has_no_element_store() {
    fn empty_checked<
        'a,
        T: co3::ReprC<CType: Sized>
            + co3::stored::DecodeOwned<'a, Store: co3::stored::EmptyStore + 'a>
            + co3::stored::EncodeMut<'a, Store = co3::sync::RefMutSizedEncodeStore<'a, T, ()>>,
    >() {
    }
    fn empty_direct<
        'a,
        T: co3::ReprC<CType: Sized>
            + co3::stored::DecodeOwned<'a, Store: co3::stored::EmptyStore + 'a>
            + co3::stored::EncodeMutUnchecked<
                'a,
                SoftStore = co3::sync::RefMutSizedEncodeStore<'a, T, ()>,
            >,
    >() {
    }
    empty_checked::<Vec<u32>>();
    empty_direct::<Vec<u32>>();
    empty_checked::<Box<[u32]>>();
    empty_direct::<Box<[u32]>>();

    let mut values = Box::<[u32]>::from([31, 32]).into_vec();
    let original = values.as_ptr();
    let mut store = Default::default();
    let encoded = unsafe { (&mut values).soft_encode_unchecked(&mut store) };
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), original);
    unsafe { header.as_ptr().add(1).write(33) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [31, 33]);
}

#[test]
fn mutable_vec_allows_replacing_its_owned_header() {
    let mut values = vec![1_u32, 2];
    let mut store = Default::default();
    let encoded = (&mut values).soft_encode(&mut store);
    let header = unsafe { &mut *encoded.as_ptr() };
    // A callee can consume the old allocation and install a new owned value.
    drop(unsafe { header.into_box().unwrap() });
    *header = vec![4_u32, 5, 6].soft_encode(&mut ());
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [4, 5, 6]);
}

#[derive(RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(identity)]
struct NoClone(u32);

#[test]
fn stable_mutable_vec_reuses_non_clone_elements() {
    static_assertions::assert_not_impl_any!(NoClone:
        co3::stored::EncodeMutUnchecked<'static>, co3::stored::EncodeMut<'static>);

    let mut values = Box::<[NoClone]>::from([NoClone(41)]).into_vec();
    let original = values.as_ptr();
    let mut store = Default::default();
    let encoded = (&mut values).soft_encode(&mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), original);
    unsafe { (*header.as_ptr()).0 = 42 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[0].0, 42);
}

#[test]
fn stable_identity_mutable_reference_needs_no_referent_encoder() {
    let mut value = NoClone(51);
    let ptr = &mut value as *mut NoClone;
    let encoded = (&mut value).soft_encode(&mut ());
    assert_eq!(encoded.as_ptr(), ptr);
    unsafe { (*encoded.as_ptr()).0 = 52 };
    assert_eq!(value.0, 52);
}

#[derive(RustSpec, co3::ReprC)]
#[repr_c(transparent)]
struct TransparentBuffer(Vec<u32>);

#[test]
fn transparent_mutable_wrapper_delegates_writeback() {
    let mut value = TransparentBuffer(vec![61]);
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { (*encoded.as_ptr()).as_ptr().write(62) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value.0, [62]);
}

#[test]
fn mutable_non_robust_buffer_validates_writeback() {
    let mut value = vec![false];
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { (*encoded.as_ptr()).as_ptr().cast::<u8>().write(2) };
    assert_eq!(store.sync(), None);
    assert!(value.is_empty());
}

#[test]
fn mutable_dst_store_commits_only_on_sync() {
    use co3::primitives::CBool;

    let mut value = [false, true];
    let mut store = Default::default();
    let encoded = (&mut value[..]).soft_encode(&mut store);
    unsafe { encoded.as_ptr().write(CBool::TRUE) };
    drop(store);
    assert_eq!(value, [false, true]);

    let mut store = Default::default();
    let encoded = (&mut value[..]).soft_encode(&mut store);
    unsafe { encoded.as_ptr().write(CBool::TRUE) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, [true, true]);

    let mut store = Default::default();
    let encoded = (&mut value[..]).soft_encode(&mut store);
    unsafe { encoded.as_ptr().write(CBool(2)) };
    assert_eq!(store.sync(), None);
    assert_eq!(value, [true, true]);
}

#[test]
fn mutable_string_writes_back() {
    let mut value = String::from("abc");
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { (*encoded.as_ptr()).as_ptr().cast::<u8>().write(b'z') };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, "zbc");
}

#[test]
fn nested_mutable_reference_writes_back_to_original_vec() {
    let mut values = vec![1_u32, 2];
    let mut reference = &mut values;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut reference, &mut store);
    unsafe { (*(*encoded.as_ptr()).as_ptr()).as_ptr().add(1).write(9) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(*reference, [1, 9]);

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&mut reference, &mut store) };
    unsafe { (*(*encoded.as_ptr()).as_ptr()).as_ptr().write(7) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(*reference, [7, 9]);
}

#[test]
fn nested_mutable_reference_rejects_pointer_replacement() {
    let mut values = vec![1_u32, 2];
    let mut reference = &mut values;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut reference, &mut store);
    unsafe { *encoded.as_ptr() = co3::reference::CRefMut::null() };
    assert_eq!(store.sync(), None);
    assert_eq!(*reference, [1, 2]);
}

#[test]
fn nested_mutable_reference_validates_pointee_and_retains_borrowed_elements() {
    let mut values = vec![true];
    let mut reference = &mut values;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut reference, &mut store);
    unsafe { *(*(*encoded.as_ptr()).as_ptr()).as_ptr() = co3::primitives::CBool(2) };
    assert_eq!(store.sync(), None);
    // Move-out encoding leaves a valid empty source when validation fails.
    assert!(reference.is_empty());

    let value = 7_u32;
    let mut values = vec![&value];
    let mut reference = &mut values;
    let mut store = Default::default();
    let _encoded = co3::soft_encode(&mut reference, &mut store);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(reference[0], &value);
}

#[test]
fn sized_aggregate_mutable_encoders_are_unsupported() {
    use co3::stored::{EncodeMut, EncodeMutUnchecked};
    static_assertions::assert_not_impl_any!([bool; 2]: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!((u32, bool): EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(co3::tuple::ReprCTuple2<u32, bool>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(Record: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(DerivedMutable: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(GenericMutable<u32>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
}

#[test]
fn stable_vec_reuses_allocation() {
    let values = Box::<[u8]>::from([1, 2, 3]).into_vec();
    let original = values.as_ptr();
    let mut store = Default::default();
    let encoded = co3::soft_encode(values, &mut store);
    assert_eq!(encoded.as_ptr(), original.cast_mut());
    assert_eq!(encoded.metadata(), 3);
    let decoded = unsafe { encoded.into_box().unwrap() }.into_vec();
    assert_eq!(decoded, [1, 2, 3]);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn stable_vec_reuses_allocation_and_syncs_element_stores() {
    let mut values = [false, false];
    let references: Vec<_> = values.iter_mut().collect();
    let original = references.as_ptr().cast_mut().cast();
    let mut store = Default::default();
    let encoded = co3::soft_encode(references, &mut store);
    assert_eq!(encoded.as_ptr(), original);
    unsafe { *(*encoded.as_ptr()).as_ptr() = CBool::TRUE };
    drop(unsafe { encoded.into_box().unwrap() });
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [true, false]);
}
