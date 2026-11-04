use co3::{
    ReprC,
    borrow::Borrow,
    reference::{CRef, CRefMut},
    rust_spec::RustSpec,
    stored::{DecodeMove, DecodeMut, DecodeRef, Store},
};
use static_assertions::assert_not_impl_any;

// Reference-only downstream decoding must not require Borrow or DecodeMove.
#[derive(RustSpec)]
struct ReferenceOnly(u8);
impl ReprC for ReferenceOnly {
    type CType = u8;
}

#[derive(Default)]
struct SharedStore(Option<ReferenceOnly>);
impl Store for SharedStore {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

unsafe impl<'d> DecodeRef<'d> for ReferenceOnly {
    type Store = SharedStore;

    unsafe fn decode_ref<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut SharedStore,
    ) -> Option<&'d Self> {
        let value = *unsafe { source.into_ref()? };
        if value == 0 {
            return None;
        }
        Some(store.0.insert(Self(value)))
    }

    unsafe fn decode_ref_unchecked<'itm: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'itm mut SharedStore,
    ) -> &'d Self {
        let value = *unsafe { source.into_ref().unwrap_unchecked() };
        store.0.insert(Self(value))
    }
}

#[derive(Default)]
struct MutableStore {
    source: Option<core::ptr::NonNull<u8>>,
    value: Option<ReferenceOnly>,
}
impl Store for MutableStore {
    fn sync(self) -> Option<()> {
        if let (Some(source), Some(value)) = (self.source, self.value) {
            // The decode contract keeps the foreign storage exclusively accessible through sync.
            unsafe { source.as_ptr().write(value.0) };
        }
        Some(())
    }
}

unsafe impl<'d> DecodeMut<'d> for ReferenceOnly {
    type Store = MutableStore;

    unsafe fn decode_mut<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = unsafe { source.into_mut()? };
        if *source == 0 {
            return None;
        }
        let value = ReferenceOnly(*source);
        store.source = Some(core::ptr::NonNull::from(source));
        Some(store.value.insert(value))
    }

    unsafe fn decode_mut_unchecked<'itm: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'itm mut Self::Store,
    ) -> &'d mut Self {
        let source = unsafe { source.into_mut().unwrap_unchecked() };
        let value = ReferenceOnly(*source);
        store.source = Some(core::ptr::NonNull::from(source));
        store.value.insert(value)
    }
}

#[test]
fn downstream_shared_decoder_is_used_without_owned_decoding() {
    assert_not_impl_any!(ReferenceOnly: Borrow, DecodeMove<'static>);
    let source = 7;
    let mut store = Default::default();
    let value =
        unsafe { co3::soft_decode::<&ReferenceOnly>(CRef::from_ref(&source), &mut store) }.unwrap();
    assert_eq!(value.0, 7);
    store.sync().unwrap();

    let mut store = Default::default();
    assert!(unsafe { co3::soft_decode::<&ReferenceOnly>(CRef::null(), &mut store) }.is_none());
    let mut store = Default::default();
    let zero = 0;
    assert!(
        unsafe { co3::soft_decode::<&ReferenceOnly>(CRef::from_ref(&zero), &mut store) }.is_none()
    );
    // Exercise unchecked dispatch with a valid source as well.
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode_unchecked::<&ReferenceOnly>(CRef::from_ref(&source), &mut store)
    };
    assert_eq!(value.0, 7);
}

#[test]
fn downstream_mutable_decoder_synchronizes_the_source() {
    for unchecked in [false, true] {
        let mut source = 7;
        let mut store = Default::default();
        let carrier = CRefMut::from_mut(&mut source);
        let value = if unchecked {
            unsafe { co3::soft_decode_unchecked::<&mut ReferenceOnly>(carrier, &mut store) }
        } else {
            unsafe { co3::soft_decode::<&mut ReferenceOnly>(carrier, &mut store) }.unwrap()
        };
        value.0 = 11;
        store.sync().unwrap();
        assert_eq!(source, 11);
    }
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&mut ReferenceOnly>(CRefMut::null(), &mut store) }.is_none()
    );
}

