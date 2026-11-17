use co3::{
    reference::{CRef, CSoftRef, CSoftRefMut},
    rust_spec::RustSpec,
    stored::{SoftDecodeMut, SoftDecodeRef, Store},
};
use static_assertions::assert_not_impl_any;

#[test]
fn stable_references_keep_the_direct_decoding_path() {
    assert_not_impl_any!(u8: SoftDecodeRef<'static>, SoftDecodeMut<'static>);
    let source = 7u8;
    let mut store = ();
    let value = unsafe { co3::soft_decode::<&u8>(CRef::from_ref(&source), &mut store) }.unwrap();
    assert!(core::ptr::eq(value, &source));
    let mut source = 7u8;
    let original = core::ptr::from_mut(&mut source);
    let mut store = ();
    let value = unsafe {
        co3::soft_decode::<&mut u8>(co3::reference::CRefMut::from_mut(&mut source), &mut store)
    }
    .unwrap();
    assert_eq!(core::ptr::from_mut(value), original);
    *value = 9;
    assert_eq!(source, 9);
}

#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct Derived(u8, bool);

#[test]
fn derived_unstable_referents_use_temporary_storage_and_writeback() {
    let mut carrier = co3::encode(Derived(3, true));
    let mut store = Default::default();
    let value =
        unsafe { co3::soft_decode::<&Derived>(CSoftRef::from_ref(&carrier), &mut store) }.unwrap();
    assert_eq!(value, &Derived(3, true));
    store.sync().unwrap();

    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode::<&mut Derived>(CSoftRefMut::from_mut(&mut carrier), &mut store)
    }
    .unwrap();
    value.0 = 9;
    value.1 = false;
    store.sync().unwrap();
    assert_eq!(
        unsafe { co3::decode::<Derived>(carrier) },
        Some(Derived(9, false))
    );
}

#[derive(RustSpec, co3::ReprC)]
#[repr(transparent)]
struct TransparentTuple((u8,));

fn read_transparent_tuple(value: &TransparentTuple) -> u8 {
    value.0.0
}

co3::raw! {
    fn read_transparent_tuple(#[soft] value: &TransparentTuple) -> u8;
}

#[test]
fn transparent_shared_decoder_reconstructs_the_inner_field() {
    use co3::ops::CFn1;
    type Callback<'a> = unsafe extern "C" fn(co3::borrow::CBorrowed<&'a TransparentTuple>) -> u8;
    let callback: Callback<'_> = read_transparent_tuple_raw;
    let value: Option<u8> = unsafe { callback.call(&TransparentTuple((17,))) };
    assert_eq!(value, Some(17));
}

#[test]
fn stable_wide_referents_bypass_decode_traits() {
    assert_not_impl_any!([u8]: SoftDecodeRef<'static>, SoftDecodeMut<'static>);
    let mut source = [3u8, 7];
    let carrier = co3::wide::CWideRef::from_ref(source.as_slice());
    let value = unsafe { co3::decode::<&[u8]>(carrier) }.unwrap();
    assert_eq!(value.as_ptr(), source.as_ptr());
    assert_eq!(value, &[3, 7]);
    let carrier = co3::wide::CWideRefMut::from_mut(source.as_mut_slice());
    let value = unsafe { co3::decode::<&mut [u8]>(carrier) }.unwrap();
    value[1] = 9;
    assert_eq!(source, [3, 9]);
}

#[test]
fn unstable_shared_decoder_rejects_invalid_fields() {
    let carrier = co3::tuple::CTuple2(7u8, co3::primitives::CBool(2));
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&(u8, bool)>(CSoftRef::from_ref(&carrier), &mut store) }
            .is_none()
    );
}

