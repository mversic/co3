use co3::{
    ReprC,
    borrow::Borrowed,
    reference::{CRef, CRefMut},
    stored::{
        EncodeMove, EncodeMoveUnchecked, EncodeMut, EncodeMutUnchecked, EncodeRef,
        EncodeRefUnchecked, Store,
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};

static CLONES: AtomicUsize = AtomicUsize::new(0);
static SYNCS: AtomicUsize = AtomicUsize::new(0);

#[derive(co3::rust_spec::RustSpec)]
#[rust_spec(custom_drop)]
struct Probe(u8);

impl Drop for Probe {
    fn drop(&mut self) {}
}

impl Clone for Probe {
    fn clone(&self) -> Self {
        CLONES.fetch_add(1, Ordering::Relaxed);
        Self(self.0)
    }
}

impl ReprC for Probe {
    type CType = u8;
}

#[derive(Default)]
struct ProbeStore;

impl Store for ProbeStore {
    fn sync(self) -> Option<()> {
        SYNCS.fetch_add(1, Ordering::Relaxed);
        Some(())
    }
}

unsafe impl EncodeMoveUnchecked for Probe {
    type SoftStore = ProbeStore;

    unsafe fn soft_encode_unchecked(self, _: &mut ProbeStore) -> u8 {
        self.0
    }
}

unsafe impl EncodeMove for Probe {
    type Store = ProbeStore;

    fn soft_encode<'itm>(self, _: &'itm mut ProbeStore) -> u8
    where
        Self: 'itm,
    {
        self.0
    }
}

// Borrow the actual byte: no snapshot, clone, or deferred writeback is needed.
unsafe impl<'a> EncodeRefUnchecked<'a> for Probe {
    type SoftStore = ProbeStore;

    unsafe fn encode_ref_unchecked(&'a self, _: &mut ProbeStore) -> <&'a Self as ReprC>::CType {
        CRef::from_ref(&self.0)
    }
}

unsafe impl<'a> EncodeRef<'a> for Probe {
    type Store = ProbeStore;

    fn encode_ref(&'a self, _: &mut ProbeStore) -> <&'a Self as ReprC>::CType {
        CRef::from_ref(&self.0)
    }
}

unsafe impl<'a> EncodeMutUnchecked<'a> for Probe {
    type SoftStore = ProbeStore;

    unsafe fn encode_mut_unchecked(
        &'a mut self,
        _: &mut ProbeStore,
    ) -> <&'a mut Self as ReprC>::CType {
        CRefMut::from_mut(&mut self.0)
    }
}

unsafe impl<'a> EncodeMut<'a> for Probe {
    type Store = ProbeStore;

    fn encode_mut(&'a mut self, _: &mut ProbeStore) -> <&'a mut Self as ReprC>::CType {
        CRefMut::from_mut(&mut self.0)
    }
}

#[test]
fn reference_and_borrowed_encoding_avoid_clones_and_sync_nested_state() {
    let mut store = Default::default();
    let value = Probe(7);
    let encoded = (&value).soft_encode(&mut store);
    assert_eq!(unsafe { *encoded.as_ptr() }, 7);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let value = Probe(8);
    let encoded = unsafe { (&value).soft_encode_unchecked(&mut store) };
    assert_eq!(unsafe { *encoded.as_ptr() }, 8);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    assert_eq!(
        Borrowed::into_inner(Borrowed::new(Probe(9)).soft_encode(&mut store)),
        9
    );
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { Borrowed::new(Probe(10)).soft_encode_unchecked(&mut store) };
    assert_eq!(Borrowed::into_inner(encoded), 10);
    assert_eq!(store.sync(), Some(()));

    let mut value = Probe(11);
    let mut store = Default::default();
    let encoded = (&mut value).soft_encode(&mut store);
    unsafe { *encoded.as_ptr() = 12 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value.0, 12);

    let mut store = Default::default();
    let encoded = unsafe { (&mut value).soft_encode_unchecked(&mut store) };
    unsafe { *encoded.as_ptr() = 13 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value.0, 13);

    assert_eq!(CLONES.load(Ordering::Relaxed), 0);
    assert_eq!(SYNCS.load(Ordering::Relaxed), 6);
}