#[test]
fn stable_references_keep_the_direct_decoding_path() {
    assert_not_impl_any!(u8: DecodeRef<'static>, DecodeMut<'static>);
    let source = 7u8;
    let mut store = ();
    let value = unsafe { co3::soft_decode::<&u8>(CRef::from_ref(&source), &mut store) }.unwrap();
    assert!(core::ptr::eq(value, &source));
    let mut source = 7u8;
    let original = core::ptr::from_mut(&mut source);
    let mut store = ();
    let value =
        unsafe { co3::soft_decode::<&mut u8>(CRefMut::from_mut(&mut source), &mut store) }.unwrap();
    assert_eq!(core::ptr::from_mut(value), original);
    *value = 9;
    assert_eq!(source, 9);
}

#[cfg(feature = "derive")]
#[derive(RustSpec, co3::ReprC, Debug, PartialEq)]
struct Derived(u8, bool);

#[cfg(feature = "derive")]
#[test]
fn derived_unstable_referents_use_temporary_storage_and_writeback() {
    let mut carrier = co3::encode(Derived(3, true));
    let mut store = Default::default();
    let value =
        unsafe { co3::soft_decode::<&Derived>(CRef::from_ref(&carrier), &mut store) }.unwrap();
    assert_eq!(value, &Derived(3, true));
    store.sync().unwrap();

    let mut store = Default::default();
    let value =
        unsafe { co3::soft_decode::<&mut Derived>(CRefMut::from_mut(&mut carrier), &mut store) }
            .unwrap();
    value.0 = 9;
    value.1 = false;
    store.sync().unwrap();
    assert_eq!(
        unsafe { co3::decode::<Derived>(carrier) },
        Some(Derived(9, false))
    );
}

#[test]
fn stable_wide_referents_bypass_decode_traits() {
    assert_not_impl_any!([u8]: DecodeRef<'static>, DecodeMut<'static>);
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
    let carrier = co3::tuple::Tuple2(7u8, co3::primitives::CBool(2));
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&(u8, bool)>(CRef::from_ref(&carrier), &mut store) }.is_none()
    );
}

#[test]
fn stable_mutable_encoder_preserves_pointer_identity() {
    use co3::stored::{EncodeMut, EncodeMutUnchecked};
    static_assertions::assert_impl_all!(u32: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!([u32; 2]: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(co3::vec::CVec<u8>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
    static_assertions::assert_impl_all!(co3::boxed::CBox<u8>: EncodeMut<'static>, EncodeMutUnchecked<'static>);
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
    use co3::{primitives::CBool, tuple::Tuple2};
    let carrier = co3::vec::CVec::from_vec(vec![Tuple2(7u8, CBool::TRUE)]);
    for unchecked in [false, true] {
        let mut store = Default::default();
        let value = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&Vec<(u8, bool)>>(CRef::from_ref(&carrier), &mut store)
            }
        } else {
            unsafe { co3::soft_decode::<&Vec<(u8, bool)>>(CRef::from_ref(&carrier), &mut store) }
                .unwrap()
        };
        assert_eq!(value, &[(7, true)]);
        assert_eq!(store.sync(), Some(()));
        let mut store = Default::default();
        let value = unsafe {
            co3::soft_decode::<&co3::vec::Vec<(u8, bool)>>(CRef::from_ref(&carrier), &mut store)
        }
        .unwrap();
        assert_eq!(&**value, &[(7, true)]);
        assert_eq!(store.sync(), Some(()));
    }
    let original = unsafe { carrier.into_vec() }.unwrap();
    assert_eq!(original[0].0, 7);
    assert_eq!(original[0].1, CBool::TRUE);

    let carrier = co3::vec::CVec::from_vec(vec![Tuple2(7u8, CBool(2))]);
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&Vec::<(u8, bool)>>(CRef::from_ref(&carrier), &mut store) }
            .is_none()
    );
    assert_eq!(store.sync(), Some(()));
    drop(unsafe { carrier.into_vec() }.unwrap());
}

