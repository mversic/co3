use co3::{
    rust_spec::RustSpec,
    stored::{EncodeMove, EncodeMoveUnchecked, Store},
};

use crate::TransparentCarrier;

// No Clone implementation: the stable vector path must not require one.
#[derive(RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(identity)]
struct NoClone(u32);

#[test]
fn stable_vec_borrows_non_clone_elements() {
    let values = vec![NoClone(7), NoClone(11)];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
    assert_eq!(header.len(), 2);
    assert_eq!(unsafe { (*header.as_ptr()).0 }, 7);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[1].0, 11);

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn nested_shared_vec_refs_borrow_elements() {
    let values = vec![NoClone(7), NoClone(11)];
    let reference = &values;
    let nested = &reference;
    let mut store = Default::default();
    // Use the free function to prevent method-call autoderef from hiding a
    // missing nested-reference implementation.
    let encoded = co3::soft_encode(nested, &mut store);
    let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
    assert_eq!(header.len(), 2);
    assert_eq!(unsafe { (*header.as_ptr()).0 }, 7);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&nested, &mut store) };
    let header = unsafe { &*(*(*encoded.as_ptr()).as_ptr()).as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), values.as_ptr());
    assert_eq!(header.len(), 2);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values[1].0, 11);
}

#[test]
fn stable_vec_handles_empty_zst_and_non_robust_elements() {
    for values in [vec![], vec![false, true]] {
        let mut store = Default::default();
        let encoded = co3::soft_encode(&values, &mut store);
        let header = unsafe { &*encoded.as_ptr() };
        assert_eq!(header.as_ptr().cast::<bool>().cast_const(), values.as_ptr());
        assert_eq!(header.len(), values.len());
        assert_eq!(store.sync(), Some(()));
    }
    let values = vec![(); 3];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
    assert_eq!(unsafe { (*encoded.as_ptr()).len() }, 3);
    assert_eq!(store.sync(), Some(()));
}

#[derive(Clone, RustSpec, co3::ReprC)]
struct Record<T>(T);

// The layout of this downstream type is projected through its foreign field.
#[derive(Clone, RustSpec, co3::ReprC)]
#[repr(transparent)]
struct Projected((u32,));

#[test]
fn derived_shared_refs_support_field_dependent_layouts() {
    let value = Record((13_u32,));
    let mut store = Default::default();
    let encoded = co3::soft_encode(&value, &mut store);
    assert_eq!(unsafe { (*encoded.as_ptr()).0.0 }, 13);
    assert_eq!(store.sync(), Some(()));

    let value = Projected((17,));
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut store) };
    assert_eq!(unsafe { (*encoded.as_ptr()).0.0 }, 17);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn unstable_vec_converts_elements() {
    let values = vec![(19_u32,), (23,)];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_ne!(
        header.as_ptr().cast::<(u32,)>().cast_const(),
        values.as_ptr()
    );
    assert_eq!(header.len(), 2);
    assert_eq!(unsafe { (*header.as_ptr()).0 }, 19);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values, [(19,), (23,)]);
}

#[test]
fn checked_vec_of_cells_borrows_element_storage() {
    use core::cell::Cell;
    let values = vec![Cell::new(29_u32)];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_eq!(
        header.as_ptr().cast::<Cell<u32>>().cast_const(),
        values.as_ptr()
    );
    unsafe { header.as_ptr().cast::<u32>().write(31) };
    assert_eq!(values[0].get(), 31);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(
        header.as_ptr().cast::<Cell<u32>>().cast_const(),
        values.as_ptr()
    );
    assert_eq!(store.sync(), Some(()));
}

#[derive(RustSpec)]
struct RefOnly(u32);

unsafe impl co3::borrow::Borrow for RefOnly {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }
    fn from_borrow(source: Self) -> Self {
        source
    }
}

impl co3::ReprC for RefOnly {
    type CType = u32;
}

#[derive(Default)]
struct CheckedStore(bool);
impl Store for CheckedStore {
    fn sync(self) -> Option<()> {
        self.0.then_some(())
    }
}

