use core::{marker::PhantomData, ptr::NonNull};

use co3::{
    CType,
    boxed::{CBox, CWideBox},
    pack::{Pack2, Unpack2},
    slice::{CWideRef, CWideRefMut},
    stored::Store,
    sync::{RefMutWideEncodeStore, RefWideEncodeStore},
    wide::Wide,
};

use crate::TransparentCarrier;

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Metadata {
    length: usize,
    marker: u32,
}
unsafe impl CType for Metadata {}
unsafe impl co3::Zeroable for Metadata {}

#[repr(transparent)]
#[derive(co3::rust_spec::RustSpec)]
struct Custom<M>(PhantomData<M>, [u8]);
unsafe impl CType for Custom<Metadata> {}
impl co3::ReprC for Custom<Metadata> {
    type CType = Self;
}

macro_rules! impl_custom_wide {
    ($metadata:ty, $make:expr, $length:expr) => {
        unsafe impl Wide for Custom<$metadata> {
            type Header = [u8; 0];
            type Metadata = $metadata;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                ($make)((ptr as *const [u8]).len())
            }
            fn as_ptr(ptr: *const Self) -> *const Self::Header {
                ptr.cast()
            }
            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
                ptr.cast()
            }
            unsafe fn from_raw_parts<'a>(
                data: *const Self::Header,
                metadata: Self::Metadata,
            ) -> &'a Self {
                let slice = core::ptr::slice_from_raw_parts(data.cast::<u8>(), ($length)(metadata));
                unsafe { &*(slice as *const Self) }
            }
            unsafe fn from_raw_parts_mut<'a>(
                data: *mut Self::Header,
                metadata: Self::Metadata,
            ) -> &'a mut Self {
                let slice =
                    core::ptr::slice_from_raw_parts_mut(data.cast::<u8>(), ($length)(metadata));
                unsafe { &mut *(slice as *mut Self) }
            }
            fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
                Box::into_non_null(self).cast()
            }
            unsafe fn from_non_null(
                data: NonNull<Self::Header>,
                metadata: Self::Metadata,
            ) -> Box<Self> {
                let value = unsafe { Self::from_raw_parts_mut(data.as_ptr(), metadata) };
                unsafe { Box::from_raw(value as *mut Self) }
            }
        }
    };
}
impl_custom_wide!(
    Metadata,
    |length| Metadata { length, marker: 7 },
    |m: Metadata| m.length
);
impl_custom_wide!(usize, |length| length, |length| length);
unsafe impl CType for Custom<usize> {}
impl co3::ReprC for Custom<usize> {
    type CType = Self;
}

#[test]
fn raw_wide_unpack_preserves_custom_metadata() {
    type Pointee = Custom<Metadata>;
    let mut bytes = [1_u8, 2, 3];
    let ptr = bytes.as_mut_slice() as *mut [u8] as *mut Pointee;
    let expected = Metadata {
        length: 3,
        marker: 7,
    };
    let nonnull = NonNull::new(ptr).unwrap();

    let (data, metadata) = <*const Pointee as Unpack2<*const u8, Metadata>>::unpack(ptr).unwrap();
    assert_eq!((data, metadata), (bytes.as_ptr(), expected));
    let (data, metadata) = <*mut Pointee as Unpack2<*mut u8, Metadata>>::unpack(ptr).unwrap();
    assert_eq!((data, metadata), (bytes.as_mut_ptr(), expected));
    let (data, metadata) =
        <NonNull<Pointee> as Unpack2<*const u8, Metadata>>::unpack(co3::encode(nonnull)).unwrap();
    assert_eq!((data, metadata), (bytes.as_ptr(), expected));
    let (data, metadata) =
        <NonNull<Pointee> as Unpack2<*mut u8, Metadata>>::unpack(co3::encode(nonnull)).unwrap();
    assert_eq!((data, metadata), (bytes.as_mut_ptr(), expected));
}

#[test]
fn wide_reference_pack_preserves_slice_length() {
    type Pointee = Custom<usize>;
    let mut bytes = [1_u8, 2, 3];
    let expected = 3_usize;
    let shared =
        unsafe { <&Pointee as Pack2<*const u8, usize>>::pack(bytes.as_ptr(), expected) }.unwrap();
    assert_eq!(shared.metadata(), expected);
    assert_eq!(unsafe { &shared.into_ref().unwrap().1 }, &bytes);
    let (data, metadata) = <&Pointee as Unpack2<*const u8, usize>>::unpack(shared).unwrap();
    assert_eq!((data, metadata), (bytes.as_ptr(), expected));

    let mutable =
        unsafe { <&mut Pointee as Pack2<*mut u8, usize>>::pack(bytes.as_mut_ptr(), expected) }
            .unwrap();
    assert_eq!(mutable.metadata(), expected);
    let (data, metadata) = <&mut Pointee as Unpack2<*mut u8, usize>>::unpack(mutable).unwrap();
    assert_eq!((data, metadata), (bytes.as_mut_ptr(), expected));
    unsafe { mutable.into_mut().unwrap().1[0] = 9 };
    assert_eq!(bytes, [9, 2, 3]);
}