#[test]
fn shared_box_decoding_preserves_foreign_ownership() {
    use co3::{primitives::CBool, tuple::Tuple2};
    let carrier = co3::boxed::CBox::from_box(Box::new(Tuple2(9u8, CBool::TRUE)));
    for unchecked in [false, true] {
        let mut store = Default::default();
        let value = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&Box<(u8, bool)>>(CRef::from_ref(&carrier), &mut store)
            }
        } else {
            unsafe { co3::soft_decode::<&Box<(u8, bool)>>(CRef::from_ref(&carrier), &mut store) }
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
    use co3::{primitives::CBool, tuple::Tuple2};
    for unchecked in [false, true] {
        let mut carrier = [Tuple2(1u8, CBool::TRUE), Tuple2(2, CBool::FALSE)];
        let mut shared = Default::default();
        let values = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&[(u8, bool); 2]>(
                    CRef::from_ref(&carrier),
                    &mut shared,
                )
            }
        } else {
            unsafe { co3::soft_decode::<&[(u8, bool); 2]>(CRef::from_ref(&carrier), &mut shared) }
                .unwrap()
        };
        assert_eq!(values, &[(1, true), (2, false)]);
        assert_eq!(shared.sync(), Some(()));

        let mut mutable = Default::default();
        let values = if unchecked {
            unsafe {
                co3::soft_decode_unchecked::<&mut [(u8, bool); 2]>(
                    CRefMut::from_mut(&mut carrier),
                    &mut mutable,
                )
            }
        } else {
            unsafe {
                co3::soft_decode::<&mut [(u8, bool); 2]>(
                    CRefMut::from_mut(&mut carrier),
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

    let mut carrier = [Tuple2(1u8, CBool(2))];
    let mut shared = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&[(u8, bool); 1]>(CRef::from_ref(&carrier), &mut shared) }
            .is_none()
    );
    let mut mutable = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&mut [(u8, bool); 1]>(CRefMut::from_mut(&mut carrier), &mut mutable)
        }
        .is_none()
    );
    assert_eq!(mutable.sync(), None);
    assert_eq!(carrier[0].1.0, 2);
}

#[test]
fn shared_enum_decoders_handle_tags_and_niches() {
    use co3::{option::COption, primitives::CBool, result::CResult, tuple::Tuple2};

    let tagged = COption::Some(Tuple2(3u8, 5u8));
    let mut store = Default::default();
    let decoded =
        unsafe { co3::soft_decode::<&Option<(u8, u8)>>(CRef::from_ref(&tagged), &mut store) }
            .unwrap();
    assert_eq!(decoded, &Some((3, 5)));
    store.sync().unwrap();

    let tagged = COption::Some(Tuple2(3u8, CBool::TRUE));
    let mut store = Default::default();
    let decoded = unsafe {
        co3::soft_decode_unchecked::<&co3::option::Option<(u8, bool)>>(
            CRef::from_ref(&tagged),
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
            co3::soft_decode::<&Option<(bool, u8)>>(CRef::from_ref(&carrier), &mut store)
        }
        .unwrap();
        assert_eq!(*decoded, expected);
        store.sync().unwrap();
    }

    let carrier: CResult<(u8, bool), u8> = CResult::Err(7);
    let mut store = Default::default();
    let decoded = unsafe {
        co3::soft_decode::<&Result<(u8, bool), u8>>(CRef::from_ref(&carrier), &mut store)
    }
    .unwrap();
    assert_eq!(decoded, &Err(7));
    store.sync().unwrap();
    let mut store = Default::default();
    let decoded = unsafe {
        co3::soft_decode_unchecked::<&co3::result::Result<(u8, bool), u8>>(
            CRef::from_ref(&carrier),
            &mut store,
        )
    };
    assert_eq!(decoded, &co3::result::Result::Err(7));
    store.sync().unwrap();

    for value in [Ok((true, 4u8)), Err(())] {
        let carrier = co3::encode(value);
        let mut store = Default::default();
        let decoded = unsafe {
            co3::soft_decode::<&Result<(bool, u8), ()>>(CRef::from_ref(&carrier), &mut store)
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
                CRef::from_ref(&carrier),
                &mut store,
            )
        };
        assert_eq!(*decoded, value);
        store.sync().unwrap();
    }

    // Invalid active payloads and tags are rejected without inspecting an inactive payload.
    let invalid = COption::Some(Tuple2(3u8, CBool(3)));
    let mut store = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&co3::option::Option<(u8, bool)>>(
                CRef::from_ref(&invalid),
                &mut store,
            )
        }
        .is_none()
    );
    let mut invalid = COption::<(u8, bool)>::None();
    unsafe {
        core::ptr::from_mut(&mut invalid).cast::<u8>().write(2);
    }
    let mut store = Default::default();
    assert!(
        unsafe {
            co3::soft_decode::<&co3::option::Option<(u8, bool)>>(
                CRef::from_ref(&invalid),
                &mut store,
            )
        }
        .is_none()
    );
}

