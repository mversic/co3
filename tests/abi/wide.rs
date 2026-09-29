use std::num::NonZeroU8;

use co3::{ReprC, rust_spec::RustSpec, wide::Wide};
use static_assertions::assert_impl_all;

#[repr(transparent)]
#[derive(RustSpec, ReprC)]
struct Bytes([u8]);

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct Packet {
    tag: NonZeroU8,
    payload: [u8],
}

#[repr(C)]
#[derive(RustSpec, ReprC)]
struct TuplePacket(NonZeroU8, [u8]);

#[test]
fn struct_wide_classification() {
    assert_impl_all!(Bytes: Wide<Metadata = usize>);
    assert_impl_all!(Packet: Wide<Metadata = usize>);
    assert_impl_all!(TuplePacket: Wide<Metadata = usize>);

    assert_impl_all!(<Bytes as Wide>::Data: Sized);
    assert_impl_all!(<Packet as Wide>::Data: Sized);
    assert_impl_all!(<TuplePacket as Wide>::Data: Sized);
}

#[test]
fn struct_wide_raw_parts_round_trip() {
    let values: [u8; 3] = [1, 2, 3];
    let bytes = unsafe {
        <Bytes as Wide>::from_raw_parts(
            values.as_ptr().cast::<<Bytes as Wide>::Data>(),
            values.len(),
        )
    };
    assert_eq!(&bytes.0, &values);

    let mut values: [u8; 3] = [1, 2, 3];
    let bytes = unsafe {
        <Bytes as Wide>::from_raw_parts_mut(
            values.as_mut_ptr().cast::<<Bytes as Wide>::Data>(),
            values.len(),
        )
    };
    bytes.0[1] = 9;
    assert_eq!(values, [1, 9, 3]);

    let values = vec![4u8, 5, 6].into_boxed_slice();
    let len = values.len();
    let data = Box::into_raw(values).cast::<<Bytes as Wide>::Data>();
    let bytes =
        unsafe { <Bytes as Wide>::from_non_null(core::ptr::NonNull::new_unchecked(data), len) };
    let metadata = bytes.metadata();
    let data = bytes.into_non_null();
    let bytes = unsafe { <Bytes as Wide>::from_non_null(data, metadata) };
    assert_eq!(&bytes.0, &[4, 5, 6]);
}
