use co3::{
    option::{COption, Option as StableOption},
    rust_spec::RustSpec,
    stored::Store,
};

extern "C" fn first_callback() {}
extern "C" fn second_callback() {}

#[test]
fn mutable_non_robust_wrappers_validate_writeback() {
    use core::{
        cell::{Cell, UnsafeCell},
        mem::ManuallyDrop,
    };

    assert_not_shared_encoder!(Cell<bool>: co3::stored::EncodeRef<'static>);
    assert_not_shared_encoder!(UnsafeCell<bool>: co3::stored::EncodeRef<'static>);

    let mut manual = ManuallyDrop::new(true);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut manual, &mut store);
    unsafe { *encoded.as_ptr() = co3::primitives::CBool(2) };
    assert_eq!(store.sync(), None);
    assert!(*manual);

    let mut manual_string = ManuallyDrop::new(String::from("held"));
    let mut store = Default::default();
    let _encoded = co3::soft_encode(&mut manual_string, &mut store);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(&*manual_string, "held");

    let mut cell = Cell::new(true);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut cell, &mut store);
    unsafe { *(*encoded.as_ptr()).get() = co3::primitives::CBool::FALSE };
    assert_eq!(store.sync(), Some(()));
    assert!(!cell.get());

    let mut nonzero_cell = Cell::new(core::num::NonZeroU8::new(3).unwrap());
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut nonzero_cell, &mut store);
    unsafe { *(*encoded.as_ptr()).get() = core::mem::zeroed() };
    assert_eq!(store.sync(), None);
    assert_eq!(nonzero_cell.get().get(), 3);

    let mut unsafe_cell = UnsafeCell::new(true);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut unsafe_cell, &mut store);
    unsafe { *(*encoded.as_ptr()).get() = co3::primitives::CBool(2) };
    assert_eq!(store.sync(), None);
    assert!(unsafe { *unsafe_cell.get() });
}

#[test]
fn mutable_function_pointer_rejects_null_writeback() {
    let mut callback: extern "C" fn() = first_callback;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut callback, &mut store);
    unsafe { *encoded.as_ptr() = None };
    assert_eq!(store.sync(), None);
    assert!(core::ptr::fn_addr_eq(
        callback,
        first_callback as extern "C" fn()
    ));

    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut callback, &mut store);
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
    let encoded = co3::soft_encode(&mut value, &mut store);
    let output = unsafe { &mut *encoded.as_ptr() };
    *output = COption::Some(11);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, StableOption::Some(11));

    let mut value = StableOption::<u32>::None;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value, &mut store);
    unsafe { *encoded.as_ptr() = COption::Some(13) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, StableOption::Some(13));
}

#[test]
fn mutable_copy_result_can_change_variant() {
    let mut value: Result<u32, bool> = Ok(7);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value, &mut store);
    unsafe { *encoded.as_ptr() = co3::result::CResult::Err(co3::primitives::CBool::TRUE) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, Err(true));

    let mut value: co3::result::Result<u32, bool> = co3::result::Result::Ok(4);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value, &mut store);
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

// FIXME:
//#[test]
//fn mutable_vec_reuses_buffer_and_writes_back() {
//    let mut values = Box::<[u32]>::from([7, 11]).into_vec();
//    let original = values.as_ptr();
//    let mut store = Default::default();
//    let encoded = co3::soft_encode((&mut values), &mut store);
//    let header = unsafe { &mut *encoded.as_ptr() };
//    assert_eq!(header.as_ptr().cast_const(), original);
//    unsafe { header.as_ptr().write(17) };
//    assert_eq!(store.sync(), Some(()));
//    assert_eq!(values, [17, 11]);
//}

