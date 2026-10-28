//! Regression tests for paths that must not invoke field converters.
use crate::{
    ReprC,
    primitives::CBool,
    stored::{DecodeOwned, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked, Store},
    transmute::AbiCompatible,
};
use core::cell::{Cell, UnsafeCell};

#[derive(rust_spec::RustSpec)]
#[repr(transparent)]
struct Direct(u8);

impl ReprC for Direct {
    type CType = u8;
}
// SAFETY: Direct is a transparent, robust u8.
unsafe impl AbiCompatible for Direct {
    unsafe fn is_valid(_: &u8) -> bool {
        true
    }
}
// These deliberately panic: aggregate fast paths must not invoke them.
unsafe impl EncodeMoveUnchecked for Direct {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked(self, _: &mut ()) -> u8 {
        panic!("visited field")
    }
}
unsafe impl EncodeMove for Direct {
    type Store = ();
    fn soft_encode<'itm>(self, _: &mut ()) -> u8
    where
        Self: 'itm,
    {
        panic!("visited field")
    }
}
unsafe impl<'d> DecodeOwned<'d> for Direct {
    type Store = ();
    unsafe fn soft_decode<'itm: 'd>(_: u8, _: &'itm mut ()) -> Option<Self> {
        panic!("visited field")
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(_: u8, _: &'itm mut ()) -> Self {
        panic!("visited field")
    }
}

#[test]
fn stable_robust_arrays_move_without_visiting_fields() {
    assert_eq!([Direct(3), Direct(4)].soft_encode(&mut ()), [3, 4]);
    assert_eq!(
        unsafe { [Direct(5), Direct(6)].soft_encode_unchecked(&mut ()) },
        [5, 6]
    );
    let values = unsafe { <[Direct; 2] as DecodeOwned>::soft_decode([7, 8], &mut ()) }.unwrap();
    assert_eq!(values.map(|x| x.0), [7, 8]);
    let values = unsafe { <[Direct; 2] as DecodeOwned>::soft_decode_unchecked([9, 10], &mut ()) };
    assert_eq!(values.map(|x| x.0), [9, 10]);
}

#[test]
fn owned_cells_move_without_visiting_contents() {
    assert_eq!(Cell::new(Direct(3)).soft_encode(&mut ()).into_inner(), 3);
    assert_eq!(
        UnsafeCell::new(Direct(4)).soft_encode(&mut ()).into_inner(),
        4
    );
    let cell = unsafe { Cell::new(Direct(5)).soft_encode_unchecked(&mut ()) };
    assert_eq!(cell.into_inner(), 5);
    let cell =
        unsafe { <UnsafeCell<Direct> as DecodeOwned>::soft_decode(UnsafeCell::new(6), &mut ()) }
            .unwrap();
    assert_eq!(cell.into_inner().0, 6);
    let cell = unsafe {
        <Cell<Direct> as DecodeOwned>::soft_decode_unchecked(UnsafeCell::new(7), &mut ())
    };
    assert_eq!(cell.into_inner().0, 7);
    // Non-Copy carriers are moved, not read through &UnsafeCell.
    let cells = [UnsafeCell::new(Direct(8)), UnsafeCell::new(Direct(9))].soft_encode(&mut ());
    assert_eq!(cells.map(UnsafeCell::into_inner), [8, 9]);
}

#[test]
fn stable_shared_arrays_preserve_the_original_pointer() {
    let values = [Direct(3), Direct(4)];
    let encoded = values.encode_ref(&mut ());
    assert_eq!(encoded.as_ptr().cast::<u8>(), values.as_ptr().cast::<u8>());
    let encoded = unsafe { values.encode_ref_unchecked(&mut ()) };
    assert_eq!(encoded.as_ptr().cast::<u8>(), values.as_ptr().cast::<u8>());
}

#[test]
fn nonrobust_checked_conversions_still_validate() {
    let mut store = Default::default();
    assert!(
        unsafe { <[bool; 2] as DecodeOwned>::soft_decode([CBool(1), CBool(2)], &mut store) }
            .is_none()
    );
    assert!(
        unsafe { <Cell<bool> as DecodeOwned>::soft_decode(UnsafeCell::new(CBool(2)), &mut ()) }
            .is_none()
    );
    let mut store = Default::default();
    let values =
        unsafe { <[bool; 2] as DecodeOwned>::soft_decode([CBool(1), CBool(0)], &mut store) }
            .unwrap();
    assert_eq!(values, [true, false]);
}

#[test]
fn checked_arrays_keep_mutable_reference_writeback() {
    let mut value = true;
    let mut store = Default::default();
    let encoded = [&mut value].soft_encode(&mut store);
    // SAFETY: The mutable carrier refers to the live conversion store.
    unsafe { encoded[0].as_ptr().write(CBool::FALSE) };
    assert_eq!(store.sync(), Some(()));
    assert!(!value);
}

#[test]
fn unstable_arrays_keep_field_conversion_and_stores() {
    let values = [(true, 3u8), (false, 4u8)];
    let mut store = Default::default();
    let encoded = values.soft_encode(&mut store);
    assert_eq!(encoded[0].0, CBool::TRUE);
    assert_eq!(encoded[1].1, 4);
    let mut decode_store = Default::default();
    let decoded =
        unsafe { <[(bool, u8); 2] as DecodeOwned>::soft_decode(encoded, &mut decode_store) }
            .unwrap();
    assert_eq!(decoded, values);
}

#[test]
fn moving_values_does_not_drop_the_source_or_duplicate_ownership() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    #[repr(transparent)]
    struct Owned(u8);
    impl Drop for Owned {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }
    // SAFETY: Identity conversion transfers ownership of the same type.
    let moved: Owned = unsafe { crate::transmute::transmute_owned(Owned(7)) };
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(moved.0, 7);
    drop(moved);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[test]
fn unchecked_arrays_keep_shared_interior_references_without_validation() {
    use core::num::NonZeroU8;
    let cell = UnsafeCell::new(NonZeroU8::new(3).unwrap());
    let mut store = Default::default();
    let encoded = unsafe { [&cell].soft_encode_unchecked(&mut store) };
    assert_eq!(
        encoded[0].as_ptr().cast::<()>(),
        core::ptr::from_ref(&cell).cast_mut().cast::<()>()
    );
}

#[test]
fn manually_drop_robust_interior_decodes_without_visiting_payload() {
    use core::mem::ManuallyDrop;

    #[derive(rust_spec::RustSpec)]
    #[repr(transparent)]
    struct Interior(UnsafeCell<u8>);

    impl ReprC for Interior {
        type CType = UnsafeCell<u8>;
    }
    // SAFETY: Interior is a transparent wrapper around its carrier.
    unsafe impl AbiCompatible for Interior {
        unsafe fn is_valid(_: &Self::CType) -> bool {
            panic!("visited robust interior validator")
        }
    }
    unsafe impl<'d> DecodeOwned<'d> for Interior {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(_: Self::CType, _: &'itm mut ()) -> Option<Self> {
            panic!("visited interior decoder")
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(_: Self::CType, _: &'itm mut ()) -> Self {
            panic!("visited interior decoder")
        }
    }

    let decoded = unsafe {
        <ManuallyDrop<Interior> as DecodeOwned>::soft_decode(UnsafeCell::new(3), &mut ())
    }
    .unwrap();
    assert_eq!(ManuallyDrop::into_inner(decoded).0.into_inner(), 3);
    let decoded = unsafe {
        <ManuallyDrop<Interior> as DecodeOwned>::soft_decode_unchecked(UnsafeCell::new(4), &mut ())
    };
    assert_eq!(ManuallyDrop::into_inner(decoded).0.into_inner(), 4);

    // Non-robust interior still delegates to the payload's validating decoder.
    assert!(
        unsafe {
            <ManuallyDrop<Cell<bool>> as DecodeOwned>::soft_decode(
                UnsafeCell::new(CBool(2)),
                &mut (),
            )
        }
        .is_none()
    );
    let decoded = unsafe {
        <ManuallyDrop<Cell<bool>> as DecodeOwned>::soft_decode(UnsafeCell::new(CBool(1)), &mut ())
    }
    .unwrap();
    assert!(ManuallyDrop::into_inner(decoded).into_inner());
}