// This downstream type needs neither Clone nor owned encoding. Its checked
// shared conversion has state, whereas its unchecked conversion has none.
unsafe impl<'a> co3::stored::EncodeRefUnchecked<'a> for RefOnly {
    type SoftStore = co3::sync::RefSizedEncodeStore<Self, ()>;
    unsafe fn encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as co3::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|_fields| self.0)
    }
}
unsafe impl<'a> co3::stored::EncodeRef<'a> for RefOnly {
    type Store = co3::sync::RefSizedEncodeStore<Self, CheckedStore>;
    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as co3::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| {
            fields.0 = true;
            self.0
        })
    }
}

#[test]
fn downstream_shared_only_type_keeps_independent_stores() {
    let value = RefOnly(37);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&value, &mut store);
    assert_eq!(unsafe { *encoded.as_ptr() }, 37);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut store) };
    assert_eq!(unsafe { *encoded.as_ptr() }, 37);
    assert_eq!(store.sync(), Some(()));
}

#[derive(RustSpec, co3::ReprC)]
struct Nested {
    values: Vec<NoClone>,
    text: String,
    boxed: Box<(u32,)>,
}

#[derive(RustSpec, co3::ReprC)]
#[repr_c(transparent)]
struct Transparent<T>(T);

#[derive(RustSpec, co3::ReprC)]
#[repr(u8)]
enum Choice {
    #[repr_c(allow_zero_tag)]
    Empty,
    Value(RefOnly),
}

#[test]
fn derived_struct_and_transparent_wrapper_delegate_without_clone() {
    let value = Nested {
        values: vec![NoClone(41)],
        text: String::from("borrowed"),
        boxed: Box::new((43,)),
    };
    let mut store = Default::default();
    let encoded = co3::soft_encode(&value, &mut store);
    let fields = unsafe { &*encoded.as_ptr() };
    assert_eq!(fields.values.as_ptr().cast_const(), value.values.as_ptr());
    assert_eq!(fields.text.as_ptr().cast_const(), value.text.as_ptr());
    assert_eq!(fields.text.capacity(), value.text.capacity());
    assert_eq!(unsafe { (*fields.boxed.as_ptr()).0 }, 43);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value.values[0].0, 41);

    let wrapped = Transparent(value.values);
    let mut store = Default::default();
    let header = co3::soft_encode(&wrapped, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), wrapped.0.as_ptr());
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn tuples_arrays_options_results_and_manually_drop_delegate() {
    let value = (vec![NoClone(47)],);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&value, &mut store);
    assert_eq!(
        unsafe { (*encoded.as_ptr()).0.as_ptr().cast_const() },
        value.0.as_ptr()
    );
    assert_eq!(store.sync(), Some(()));

    let array = [value.0];
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&array, &mut store) };
    let encoded = unsafe { &*encoded.as_ptr() };
    assert_eq!(encoded[0].as_ptr().cast_const(), array[0].as_ptr());
    assert_eq!(store.sync(), Some(()));

    let elements = vec![NoClone(53)];
    let pointer = elements.as_ptr();
    let value = Some(elements);
    let mut store = Default::default();
    let header = co3::soft_encode(&value, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(header.payload().as_ptr().cast_const(), pointer);
    assert_eq!(store.sync(), Some(()));

    let Some(value) = value else { unreachable!() };
    let value = Ok::<_, ()>(value);
    let mut store = Default::default();
    let header = co3::soft_encode(&value, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(header.payload().as_ptr().cast_const(), pointer);
    assert_eq!(store.sync(), Some(()));

    let Ok(value) = value else { unreachable!() };
    let value = core::mem::ManuallyDrop::new(value);
    let mut store = Default::default();
    let header = co3::soft_encode(&value, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), value.as_ptr());
    assert_eq!(store.sync(), Some(()));
    drop(core::mem::ManuallyDrop::into_inner(value));
}

