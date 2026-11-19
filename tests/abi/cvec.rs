use co3::vec::abi;

#[test]
fn raw_vector_roundtrip_preserves_allocation_and_capacity() {
    let mut value = Vec::with_capacity(32);
    value.extend([1u32, 2, 3]);
    let data = value.as_ptr();
    let capacity = value.capacity();
    let carrier = abi::Vec::<u32>::from_vec(value);
    assert_eq!(carrier.as_ptr(), data.cast_mut());
    assert_eq!(carrier.capacity(), capacity);
    let value = unsafe { carrier.into_vec() }.unwrap();
    assert_eq!(value, [1, 2, 3]);
    assert_eq!(value.capacity(), capacity);
}

#[test]
fn empty_and_zero_sized_vectors_roundtrip() {
    let empty = abi::Vec::<u8>::from_vec(Vec::<u8>::new());
    assert!(unsafe { empty.into_vec() }.unwrap().is_empty());
    let zst = abi::Vec::<()>::from_vec(vec![(); 3]);
    assert_eq!(unsafe { zst.into_vec() }.unwrap().len(), 3);
    assert!(unsafe { abi::Vec::<u8>::null().into_vec() }.is_none());
}

#[test]
fn stable_vec_owns_and_preserves_the_allocation() {
    let mut source = Vec::with_capacity(16);
    source.extend([1u32, 2, 3]);
    let capacity = source.capacity();
    let ptr = source.as_ptr();
    let mut stable = co3::vec::Vec::from(source);
    stable[1] = 9;
    assert_eq!(&*stable, &[1, 9, 3]);
    let result: Vec<u32> = stable.into();
    assert_eq!(result.as_ptr(), ptr);
    assert_eq!(result.capacity(), capacity);
}