#[test]
fn wide_boxes_preserve_custom_metadata() {
    type Pointee = Custom<Metadata>;
    fn boxed() -> Box<Pointee> {
        let bytes = vec![1_u8, 2, 3].into_boxed_slice();
        unsafe { Box::from_raw(Box::into_raw(bytes) as *mut Pointee) }
    }
    let expected = Metadata {
        length: 3,
        marker: 7,
    };
    let carrier = CWideBox::<Pointee>::from_box(boxed());
    assert_eq!(carrier.metadata(), expected);
    assert_eq!(&unsafe { carrier.into_box() }.unwrap().1, &[1, 2, 3]);

    let carrier = CWideBox::<Pointee>::from_box(boxed());
    assert_eq!(carrier.metadata(), expected);
    assert_eq!(&unsafe { carrier.into_box() }.unwrap().1, &[1, 2, 3]);
    assert_eq!(
        CWideBox::<Pointee>::null().metadata(),
        Metadata {
            length: 0,
            marker: 0
        }
    );
    assert_eq!(
        CWideBox::<Pointee>::null().metadata(),
        Metadata {
            length: 0,
            marker: 0
        }
    );
}

#[test]
fn owned_slice_pack_handles_both_mutability_kinds() {
    use core::cell::{Cell, UnsafeCell};

    let plain = CWideBox::from_box(vec![3_u8, 5].into_boxed_slice());
    let (data, len) = <Box<[u8]> as Unpack2<CBox<u8>, usize>>::unpack(plain).unwrap();
    let carrier = unsafe { <Box<[u8]> as Pack2<CBox<u8>, usize>>::pack(data, len) }.unwrap();
    assert_eq!(&*unsafe { carrier.into_box() }.unwrap(), &[3, 5]);

    type Element = UnsafeCell<u8>;
    let inner = vec![Element::new(3), Element::new(5)].into_boxed_slice();
    let carrier = CWideBox::<[Cell<u8>]>::from_box(inner);
    let (data, len) = <Box<[Cell<u8>]> as Unpack2<CBox<Cell<u8>>, usize>>::unpack(carrier).unwrap();
    let carrier =
        unsafe { <Box<[Cell<u8>]> as Pack2<CBox<Cell<u8>>, usize>>::pack(data, len) }.unwrap();
    let mut boxed = unsafe { carrier.into_box() }.unwrap();
    assert_eq!(
        boxed.iter_mut().map(|v| *v.get_mut()).collect::<Vec<_>>(),
        [3, 5]
    );

    // A vector's capacity cannot be represented by a pointer/length pair.
    static_assertions::assert_not_impl_any!(Vec<u8>:
        Unpack2<CBox<u8>, usize>, Pack2<CBox<u8>, usize>);
    static_assertions::assert_not_impl_any!(Vec<Cell<u8>>:
        Unpack2<CBox<Cell<u8>>, usize>, Pack2<CBox<Cell<u8>>, usize>);
}

#[test]
fn owned_pack_preserves_null_pointer_and_metadata() {
    type Pointee = [u8];
    let metadata = 3_usize;
    let carrier =
        unsafe { <Option<Box<Pointee>> as Pack2<CBox<u8>, usize>>::pack(CBox::null(), metadata) }
            .unwrap();
    assert!(carrier.payload().as_ptr().is_null());
    let (pointer, unpacked) =
        <Option<Box<Pointee>> as Unpack2<CBox<u8>, usize>>::unpack(carrier).unwrap();
    assert!(pointer.as_ptr().is_null());
    assert_eq!(unpacked, metadata);
}

#[test]
fn carriers_preserve_non_slice_metadata() {
    type Pointee = Custom<Metadata>;
    let mut source = [3_u8, 5, 8];
    let value = unsafe { &mut *(source.as_mut_slice() as *mut [u8] as *mut Pointee) };
    let shared = CWideRef::<Pointee>::from_ref(&*value);
    assert_eq!(
        shared.metadata(),
        Metadata {
            length: 3,
            marker: 7
        }
    );
    let raw = unsafe { CWideRef::<Pointee>::from_raw_parts(shared.as_ptr(), shared.metadata()) };
    let decoded =
        unsafe { co3::soft_decode::<CWideRef<Pointee>>(raw, &mut Default::default()) }.unwrap();
    assert_eq!(decoded.metadata(), shared.metadata());
    assert_eq!(unsafe { &shared.into_ref().unwrap().1 }, &[3, 5, 8]);

    let mutable = CWideRefMut::<Pointee, true>::from_mut(value);
    assert_eq!(
        mutable.metadata(),
        Metadata {
            length: 3,
            marker: 7
        }
    );
    unsafe { mutable.into_mut().unwrap().1[1] = 9 };
    assert_eq!(source, [3, 9, 8]);

    let null = CWideRef::<Pointee>::null();
    assert!(null.as_ptr().is_null());
    assert_eq!(
        null.metadata(),
        Metadata {
            length: 0,
            marker: 0
        }
    );
    let decoded =
        unsafe { co3::soft_decode::<CWideRef<Pointee>>(null, &mut Default::default()) }.unwrap();
    assert!(unsafe { decoded.into_ref() }.is_none());
    static_assertions::assert_impl_all!(CWideRef<Pointee>: CType, Copy);
}

