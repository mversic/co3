use co3::{
    ReprC,
    borrow::Borrow,
    reference::{CRef, CRefMut},
    rust_spec::RustSpec,
    stored::{DecodeMut, DecodeOwned, DecodeRef, Store},
};
use static_assertions::assert_not_impl_any;

// Reference-only downstream decoding must not require Borrow or DecodeOwned.
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
    assert_not_impl_any!(ReferenceOnly: Borrow, DecodeOwned<'static>);
    let source = 7;
    let mut store = SharedStore::default();
    let value =
        unsafe { <&ReferenceOnly>::soft_decode(CRef::from_ref(&source), &mut store) }.unwrap();
    assert_eq!(value.0, 7);
    store.sync().unwrap();

    let mut store = SharedStore::default();
    assert!(unsafe { <&ReferenceOnly>::soft_decode(CRef::null(), &mut store) }.is_none());
    let mut store = SharedStore::default();
    let zero = 0;
    assert!(unsafe { <&ReferenceOnly>::soft_decode(CRef::from_ref(&zero), &mut store) }.is_none());
    // Exercise unchecked dispatch with a valid source as well.
    let mut store = SharedStore::default();
    let value =
        unsafe { <&ReferenceOnly>::soft_decode_unchecked(CRef::from_ref(&source), &mut store) };
    assert_eq!(value.0, 7);
}

#[test]
fn downstream_mutable_decoder_synchronizes_the_source() {
    for unchecked in [false, true] {
        let mut source = 7;
        let mut store = MutableStore::default();
        let carrier = CRefMut::from_mut(&mut source);
        let value = if unchecked {
            unsafe { <&mut ReferenceOnly>::soft_decode_unchecked(carrier, &mut store) }
        } else {
            unsafe { <&mut ReferenceOnly>::soft_decode(carrier, &mut store) }.unwrap()
        };
        value.0 = 11;
        store.sync().unwrap();
        assert_eq!(source, 11);
    }
    let mut store = MutableStore::default();
    assert!(unsafe { <&mut ReferenceOnly>::soft_decode(CRefMut::null(), &mut store) }.is_none());
}

#[test]
fn stable_references_keep_the_direct_decoding_path() {
    assert_not_impl_any!(u8: DecodeRef<'static>, DecodeMut<'static>);
    let source = 7u8;
    let mut store = ();
    let value = unsafe { <&u8>::soft_decode(CRef::from_ref(&source), &mut store) }.unwrap();
    assert!(core::ptr::eq(value, &source));
    let mut source = 7u8;
    let original = core::ptr::from_mut(&mut source);
    let mut store = ();
    let value =
        unsafe { <&mut u8>::soft_decode(CRefMut::from_mut(&mut source), &mut store) }.unwrap();
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
    let mut store = <Derived as DecodeRef>::Store::default();
    let value = unsafe { Derived::decode_ref(CRef::from_ref(&carrier), &mut store) }.unwrap();
    assert_eq!(value, &Derived(3, true));
    store.sync().unwrap();

    let mut store = <Derived as DecodeMut>::Store::default();
    let value =
        unsafe { Derived::decode_mut(CRefMut::from_mut(&mut carrier), &mut store) }.unwrap();
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
    let carrier = co3::slice::CWideRef::from_ref(source.as_slice());
    let value = unsafe { co3::decode::<&[u8]>(carrier) }.unwrap();
    assert_eq!(value.as_ptr(), source.as_ptr());
    assert_eq!(value, &[3, 7]);
    let carrier = co3::slice::CWideRefMut::from_mut(source.as_mut_slice());
    let value = unsafe { co3::decode::<&mut [u8]>(carrier) }.unwrap();
    value[1] = 9;
    assert_eq!(source, [3, 9]);
}

#[test]
fn unstable_shared_decoder_rejects_invalid_fields() {
    let carrier = co3::tuple::ReprCTuple2(7u8, co3::primitives::CBool(2));
    let mut store = <(u8, bool) as DecodeRef>::Store::default();
    assert!(unsafe { <(u8, bool)>::decode_ref(CRef::from_ref(&carrier), &mut store) }.is_none());
}
