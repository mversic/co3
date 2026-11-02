use co3::{
    primitives::Bool,
    stored::Store,
    sync::{RefMutWideEncodeStore, RefWideEncodeStore},
    tuple::{Tuple2, Tuple3},
    wide::Wide,
};

#[test]
fn mutable_store_uses_rust_pointee_and_its_c_representation() {
    let mut result = false;
    let mut store = RefMutWideEncodeStore::<[bool], (), &mut bool>::default();
    let carrier = store.encode(
        |_| (vec![Bool::FALSE].into_boxed_slice(), &mut result),
        |value, result| *result = value[0] == Bool::TRUE,
    );
    unsafe { carrier.into_mut().unwrap()[0] = Bool::TRUE };
    assert_eq!(store.sync(), Some(()));
    assert!(result);
}

#[test]
fn shared_reuse_keeps_previous_pointee_alive_during_construction() {
    let mut store = RefWideEncodeStore::<[u32], ()>::default();
    let previous = store.encode(|_| vec![7].into_boxed_slice());
    let current = store.encode(|_| {
        let value = unsafe { *previous.as_ptr() };
        vec![value + 1].into_boxed_slice()
    });
    assert_eq!(unsafe { current.into_ref().unwrap() }, &[8]);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn mutable_reuse_only_writes_back_the_latest_value() {
    let mut first = 1;
    let mut second = 2;
    let mut store = RefMutWideEncodeStore::<[u32], (), &mut u32>::default();
    store.encode(
        |_| (vec![10].into_boxed_slice(), &mut first),
        |value, target| *target = value[0],
    );
    let current = store.encode(
        |_| (vec![20].into_boxed_slice(), &mut second),
        |value, target| *target = value[0],
    );
    unsafe { current.into_mut().unwrap()[0] = 30 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(first, 1);
    assert_eq!(second, 30);
}

#[test]
fn builder_panic_cancels_pending_mutable_writeback() {
    let mut source = 1;
    let mut store = RefMutWideEncodeStore::<[u32], (), &mut u32>::default();
    store.encode(
        |_| (vec![10].into_boxed_slice(), &mut source),
        |value, target| *target = value[0],
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.encode(|_| panic!("builder failed"), |_, _| unreachable!());
    }));
    assert!(result.is_err());
    assert_eq!(store.sync(), Some(()));
    assert_eq!(source, 1);
}

fn sync_after_move<S: Store>(store: S) -> Option<()> {
    let moved = Box::new(store);
    (*moved).sync()
}

#[test]
fn stores_can_move_before_writeback() {
    let mut values = [1_u32, 2];
    let mut store = ();
    let encoded = co3::soft_encode(&mut values[..], &mut store);
    unsafe { encoded.as_ptr().cast::<u32>().write(3) };
    assert_eq!(sync_after_move(store), Some(()));
    assert_eq!(values, [3, 2]);
}

#[test]
fn wide_tuple_preserves_data_and_metadata() {
    let value: Box<Tuple3<u8, u64, [u32]>> = Box::new(Tuple3(5, 17, [23, 29, 31]));
    type Tuple = Tuple3<u8, u64, [u32]>;

    let ptr = core::ptr::from_ref::<Tuple>(&value);
    let len = Tuple::metadata(ptr);
    let header = Tuple::as_ptr(ptr);
    assert_eq!(len, 3);
    assert_eq!(header.cast::<()>(), ptr.cast::<()>());
    let borrowed = unsafe { Tuple::from_raw_parts(header, len) };
    assert_eq!(borrowed.0, 5);
    assert_eq!(borrowed.1, 17);
    assert_eq!(&borrowed.2, &[23, 29, 31]);

    let data = Tuple::into_non_null(value);
    let recovered = unsafe { Tuple::from_non_null(data, len) };
    assert_eq!(&recovered.2, &[23, 29, 31]);
}

#[test]
fn one_field_wide_tuples_are_unsupported() {
    use co3::stored::{EncodeMut, EncodeMutUnchecked, EncodeRef, EncodeRefUnchecked};
    use co3::tuple::Tuple1;
    assert_not_shared_encoder!(Tuple1<[u8]>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(Tuple1<[u8]>: Wide, EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_not_shared_encoder!(([u8],): EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(([u8],): EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_not_shared_encoder!((u32, [u8]): EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!((u32, [u8]): EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_not_shared_encoder!(Tuple2<u32, [u8]>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    static_assertions::assert_not_impl_any!(Tuple2<u32, [u8]>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    assert_shared_encoder!(Tuple1<u8>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    assert_not_shared_encoder!(Tuple1<core::ffi::CStr>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    assert_not_shared_encoder!(Tuple2<u32, core::ffi::CStr>: EncodeRef<'static>, EncodeRefUnchecked<'static>);
    assert_not_shared_encoder!((u32, core::ffi::CStr): EncodeRef<'static>, EncodeRefUnchecked<'static>);
}

#[test]
fn nested_wide_tuples_preserve_tail_metadata() {
    type Nested = Tuple2<u16, Tuple2<u8, [u32]>>;
    let value: Box<Nested> = Box::new(Tuple2(7, Tuple2(3, [11, 13])));
    let ptr = core::ptr::from_ref::<Nested>(&value);
    let recovered = unsafe { Nested::from_raw_parts(Nested::as_ptr(ptr), Nested::metadata(ptr)) };
    assert_eq!(recovered.0, 7);
    assert_eq!(recovered.1.0, 3);
    assert_eq!(&recovered.1.1, &[11, 13]);
}