#[test]
fn wide_store_returns_custom_metadata() {
    type Pointee = Custom<Metadata>;
    let mut store = RefWideEncodeStore::<Pointee, ()>::default();
    let carrier = store.encode(|_| {
        let bytes: Box<[u8]> = vec![2, 4].into_boxed_slice();
        let ptr = Box::into_raw(bytes) as *mut Pointee;
        unsafe { Box::from_raw(ptr) }
    });
    assert_eq!(
        carrier.metadata(),
        Metadata {
            length: 2,
            marker: 7
        }
    );
    assert_eq!(unsafe { &carrier.into_ref().unwrap().1 }, &[2, 4]);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn mutable_wide_store_returns_custom_metadata_and_syncs() {
    type Pointee = Custom<Metadata>;
    let mut result = 0;
    let mut store = RefMutWideEncodeStore::<Pointee, (), &mut u8>::default();
    let carrier = store.encode(
        |_| {
            let bytes = vec![2_u8, 4].into_boxed_slice();
            let ptr = Box::into_raw(bytes) as *mut Pointee;
            (unsafe { Box::from_raw(ptr) }, &mut result)
        },
        |value, result| *result = value.1[0],
    );
    assert_eq!(
        carrier.metadata(),
        Metadata {
            length: 2,
            marker: 7
        }
    );
    unsafe { carrier.into_mut().unwrap().1[0] = 9 };
    assert_eq!(store.sync(), Some(()));
    assert_eq!(result, 9);
}

#[test]
fn comparisons_use_pointer_then_arbitrary_metadata() {
    type Pointee = Custom<Metadata>;
    let mut bytes = [0_u8; 2];
    let first = bytes.as_mut_ptr();
    let second = unsafe { first.add(1) };
    let a = Metadata {
        length: 0,
        marker: 1,
    };
    let b = Metadata {
        length: 0,
        marker: 2,
    };

    let one = unsafe { CWideRef::<Pointee>::from_raw_parts(first, a) };
    let same_pointer = unsafe { CWideRef::<Pointee>::from_raw_parts(first, b) };
    let different_pointer = unsafe { CWideRef::<Pointee>::from_raw_parts(second, a) };
    assert!(one < same_pointer);
    assert_ne!(one, different_pointer);
    assert_eq!(one.cmp(&different_pointer), first.cmp(&second));

    let one = unsafe { CWideRefMut::<Pointee, true>::from_raw_parts_mut(first, a) };
    let same_pointer = unsafe { CWideRefMut::<Pointee, true>::from_raw_parts_mut(first, b) };
    let different_pointer = unsafe { CWideRefMut::<Pointee, true>::from_raw_parts_mut(second, a) };
    assert!(one < same_pointer);
    assert_ne!(one, different_pointer);
    assert_eq!(one.cmp(&different_pointer), first.cmp(&second));

    let null = CWideRef::<Pointee>::null();
    let other_null = unsafe { CWideRef::<Pointee>::from_raw_parts(core::ptr::null(), a) };
    assert_ne!(null, other_null);
}

#[test]
fn wide_carrier_hashes_include_data_and_metadata() {
    use core::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    fn hash(value: &impl Hash) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    type Pointee = Custom<Metadata>;
    let mut byte = 0_u8;
    let data = &mut byte as *mut u8;
    let metadata = Metadata {
        length: 1,
        marker: 7,
    };
    let shared = unsafe { CWideRef::<Pointee>::from_raw_parts(data, metadata) };
    let mutable = unsafe { CWideRefMut::<Pointee, true>::from_raw_parts_mut(data, metadata) };
    let nonexclusive = unsafe { CWideRefMut::<Pointee, false>::from_raw_parts_mut(data, metadata) };

    assert_eq!(hash(&shared), hash(&(data as *const u8, metadata)));
    assert_eq!(hash(&mutable), hash(&(data, metadata)));
    assert_eq!(hash(&nonexclusive), hash(&(data, metadata)));
}