#[test]
fn stable_mutable_encoder_preserves_pointer_identity() {
    use co3::stored::{SoftEncodeMut, SoftEncodeMutUnchecked};
    static_assertions::assert_impl_all!(u32: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!([u32; 2]: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(co3::vec::CVec<u8>: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(co3::boxed::CBox<u8>: SoftEncodeMut<'static>, SoftEncodeMutUnchecked<'static>);
    let mut value = 7_u32;
    let original = core::ptr::from_mut(&mut value);
    let encoded = co3::soft_encode(&mut value, &mut Default::default());
    assert_eq!(encoded.as_ptr(), original);

    let mut values = [1_u32, 2];
    let original = values.as_mut_ptr();
    let encoded = co3::encode(values.as_mut_slice());
    assert_eq!(encoded.as_ptr(), original);
    assert_eq!(encoded.metadata(), 2);
}

#[test]
fn mutable_array_with_exclusive_reference_writes_back() {
    let mut value = false;
    let mut values = [&mut value];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    unsafe {
        (*encoded.as_ptr())[0]
            .as_ptr()
            .write(co3::primitives::CBool(1))
    };
    assert_eq!(store.sync(), Some(()));
    assert!(value);
}

#[test]
fn mutable_vec_with_exclusive_reference_writes_back() {
    let mut value = false;
    let mut values = co3::vec::Vec::with_capacity(4);
    values.push(&mut value);
    let capacity = values.capacity();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let vector = unsafe { *encoded.as_ptr() };
    let field = unsafe { *vector.as_ptr() };
    unsafe { field.as_ptr().write(co3::primitives::CBool(1)) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values.capacity(), capacity);

    let encoded = unsafe { co3::soft_encode_unchecked(&mut values, &mut ()) };
    let vector = unsafe { *encoded.as_ptr() };
    let field = unsafe { *vector.as_ptr() };
    unsafe { field.as_ptr().write(co3::primitives::CBool(0)) };
    assert_eq!(().sync(), Some(()));
    assert_eq!(values.capacity(), capacity);
    drop(values);
    assert!(!value);
}

#[test]
fn mutable_str_validates_before_writeback() {
    let mut value = String::from("abc");
    let mut store = Default::default();
    let encoded = co3::soft_encode(value.as_mut_str(), &mut store);
    unsafe { encoded.as_ptr().write(b'z') };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, "zbc");

    let mut store = Default::default();
    let encoded = co3::soft_encode(value.as_mut_str(), &mut store);
    unsafe { encoded.as_ptr().write(0xff) };
    assert_eq!(store.sync(), None);
    assert_eq!(value, "zbc");
}

#[test]
fn mutable_nonrobust_slice_validates_before_writeback() {
    let mut values = [false, true];
    let mut store = Default::default();
    let encoded = co3::soft_encode(values.as_mut_slice(), &mut store);
    unsafe { encoded.as_ptr().write(co3::primitives::CBool(2)) };
    assert_eq!(store.sync(), None);
    assert_eq!(values, [false, true]);
}

#[test]
fn mutable_nonrobust_array_validates_before_writeback() {
    let mut values = [false, true];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    unsafe { (*encoded.as_ptr())[0] = co3::primitives::CBool(2) };
    assert_eq!(store.sync(), None);
    assert_eq!(values, [false, true]);
}

#[test]
fn standard_mutable_vec_keeps_original_references() {
    let mut value = false;
    let mut values = Vec::with_capacity(7);
    values.push(&mut value);
    let original = values.as_ptr();
    let capacity = values.capacity();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_eq!(header.len(), 1);
    assert_eq!(header.capacity(), capacity);
    unsafe { (*header.as_ptr()).as_ptr().write(co3::primitives::CBool(1)) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values.as_ptr(), original);
    assert!(**values.first().unwrap());

    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(&mut values, &mut store) };
    let header = unsafe { *encoded.as_ptr() };
    unsafe { (*header.as_ptr()).as_ptr().write(co3::primitives::CBool(0)) };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values.as_ptr(), original);
    assert_eq!(values.capacity(), capacity);
    drop(values);
    assert!(!value);
}

#[test]
fn mutable_vec_rejects_invalid_values_and_header_replacement() {
    let mut values = co3::vec::Vec::with_capacity(5);
    values.push(true);
    let original = values.as_ptr();
    let capacity = values.capacity();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    unsafe { header.as_ptr().write(co3::primitives::CBool(2)) };
    assert_eq!(store.sync(), None);
    assert_eq!(&*values, &[true]);
    assert_eq!(values.as_ptr(), original);

    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    unsafe { encoded.as_ptr().write(co3::vec::CVec::null()) };
    assert_eq!(store.sync(), None);
    assert_eq!(&*values, &[true]);
    assert_eq!(values.as_ptr(), original);
    assert_eq!(values.capacity(), capacity);
}

#[test]
fn mutable_vec_preserves_empty_and_zero_sized_capacity() {
    let mut empty = co3::vec::Vec::<u32>::with_capacity(9);
    let capacity = empty.capacity();
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut empty, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_eq!(header.len(), 0);
    assert_eq!(header.capacity(), capacity);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(empty.capacity(), capacity);

    let mut values = co3::vec::Vec::from(vec![(); 3]);
    let mut store = Default::default();
    let encoded = co3::soft_encode(&mut values, &mut store);
    let header = unsafe { *encoded.as_ptr() };
    assert_eq!(header.len(), 3);
    assert_eq!(header.capacity(), usize::MAX);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(values.len(), 3);
}