#[test]
fn unstable_vec_delegates_to_non_clone_shared_only_elements() {
    let values = vec![(RefOnly(59),), (RefOnly(61),)];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(unsafe { (*header.as_ptr()).0 }, 59);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&values, &mut store) };
    assert_eq!(unsafe { (*(*encoded.as_ptr()).as_ptr()).0 }, 59);
    assert_eq!(store.sync(), Some(()));
    let boxed = values.into_boxed_slice();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&boxed, &mut store);
    assert_eq!(unsafe { (*(*encoded.as_ptr()).as_ptr()).0 }, 59);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn derived_enum_syncs_only_its_active_shared_store() {
    let mut store = Default::default();
    let _encoded = co3::soft_encode(&Choice::Value(RefOnly(67)), &mut store);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let _encoded = co3::soft_encode(&Choice::Value(RefOnly(71)), &mut store);
    let _encoded = co3::soft_encode(&Choice::Empty, &mut store);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn buffers_and_boxes_borrow_original_allocations() {
    let value = Box::new(NoClone(73));
    let header = co3::soft_encode(&value, &mut ());
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), &*value as *const NoClone);
    assert_eq!(().sync(), Some(()));

    let value = vec![NoClone(79)].into_boxed_slice();
    let mut store = Default::default();
    let header = co3::soft_encode(&value, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), value.as_ptr());
    assert_eq!(store.sync(), Some(()));

    let value = std::ffi::CString::new("borrowed").unwrap();
    let mut store = Default::default();
    let header = co3::soft_encode(&value, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(
        header.as_ptr().cast::<core::ffi::c_char>().cast_const(),
        value.as_ptr()
    );
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn tagged_wrappers_delegate_and_clear_inactive_stores() {
    let mut store = Default::default();
    let encoded = co3::soft_encode(&co3::option::Option::Some(RefOnly(83)), &mut store);
    let encoded = unsafe { &*encoded.as_ptr() };
    assert_eq!(encoded.into_option(), Some(Some(83)));
    let encoded = co3::soft_encode(&co3::option::Option::<RefOnly>::None, &mut store);
    let encoded = unsafe { &*encoded.as_ptr() };
    assert_eq!(encoded.into_option(), Some(None));
    let value = *encoded;
    store.encode(|fields| {
        assert!(fields.is_none());
        value
    });
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = co3::soft_encode(&Ok::<_, u32>(RefOnly(89)), &mut store);
    let encoded = unsafe { &*encoded.as_ptr() };
    assert_eq!(encoded.into_result(), Some(Ok(89)));
    let encoded = co3::soft_encode(&Err::<RefOnly, _>(97_u32), &mut store);
    let encoded = unsafe { &*encoded.as_ptr() };
    assert_eq!(encoded.into_result(), Some(Err(97)));
    let value = *encoded;
    store.encode(|fields| {
        assert!(matches!(fields, Some(Err(()))));
        value
    });
    assert_eq!(store.sync(), Some(()));

    let _ = co3::soft_encode(&Ok::<_, ()>(core::num::NonZeroU8::new(1).unwrap()), &mut ());
    let encoded = co3::soft_encode(&Err::<core::num::NonZeroU8, _>(()), &mut ());
    assert_eq!(unsafe { *encoded.as_ptr() }.payload().0, 0);
    assert_eq!(().sync(), Some(()));
}

#[test]
fn boxed_borrowed_fields_do_not_require_static_lifetimes() {
    let value = 101_u32;
    let boxed = Box::new((&value,));
    let mut store = Default::default();
    let header = co3::soft_encode(&boxed, &mut store);
    let header = unsafe { &*header.as_ptr() };
    assert_eq!(unsafe { (*header.as_ptr()).0.as_ptr() }, &value as *const _);
    assert_eq!(store.sync(), Some(()));
}

#[derive(RustSpec, co3::ReprC)]
#[repr_c(is_valid = |value| *value != 0)]
struct Validated(u32);

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "encoding failed custom is_valid")]
fn derived_shared_encoding_preserves_custom_validation() {
    co3::soft_encode(&Validated(0), &mut Default::default());
}

