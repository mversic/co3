use co3::{
    ReprC,
    borrow::Borrowed,
    stored::{EncodeMove, EncodeMoveUnchecked, Store},
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

#[test]
fn shared_and_borrowed_encoding_clone_once_and_sync_nested_state() {
    let mut store = Default::default();
    let encoded = (&Probe(7)).soft_encode(&mut store);
    assert_eq!(unsafe { *encoded.as_ptr() }, 7);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { (&Probe(8)).soft_encode_unchecked(&mut store) };
    assert_eq!(unsafe { *encoded.as_ptr() }, 8);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    assert_eq!(Borrowed(Probe(9)).soft_encode(&mut store).0, 9);
    assert_eq!(store.sync(), Some(()));

    let mut store = Default::default();
    let encoded = unsafe { Borrowed(Probe(10)).soft_encode_unchecked(&mut store) };
    assert_eq!(encoded.0, 10);
    assert_eq!(store.sync(), Some(()));

    assert_eq!(CLONES.load(Ordering::Relaxed), 4);
    assert_eq!(SYNCS.load(Ordering::Relaxed), 4);
}