#[test]
fn shared_vector_decoding_preserves_foreign_ownership() {
    use co3::{primitives::CBool, tuple::CTuple2};
    let carrier = co3::vec::CVec::from_vec(vec![CTuple2(7u8, CBool::TRUE)]);
    for unchecked in [false, true] {
        let mut store = Default::default();
        let value = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&Vec<(u8, bool)>>(
                    CSoftRef::from_ref(&carrier),
                    &mut store,
                )
            }
        } else {
            unsafe {
                co3::soft_decode::<&Vec<(u8, bool)>>(CSoftRef::from_ref(&carrier), &mut store)
            }
            .unwrap()
        };
        assert_eq!(value, &[(7, true)]);
        assert_eq!(store.sync(), Some(()));
        let mut store = Default::default();
        let stable_carrier = unsafe {
            co3::vec::CVec::<(u8, bool), true>::from_raw_parts(
                carrier.as_ptr(),
                carrier.len(),
                carrier.capacity(),
            )
        };
        let value = unsafe {
            co3::soft_decode::<&co3::vec::Vec<(u8, bool)>>(
                CSoftRef::from_ref(&stable_carrier),
                &mut store,
            )
        }
        .unwrap();
        assert_eq!(&**value, &[(7, true)]);
        assert_eq!(store.sync(), Some(()));
    }
    let original = unsafe { carrier.into_vec() }.unwrap();
    assert_eq!(original[0].0, 7);
    assert_eq!(original[0].1, CBool::TRUE);

    let carrier = co3::vec::CVec::from_vec(vec![CTuple2(7u8, CBool(2))]);
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&Vec<(u8, bool)>>(CSoftRef::from_ref(&carrier), &mut store) }
            .is_none()
    );
    assert_eq!(store.sync(), Some(()));
    drop(unsafe { carrier.into_vec() }.unwrap());
}

#[test]
fn shared_box_decoding_preserves_foreign_ownership() {
    use co3::{primitives::CBool, tuple::CTuple2};
    let carrier = co3::boxed::CBox::from_box(Box::new(CTuple2(9u8, CBool::TRUE)));
    for unchecked in [false, true] {
        let mut store = Default::default();
        let value = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&Box<(u8, bool)>>(
                    CSoftRef::from_ref(&carrier),
                    &mut store,
                )
            }
        } else {
            unsafe {
                co3::soft_decode::<&Box<(u8, bool)>>(CSoftRef::from_ref(&carrier), &mut store)
            }
            .unwrap()
        };
        assert_eq!(**value, (9, true));
        assert_eq!(store.sync(), Some(()));
    }
    let original = unsafe { carrier.into_box() }.unwrap();
    assert_eq!(original.0, 9);
    assert_eq!(original.1, CBool::TRUE);
}

#[test]
fn array_decoders_validate_elements_and_write_back() {
    use co3::{primitives::CBool, tuple::CTuple2};
    for unchecked in [false, true] {
        let mut carrier = [CTuple2(1u8, CBool::TRUE), CTuple2(2, CBool::FALSE)];
        let mut shared = Default::default();
        let values = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&[(u8, bool); 2]>(
                    CSoftRef::from_ref(&carrier),
                    &mut shared,
                )
            }
        } else {
            unsafe {
                co3::soft_decode::<&[(u8, bool); 2]>(CSoftRef::from_ref(&carrier), &mut shared)
            }
            .unwrap()
        };
        assert_eq!(values, &[(1, true), (2, false)]);
        assert_eq!(shared.sync(), Some(()));

        let mut mutable = Default::default();
        let values = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&mut [(u8, bool); 2]>(
                    CSoftRefMut::from_mut(&mut carrier),
                    &mut mutable,
                )
            }
        } else {
            unsafe {
                co3::soft_decode::<&mut [(u8, bool); 2]>(
                    CSoftRefMut::from_mut(&mut carrier),
                    &mut mutable,
                )
            }
            .unwrap()
        };
        values[0] = (8, false);
        values[1] = (9, true);
        assert_eq!(mutable.sync(), Some(()));
        assert_eq!(carrier[0].0, 8);
        assert_eq!(carrier[0].1, CBool::FALSE);
        assert_eq!(carrier[1].0, 9);
        assert_eq!(carrier[1].1, CBool::TRUE);
    }

    let mut carrier = [CTuple2(1u8, CBool(2))];
    let mut shared = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&[(u8, bool); 1]>(CSoftRef::from_ref(&carrier), &mut shared) }
            .is_none()
    );
    let mut mutable = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&mut [(u8, bool); 1]>(
                CSoftRefMut::from_mut(&mut carrier),
                &mut mutable,
            )
        }
        .is_none()
    );
    assert_eq!(mutable.sync(), None);
    assert_eq!(carrier[0].1.0, 2);
}