#[test]
fn mutable_vec_of_boxes_reuses_nested_allocations() {
    let mut values = vec![Box::new(3_u32)];
    let old_pointee = &*values[0] as *const u32;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
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
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { &mut *encoded.as_ptr() };
    unsafe { (*header.as_ptr().add(1)).0 = 8 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(&*values, &[(3,), (8,)]);
}

// FIXME:
//#[test]
//fn mutable_vec_converts_unstable_elements() {
//    let mut values = vec![(3_u32,), (5,)];
//    let mut store = Default::default();
//    let encoded = co3::soft_encode(&mut values, &mut store);
//    let header = unsafe { &mut *encoded.as_ptr() };
//    unsafe { (*header.as_ptr().add(1)).0 = 8 };
//    assert_eq!(store.sync(), Some(()));
//    assert_eq!(values, [(3,), (8,)]);
//}

#[test]
fn mutable_boxed_slice_has_no_element_store_and_vec_writes_back() {
    fn empty_checked<
        'a,
        T: co3::ReprC<CType: Sized>
            + co3::stored::DecodeMove<'a, Store: co3::stored::EmptyStore + 'a>
            + co3::stored::EncodeMut<'a, Store = co3::sync::RefMutSizedEncodeStore<'a, T, ()>>,
    >() {
    }
    fn empty_direct<
        'a,
        T: co3::ReprC<CType: Sized>
            + co3::stored::DecodeMove<'a, Store: co3::stored::EmptyStore + 'a>
            + co3::stored::EncodeMutUnchecked<
                'a,
                SoftStore = co3::sync::RefMutSizedEncodeStore<'a, T, ()>,
            >,
    >() {
    }
    empty_checked::<Box<[u32]>>();
    empty_direct::<Box<[u32]>>();

    let mut values = Box::<[u32]>::from([31, 32]).into_vec();
    let values_len = values.len();
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&mut values, &mut store) };
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.len(), values_len);
    unsafe { header.as_ptr().add(1).write(33) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [31, 33]);
}

#[test]
fn mutable_vec_rejects_header_length_changes() {
    let mut values = vec![1_u32, 2];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    unsafe {
        *encoded.as_ptr() = co3::vec::CVec::from_raw_parts(header.as_ptr(), 1, header.capacity());
    }
    assert_eq!(store.sync(), None);
    assert_eq!(values, [1, 2]);
}

#[derive(RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct NoClone(u32);

#[test]
fn stable_mutable_vec_converts_non_clone_elements() {
    static_assertions::assert_not_impl_any!(NoClone: Copy, Clone);
    static_assertions::assert_type_eq_all!(<NoClone as co3::ReprC>::CType, u32);
    static_assertions::assert_impl_all!(NoClone:
        co3::stored::EncodeMutUnchecked<'static>, co3::stored::EncodeMut<'static>);

    let mut values = Box::<[NoClone]>::from([NoClone(41)]).into_vec();
    let values_len = values.len();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.len(), values_len);
    unsafe { *header.as_ptr() = 42 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[0].0, 42);
}

#[test]
fn stable_identity_mutable_reference_needs_no_referent_encoder() {
    let mut value = 51_u32;
    let ptr = &mut value as *mut u32;
    let encoded = co3::soft_encode(&mut value, &mut Default::default());
    assert_eq!(encoded.as_ptr(), ptr);
    unsafe { *encoded.as_ptr() = 52 };
    assert_eq!(value, 52);
}

#[derive(RustSpec, co3::ReprC)]
#[repr_c(transparent)]
struct TransparentBuffer(Vec<u32>);

#[test]
fn transparent_mutable_wrapper_delegates_writeback() {
    let mut value = TransparentBuffer(vec![61]);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value, &mut store);
    unsafe { (*encoded.as_ptr()).as_ptr().write(62) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value.0, [62]);
}

#[test]
fn mutable_non_robust_buffer_validates_writeback() {
    let mut value = vec![false];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value, &mut store);
    unsafe { (*encoded.as_ptr()).as_ptr().cast::<u8>().write(2) };
    assert_eq!(store.sync(), None);
    assert_eq!(value, [false]);
}

#[test]
fn mutable_dst_store_commits_only_on_sync() {
    use co3::primitives::CBool;

    let mut value = [false, true];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value[..], &mut store);
    unsafe { encoded.as_ptr().write(CBool::TRUE) };
    drop(store);
    assert_eq!(value, [false, true]);

    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value[..], &mut store);
    unsafe { encoded.as_ptr().write(CBool::TRUE) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, [true, true]);

    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value[..], &mut store);
    unsafe { encoded.as_ptr().write(CBool(2)) };
    assert_eq!(store.sync(), None);
    assert_eq!(value, [true, true]);
}

#[test]
fn mutable_string_writes_back() {
    let mut value = String::from("abc");
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut value, &mut store);
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
    // Rejected writeback preserves the original valid boolean.
    assert_eq!(**reference, [true]);

    let value = 7_u32;
    let mut values = vec![&value];
    let mut reference = &mut values;
    let mut store = Default::default();
    let _encoded = co3::soft_encode(&mut reference, &mut store);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(reference[0], &value);
}

#[test]
fn sized_aggregate_mutable_encoder_coverage() {
    use co3::stored::{EncodeMut, EncodeMutUnchecked};
    static_assertions::assert_impl_all!([bool; 2]: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!((u32, bool): EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(co3::tuple::Tuple2<u32, bool>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(Record: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(DerivedMutable: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_not_impl_any!(GenericMutable<u32>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
}