#[test]
fn cells_cannot_be_snapshotted_by_shared_encoders() {
    use core::cell::{Cell, UnsafeCell};

    use co3::stored::{EncodeRef, EncodeRefUnchecked};
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    assert_not_shared_encoder!(Cell<u32>: EncodeRefUnchecked<'static>, EncodeRef<'static>);
    assert_not_shared_encoder!(UnsafeCell<u32>: EncodeRefUnchecked<'static>, EncodeRef<'static>);
    assert_not_impl_any!(&Vec<Cell<bool>>: EncodeMove);
    assert_not_shared_encoder!(Box<Cell<bool>>: EncodeRef<'static>);
    assert_not_impl_any!(&Box<[Cell<bool>]>: EncodeMove);
    assert_not_impl_any!(&Vec<UnsafeCell<bool>>: EncodeMove);
    assert_not_impl_any!(&Record<Cell<u32>>: EncodeMove, EncodeMoveUnchecked);
    assert_impl_all!(&Vec<Cell<u32>>: EncodeMove, EncodeMoveUnchecked);
    assert_impl_all!(&Box<Cell<u32>>: EncodeMove, EncodeMoveUnchecked);
    assert_impl_all!(&Box<[Cell<u32>]>: EncodeMove, EncodeMoveUnchecked);
    assert_impl_all!(&Vec<UnsafeCell<u32>>: EncodeMove, EncodeMoveUnchecked);
}

#[test]
fn checked_boxes_of_cells_preserve_storage() {
    use core::cell::Cell;
    let value = Box::new(Cell::new(7_u32));
    let mut store = ();
    let encoded = co3::soft_encode(&value, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast::<Cell<u32>>().cast_const(), &*value);
    assert_eq!(store.sync(), Some(()));

    let values = vec![Cell::new(11_u32)].into_boxed_slice();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(
        header.as_ptr().cast::<Cell<u32>>().cast_const(),
        values.as_ptr()
    );
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn aggregates_encode_stable_fields_without_encode_ref_impls() {
    use co3::stored::{EncodeRef, EncodeRefUnchecked};
    assert_not_shared_encoder!(u32: EncodeRefUnchecked<'static>, EncodeRef<'static>);

    assert_not_shared_encoder!(Record<NoClone>: EncodeRefUnchecked<'static>, EncodeRef<'static>);
    assert_not_shared_encoder!((NoClone,): EncodeRefUnchecked<'static>, EncodeRef<'static>);

    let value = Record(109_u32);
    let mut store = Default::default();
    let encoded: <&Record<u32> as co3::ReprC>::CType = co3::soft_encode(&value, &mut store);
    assert_eq!(unsafe { (*encoded.as_ptr()).0 }, 109);
    assert_eq!(store.sync(), Some(()));

    let tuple = (113_u32,);
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&tuple, &mut store) };
    assert_eq!(unsafe { (*encoded.as_ptr()).0 }, 113);
    assert_eq!(store.sync(), Some(()));
}

#[derive(RustSpec, co3::ReprC)]
#[repr(transparent)]
#[repr_c(identity)]
struct InteriorIdentity(core::cell::UnsafeCell<u32>);

#[test]
fn stable_identity_reference_preserves_interior_storage() {
    let value = InteriorIdentity(core::cell::UnsafeCell::new(127));
    let mut store = ();
    let encoded = co3::soft_encode(&value, &mut store);
    assert_eq!(
        encoded.as_ptr().cast_const(),
        &value as *const InteriorIdentity
    );
    unsafe { *(*encoded.as_ptr()).0.get() = 131 };
    assert_eq!(unsafe { *value.0.get() }, 131);
}

#[test]
fn shared_borrow_of_mutable_reference_keeps_vec_shared() {
    let mut values = vec![NoClone(7), NoClone(11)];
    let pointer = values.as_ptr();
    let reference = &mut values;
    let mut store = Default::default();
    let encoded = co3::soft_encode(&reference, &mut store);
    let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), pointer);
    assert_eq!(header.len(), 2);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(reference[1].0, 11);

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&reference, &mut store) };
    let header = unsafe { &*(*encoded.as_ptr()).as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), pointer);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(reference[0].0, 7);
}

#[test]
fn shared_borrow_of_mutable_slice_reference_preserves_metadata() {
    assert_not_shared_encoder!(&mut Vec<core::cell::Cell<bool>>:
        co3::stored::EncodeRef<'static>);
    let mut values = [NoClone(3), NoClone(5)];
    let pointer = values.as_ptr();
    let reference = &mut values[..];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&reference, &mut store);
    let header = unsafe { &*encoded.as_ptr() };
    assert_eq!(header.as_ptr().cast_const(), pointer);
    assert_eq!(header.metadata(), 2);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(reference[1].0, 5);
}