#[test]
fn shared_enum_decoders_handle_tags_and_niches() {
    use co3::{option::COption, primitives::CBool, result::CResult, tuple::CTuple2};

    let tagged = COption::<(u8, u8)>::Some(CTuple2(3u8, 5u8));
    let mut store = Default::default();
    let decoded =
        unsafe { co3::soft_decode::<&Option<(u8, u8)>>(CSoftRef::from_ref(&tagged), &mut store) }
            .unwrap();
    assert_eq!(decoded, &Some((3, 5)));
    store.sync().unwrap();

    let tagged = COption::<(u8, bool), true>::Some(CTuple2(3u8, CBool::TRUE));
    let mut store = Default::default();
    let decoded = unsafe {
        co3::soft_decode_unchecked::<&co3::option::Option<(u8, bool)>>(
            CSoftRef::from_ref(&tagged),
            &mut store,
        )
    };
    assert_eq!(decoded, &co3::option::Option::Some((3, true)));
    store.sync().unwrap();

    // bool has a niche, so Option<(bool, u8)> has no extra discriminant.
    let none = co3::encode(None::<(bool, u8)>);
    let some = co3::encode(Some((true, 9u8)));
    for (carrier, expected) in [(none, None), (some, Some((true, 9)))] {
        let mut store = Default::default();
        let decoded = unsafe {
            co3::soft_decode::<&Option<(bool, u8)>>(CSoftRef::from_ref(&carrier), &mut store)
        }
        .unwrap();
        assert_eq!(*decoded, expected);
        store.sync().unwrap();
    }

    let carrier: CResult<(u8, bool), u8> = CResult::Err(7);
    let mut store = Default::default();
    let decoded = unsafe {
        co3::soft_decode::<&Result<(u8, bool), u8>>(CSoftRef::from_ref(&carrier), &mut store)
    }
    .unwrap();
    assert_eq!(decoded, &Err(7));
    store.sync().unwrap();
    let mut store = Default::default();
    let stable_carrier = CResult::<(u8, bool), u8, true>::Err(7);
    let decoded = unsafe {
        co3::soft_decode_unchecked::<&co3::result::Result<(u8, bool), u8>>(
            CSoftRef::from_ref(&stable_carrier),
            &mut store,
        )
    };
    assert_eq!(decoded, &co3::result::Result::Err(7));
    store.sync().unwrap();

    for value in [Ok((true, 4u8)), Err(())] {
        let carrier = co3::encode(value);
        let mut store = Default::default();
        let decoded = unsafe {
            co3::soft_decode::<&Result<(bool, u8), ()>>(CSoftRef::from_ref(&carrier), &mut store)
        }
        .unwrap();
        assert_eq!(*decoded, value);
        store.sync().unwrap();
    }
    for value in [Ok(()), Err((true, 4u8))] {
        let carrier = co3::encode(value);
        let mut store = Default::default();
        let decoded = unsafe {
            co3::soft_decode_unchecked::<&Result<(), (bool, u8)>>(
                CSoftRef::from_ref(&carrier),
                &mut store,
            )
        };
        assert_eq!(*decoded, value);
        store.sync().unwrap();
    }

    // Invalid active payloads and tags are rejected without inspecting an inactive payload.
    let invalid = COption::Some(CTuple2(3u8, CBool(3)));
    let mut store = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&co3::option::Option<(u8, bool)>>(
                CSoftRef::from_ref(&invalid),
                &mut store,
            )
        }
        .is_none()
    );
    let mut invalid = COption::<(u8, bool), true>::None();
    unsafe {
        core::ptr::from_mut(&mut invalid).cast::<u8>().write(2);
    }
    let mut store = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&co3::option::Option<(u8, bool)>>(
                CSoftRef::from_ref(&invalid),
                &mut store,
            )
        }
        .is_none()
    );
}

#[test]
fn manually_drop_reference_decoder_delegates_to_inner_storage() {
    use core::mem::ManuallyDrop;

    let carrier = co3::encode(ManuallyDrop::new((3u8, true)));
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode::<&ManuallyDrop<(u8, bool)>>(CSoftRef::from_ref(&carrier), &mut store)
    }
    .unwrap();
    assert_eq!(**value, (3, true));
    store.sync().unwrap();

    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode_unchecked::<&ManuallyDrop<(u8, bool)>>(
            CSoftRef::from_ref(&carrier),
            &mut store,
        )
    };
    assert_eq!(**value, (3, true));
    store.sync().unwrap();

    let invalid = core::mem::ManuallyDrop::new(co3::tuple::CTuple2(3u8, co3::primitives::CBool(3)));
    let mut store = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&ManuallyDrop<(u8, bool)>>(CSoftRef::from_ref(&invalid), &mut store)
        }
        .is_none()
    );
    assert!(
        unsafe { co3::soft_decode::<&ManuallyDrop<(u8, bool)>>(CSoftRef::null(), &mut store) }
            .is_none()
    );
}