#[test]
fn stable_vec_operations_preserve_ownership() {
    use std::{cell::Cell, rc::Rc};
    struct Count(Rc<Cell<usize>>);
    impl Drop for Count {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut value = co3::vec::Vec::with_capacity(4);
    value.push(Count(drops.clone()));
    value.push(Count(drops.clone()));
    assert_eq!(value.len(), 2);
    drop(value.pop());
    assert_eq!(drops.get(), 1);
    drop(value.into_iter());
    assert_eq!(drops.get(), 2);
    let mut zst = co3::vec::Vec::new();
    zst.push(());
    assert_eq!(zst.capacity(), usize::MAX);
    assert_eq!(zst.pop(), Some(()));
    let original: co3::vec::Vec<_> = [1, 2, 3].into_iter().collect();
    let mut cloned = original.clone();
    cloned[0] = 9;
    assert_eq!(original[0], 1);
    static_assertions::assert_impl_all!(co3::vec::Vec<u8>: Send, Sync);
    static_assertions::assert_not_impl_any!(co3::vec::Vec<Rc<u8>>: Send, Sync);
}

#[test]
fn raw_carrier_construction_needs_no_allocation_invariants() {
    let carrier = unsafe { abi::Vec::<u8>::from_raw_parts(core::ptr::null_mut(), 7, 3) };
    assert_eq!(carrier.len(), 7);
    assert_eq!(carrier.capacity(), 3);
}

#[test]
fn owned_encoding_preserves_capacity_and_validates_elements() {
    let mut value = Vec::with_capacity(32);
    value.extend([1u32, 2, 3]);
    let cap = value.capacity();
    let mut encode_store = Default::default();
    let carrier = co3::soft_encode(co3::vec::Vec::from(value), &mut encode_store);
    assert_eq!(carrier.len(), 3);
    assert_eq!(carrier.capacity(), cap);
    let value = unsafe { co3::soft_decode::<co3::vec::Vec<u32>>(carrier, &mut Default::default()) }
        .unwrap();
    assert_eq!(value.capacity(), cap);
    assert_eq!(&*value, &[1, 2, 3]);
    assert_eq!(co3::convert::Store::sync(encode_store), Some(()));
    let invalid =
        abi::Vec::<<bool as co3::ReprC>::CType, true>::from_vec(vec![co3::primitives::Bool(2)]);
    assert!(
        unsafe { co3::soft_decode::<co3::vec::Vec<bool>>(invalid, &mut Default::default()) }
            .is_none()
    );
}

#[test]
fn unstable_elements_and_value_traits() {
    let value: co3::vec::Vec<_> = vec![(1u32,), (2,)].into();
    let mut encode_store = Default::default();
    let carrier = co3::soft_encode(value, &mut encode_store);
    let mut decode_store = Default::default();
    let decoded =
        unsafe { co3::soft_decode::<co3::vec::Vec<(u32,)>>(carrier, &mut decode_store) }.unwrap();
    assert_eq!(decoded, vec![(1u32,), (2,)].into());
    assert_eq!(co3::convert::Store::sync(decode_store), Some(()));
    assert_eq!(co3::convert::Store::sync(encode_store), Some(()));
    let mut values = co3::vec::Vec::new();
    values.extend([1, 2]);
    assert!(values < vec![1, 3].into());
    static_assertions::assert_impl_all!(co3::vec::Vec<u8>: Eq, Ord, core::hash::Hash, AsRef<[u8]>, AsMut<[u8]>);
}

#[test]
fn standard_vec_preserves_spare_capacity() {
    let mut value = Vec::with_capacity(64);
    value.extend([3u32, 4]);
    let cap = value.capacity();
    let mut encode_store = Default::default();
    let carrier = co3::soft_encode(value, &mut encode_store);
    assert_eq!(carrier.len(), 2);
    assert_eq!(carrier.capacity(), cap);
    let value = unsafe { co3::soft_decode::<Vec<u32>>(carrier, &mut Default::default()) }.unwrap();
    assert_eq!(value.capacity(), cap);
    assert_eq!(value, [3, 4]);
    assert_eq!(co3::convert::Store::sync(encode_store), Some(()));
}

#[test]
fn boxed_unstable_elements_have_no_public_conversion() {
    static_assertions::assert_not_impl_any!(Box<[(u32,)]>: co3::convert::Encode, co3::convert::Decode<'static>);
}

#[test]
fn mutable_slice_writeback_uses_an_explicit_slice_header() {
    let mut values = [false, true];
    let mut store = Default::default();
    let carrier = co3::soft_encode(values.as_mut_slice(), &mut store);
    assert_eq!(carrier.metadata(), 2);
    unsafe {
        carrier.as_ptr().write(co3::primitives::Bool(1));
    }
    assert_eq!(co3::convert::Store::sync(store), Some(()));
    assert_eq!(values, [true, true]);
}

#[test]
fn derived_carrier_spec_preserves_header_and_pointee_properties() {
    use core::cell::UnsafeCell;

    use co3::rust_spec::{
        self, RustSpec, Stable,
        layout::Robust,
        mutability::{Exclusive, WithInterior, WithoutInterior},
    };
    use static_assertions::assert_impl_all;

    assert_impl_all!(abi::Vec<()>: RustSpec<
        Layout = Stable,
        Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
        __Alignment = <usize as RustSpec>::__Alignment,
        Trap = Robust,
        Niche = rust_spec::niche::WithoutNiche,
        Drop = rust_spec::drop::NoDrop,
        Mutability = Exclusive<WithoutInterior>,
        Opacity = rust_spec::NonOpaque,
        __IndirectTrap = Robust,
    >);
    // bool lowers to the robust Bool carrier, including inside UnsafeCell.
    assert_impl_all!(abi::Vec<<UnsafeCell<bool> as co3::ReprC>::CType>: RustSpec<
        Layout = Stable,
        Trap = Robust,
        Mutability = Exclusive<WithInterior<Robust>>,
        __IndirectTrap = Robust,
    >);
}
