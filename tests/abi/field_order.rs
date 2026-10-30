use std::cell::RefCell;

use co3::{
    ReprC,
    rust_spec::RustSpec,
    stored::{
        DecodeMove, EncodeMove, EncodeMoveUnchecked, EncodeMut, EncodeMutUnchecked, EncodeRef,
        EncodeRefUnchecked,
    },
    tuple::Tuple3,
};

thread_local! {
    static EVENTS: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[derive(RustSpec)]
struct Probe(u8);

unsafe impl co3::borrow::Borrow for Probe {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }
    fn from_borrow(source: Self) -> Self {
        source
    }
}

impl co3::EncodeUnchecked for Probe {}
impl co3::Encode for Probe {}
impl co3::Decode<'_> for Probe {}

impl ReprC for Probe {
    type CType = u8;
}

unsafe impl EncodeMoveUnchecked for Probe {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> u8
    where
        Self: 'itm,
    {
        EVENTS.with(|events| events.borrow_mut().push(self.0));
        self.0
    }
}
unsafe impl EncodeMove for Probe {
    type Store = ();
    fn soft_encode<'a>(self, _: &'a mut ()) -> u8
    where
        Self: 'a,
    {
        EVENTS.with(|events| events.borrow_mut().push(self.0));
        self.0
    }
}
unsafe impl<'d> DecodeMove<'d> for Probe {
    type Store = ();
    unsafe fn soft_decode<'a: 'd>(source: u8, _: &'a mut ()) -> Option<Self> {
        EVENTS.with(|events| events.borrow_mut().push(source));
        Some(Self(source))
    }
}

unsafe impl<'a> EncodeRefUnchecked<'a> for Probe {
    type SoftStore = ();
    unsafe fn encode_ref_unchecked<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        EVENTS.with(|events| events.borrow_mut().push(self.0));
        co3::reference::CRef::from_ref(&self.0)
    }
}
unsafe impl<'a> EncodeRef<'a> for Probe {
    type Store = ();
    fn encode_ref<'itm>(&'a self, _: &'itm mut ()) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        EVENTS.with(|events| events.borrow_mut().push(self.0));
        co3::reference::CRef::from_ref(&self.0)
    }
}
unsafe impl<'a> EncodeMutUnchecked<'a> for Probe {
    type SoftStore = ();
    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        _: &'itm mut (),
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        EVENTS.with(|events| events.borrow_mut().push(self.0));
        co3::reference::CRefMut::from_mut(&mut self.0)
    }
}
unsafe impl<'a> EncodeMut<'a> for Probe {
    type Store = ();
    fn encode_mut<'itm>(&'a mut self, _: &'itm mut ()) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        EVENTS.with(|events| events.borrow_mut().push(self.0));
        co3::reference::CRefMut::from_mut(&mut self.0)
    }
}

#[derive(RustSpec, ReprC)]
struct Record(Probe, Probe, [u8; 1]);

fn assert_order() {
    EVENTS.with(|events| assert_eq!(&*events.borrow(), &[1, 2, 3]));
    EVENTS.with(|events| events.borrow_mut().clear());
}

fn assert_prefix_order() {
    EVENTS.with(|events| assert_eq!(&*events.borrow(), &[1, 2]));
    EVENTS.with(|events| events.borrow_mut().clear());
}

#[test]
fn owned_tuple_and_derived_fields_encode_and_decode_in_order() {
    let values = (Probe(1), Probe(2), Probe(3));
    let encoded: Tuple3<u8, u8, u8> = co3::soft_encode(values, &mut ((), (), ()));
    assert_order();
    let _ =
        unsafe { co3::soft_decode::<(Probe, Probe, Probe)>(encoded, &mut ((), (), ())) }.unwrap();
    assert_order();

    let encoded =
        unsafe { co3::soft_encode_unchecked((Probe(1), Probe(2), Probe(3)), &mut ((), (), ())) };
    assert_order();
    let _ =
        unsafe { co3::soft_decode_unchecked::<(Probe, Probe, Probe)>(encoded, &mut ((), (), ())) };
    assert_order();

    let encoded = co3::soft_encode(Record(Probe(1), Probe(2), [3]), &mut Default::default());
    assert_prefix_order();
    let _ = unsafe { co3::soft_decode::<Record>(encoded, &mut Default::default()) }.unwrap();
    assert_prefix_order();

    let encoded = unsafe {
        co3::soft_encode_unchecked(Record(Probe(1), Probe(2), [3]), &mut Default::default())
    };
    assert_prefix_order();
    let _ = unsafe { co3::soft_decode_unchecked::<Record>(encoded, &mut Default::default()) };
    assert_prefix_order();
}