#[test]
fn shared_decoders_do_not_reconstruct_cells_or_exclusive_references() {
    use core::cell::{Cell, UnsafeCell};
    assert_not_impl_any!(Cell<(u8, bool)>: SoftDecodeRef<'static>);
    assert_not_impl_any!(UnsafeCell<(u8, bool)>: SoftDecodeRef<'static>);
    assert_not_impl_any!(&'static mut (u8, bool): SoftDecodeRef<'static>);
}

#[cfg(feature = "derive")]
#[test]
fn derived_shared_decoders_validate_active_fields() {
    #[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
    enum Message {
        Empty,
        Pair(u8, bool),
        Named { value: (u8, bool) },
    }
    for expected in [
        Message::Empty,
        Message::Pair(4, true),
        Message::Named { value: (8, false) },
    ] {
        let carrier = match &expected {
            Message::Empty => co3::encode(Message::Empty),
            Message::Pair(a, b) => co3::encode(Message::Pair(*a, *b)),
            Message::Named { value } => co3::encode(Message::Named { value: *value }),
        };
        let mut store = Default::default();
        let value =
            unsafe { co3::soft_decode::<&Message>(CSoftRef::from_ref(&carrier), &mut store) }
                .unwrap();
        assert_eq!(value, &expected);
        store.sync().unwrap();
        let mut store = Default::default();
        let value = unsafe {
            co3::soft_decode_unchecked::<&Message>(CSoftRef::from_ref(&carrier), &mut store)
        };
        assert_eq!(value, &expected);
        store.sync().unwrap();
    }

    #[derive(RustSpec, co3::ReprC)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value| *value != 0)]
    struct NonZero(u8);
    let carrier = unsafe { <NonZero as co3::ReprC>::CType::from_fields(0) };
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&NonZero>(CSoftRef::from_ref(&carrier), &mut store) }.is_none()
    );
}

#[cfg(feature = "derive")]
#[test]
fn mutable_aggregates_accept_noncopy_nonclone_fields() {
    assert_not_impl_any!(Derived: Copy, Clone);
    let mut carrier = co3::encode([Derived(1, true), Derived(2, false)]);
    let mut store = Default::default();
    let values = unsafe {
        co3::soft_decode::<&mut [Derived; 2]>(CSoftRefMut::from_mut(&mut carrier), &mut store)
    }
    .unwrap();
    values[0].0 = 7;
    values[1].1 = true;
    store.sync().unwrap();
    assert_eq!(
        unsafe { co3::decode::<[Derived; 2]>(carrier) },
        Some([Derived(7, true), Derived(2, true)])
    );

    let mut carrier = co3::encode((Derived(3, true),));
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode_unchecked::<&mut (Derived,)>(
            CSoftRefMut::from_mut(&mut carrier),
            &mut store,
        )
    };
    value.0.0 = 9;
    store.sync().unwrap();
    assert_eq!(
        unsafe { co3::decode::<(Derived,)>(carrier) },
        Some((Derived(9, true),))
    );

    let mut carrier = co3::encode(co3::option::Option::Some(Derived(3, true)));
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode::<&mut co3::option::Option<Derived>>(
            CSoftRefMut::from_mut(&mut carrier),
            &mut store,
        )
    }
    .unwrap();
    if let co3::option::Option::Some(value) = value {
        value.0 = 11;
    }
    store.sync().unwrap();
    assert_eq!(
        unsafe { co3::decode::<co3::option::Option<Derived>>(carrier) },
        Some(co3::option::Option::Some(Derived(11, true)))
    );
}

#[test]
fn mutable_aggregate_fields_can_be_exclusive_references() {
    let mut target = 3u32;
    let pointer = core::ptr::from_mut(&mut target);
    let mut carrier = co3::tuple::CTuple1(co3::reference::CRefMut::from_mut(&mut target));
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode::<&mut (&mut u32,)>(CSoftRefMut::from_mut(&mut carrier), &mut store)
    }
    .unwrap();
    *value.0 = 17;
    store.sync().unwrap();
    assert_eq!(carrier.0.as_ptr(), pointer);
    assert_eq!(target, 17);
}