#[test]
fn transparent_reference_decoders_delegate_to_inner_storage() {
    use core::mem::ManuallyDrop;

    use co3::borrow::Borrowed;
    let carrier = co3::encode((3u8, true));
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode::<&ManuallyDrop<(u8, bool)>>(CRef::from_ref(&carrier), &mut store)
    }
    .unwrap();
    assert_eq!(**value, (3, true));
    store.sync().unwrap();

    let carrier = carrier.into();
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode_unchecked::<&Borrowed<(u8, bool)>>(CRef::from_ref(&carrier), &mut store)
    };
    assert_eq!(Borrowed::into_inner(*value), (3, true));
    store.sync().unwrap();
}

#[test]
fn shared_decoders_do_not_reconstruct_cells_or_exclusive_references() {
    use core::cell::{Cell, UnsafeCell};
    assert_not_impl_any!(Cell<(u8, bool)>: DecodeRef<'static>);
    assert_not_impl_any!(UnsafeCell<(u8, bool)>: DecodeRef<'static>);
    assert_not_impl_any!(&'static mut (u8, bool): DecodeRef<'static>);
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
            unsafe { co3::soft_decode::<&Message>(CRef::from_ref(&carrier), &mut store) }.unwrap();
        assert_eq!(value, &expected);
        store.sync().unwrap();
        let mut store = Default::default();
        let value =
            unsafe { co3::soft_decode_unchecked::<&Message>(CRef::from_ref(&carrier), &mut store) };
        assert_eq!(value, &expected);
        store.sync().unwrap();
    }

    #[derive(RustSpec, co3::ReprC)]
    #[repr_c(is_valid = |value| *value != 0)]
    struct NonZero(u8);
    let carrier = CNonZero(0);
    let mut store = Default::default();
    assert!(
        unsafe { co3::soft_decode::<&NonZero>(CRef::from_ref(&carrier), &mut store) }.is_none()
    );
}

#[cfg(feature = "derive")]
#[test]
fn mutable_aggregates_accept_noncopy_nonclone_fields() {
    assert_not_impl_any!(Derived: Copy, Clone);
    let mut carrier = co3::encode([Derived(1, true), Derived(2, false)]);
    let mut store = Default::default();
    let values = unsafe {
        co3::soft_decode::<&mut [Derived; 2]>(CRefMut::from_mut(&mut carrier), &mut store)
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
        co3::soft_decode_unchecked::<&mut (Derived,)>(CRefMut::from_mut(&mut carrier), &mut store)
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
            CRefMut::from_mut(&mut carrier),
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
    let mut carrier = co3::tuple::Tuple1(CRefMut::from_mut(&mut target));
    let mut store = Default::default();
    let value = unsafe {
        co3::soft_decode::<&mut (&mut u32,)>(CRefMut::from_mut(&mut carrier), &mut store)
    }
    .unwrap();
    *value.0 = 17;
    store.sync().unwrap();
    assert_eq!(target, 17);
    assert_eq!(carrier.0.as_ptr(), core::ptr::from_mut(&mut target));
}
