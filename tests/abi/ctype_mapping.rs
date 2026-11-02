use crate::TransparentCarrier;
use co3::{CType, ReprC, rust_spec::RustSpec};
use core::num::{NonZeroI64, NonZeroIsize, NonZeroU8, NonZeroU32, NonZeroUsize};
use core::{marker::PhantomData, mem::MaybeUninit};
use static_assertions::{
    assert_impl_all, assert_not_impl_any, assert_type_eq_all, assert_type_ne_all,
};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Plain(u32);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Generic<T>(T);

#[derive(RustSpec, ReprC)]
struct Unstable(u32);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct HasTrap(bool);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct NonCTypeField(MaybeUninit<NonZeroU32>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
// Optional concrete trait support is deferred only for the final field.
struct GenericNonCTypeField<T>(PhantomData<T>, MaybeUninit<NonZeroU32>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct ConditionalField {
    value: u32,
    #[cfg(any())]
    missing: MissingType,
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(custom_drop)]
struct HasDrop(u32);

impl Drop for HasDrop {
    fn drop(&mut self) {}
}

type Callback = extern "C" fn(u8) -> u8;

assert_impl_all!(Option<NonZeroU32>: CType);
assert_type_eq_all!(<NonZeroU8 as ReprC>::CType, co3::NonZero<u8>);
assert_type_eq_all!(<NonZeroI64 as ReprC>::CType, co3::NonZero<i64>);
assert_type_eq_all!(<NonZeroUsize as ReprC>::CType, co3::NonZero<usize>);
assert_type_eq_all!(<NonZeroIsize as ReprC>::CType, co3::NonZero<isize>);
assert_type_eq_all!(
    <Option<NonZeroU8> as ReprC>::CType,
    co3::option::COptionSome<NonZeroU8>
);
assert_type_eq_all!(
    <Result<NonZeroU8, ()> as ReprC>::CType,
    co3::result::CResultOk<NonZeroU8, ()>
);
assert_impl_all!(Result<NonZeroU32, ()>: CType);
assert_impl_all!(Result<(), NonZeroU32>: CType);
assert_impl_all!(Option<Callback>: CType);
assert_impl_all!(Option<&u32>: CType);
assert_not_impl_any!(co3::option::COptionSome<u8>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_impl_all!(co3::option::COptionSome<NonZeroU8>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_not_impl_any!(co3::result::CResultOk<u8, ()>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_not_impl_any!(co3::result::CResultErr<(), u8>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_not_impl_any!(co3::result::CResultOk<NonZeroU8, u8>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_not_impl_any!(co3::result::CResultErr<u8, NonZeroU8>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_impl_all!(co3::result::CResultOk<NonZeroU8, ()>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_impl_all!(co3::result::CResultErr<(), NonZeroU8>: ReprC, CType, co3::Decode<'static>, co3::transmute::AbiCompatible);
assert_impl_all!(Plain: CType);
assert_impl_all!(Generic<u32>: CType);
assert_impl_all!(ConditionalField: CType);
assert_type_eq_all!(
    <co3::option::Option<bool> as ReprC>::CType,
    co3::option::COption<bool>
);
assert_type_eq_all!(<co3::result::Result<bool, u8> as ReprC>::CType, co3::result::CResult<bool, u8>);
assert_type_eq_all!(<Vec<bool> as ReprC>::CType, co3::vec::CVec<bool>);
assert_type_eq_all!(
    <Vec<co3::primitives::Bool> as ReprC>::CType,
    co3::vec::CVec<co3::primitives::Bool>
);
assert_type_ne_all!(
    co3::option::COption<bool>,
    co3::option::COption<co3::primitives::Bool>
);
assert_type_ne_all!(co3::result::CResult<bool, u8>, co3::result::CResult<co3::primitives::Bool, u8>);
assert_type_ne_all!(co3::vec::CVec<bool>, co3::vec::CVec<co3::primitives::Bool>);

assert_not_impl_any!(Option<u32>: CType);
assert_not_impl_any!(Option<Box<u32>>: CType);
assert_not_impl_any!(Result<u32, u32>: CType);
assert_not_impl_any!(Unstable: CType);
assert_not_impl_any!(Generic<bool>: CType);
assert_not_impl_any!(HasTrap: CType);
assert_not_impl_any!(MaybeUninit<NonZeroU32>: CType);
assert_not_impl_any!(NonCTypeField: CType);
assert_not_impl_any!(GenericNonCTypeField<()>: CType);
assert_not_impl_any!(HasDrop: CType);

assert_type_eq_all!(
    <Option<&u32> as ReprC>::CType,
    co3::option::COptionSome<&u32>
);
assert_type_eq_all!(
    <Option<Box<u32>> as ReprC>::CType,
    co3::option::COptionSome<Box<u32>>
);
assert_type_eq_all!(
    <Option<&(u32,)> as ReprC>::CType,
    co3::option::COptionSome<&(u32,)>
);
assert_type_eq_all!(
    <Result<Box<u32>, ()> as ReprC>::CType,
    co3::result::CResultOk<Box<u32>, ()>
);
assert_type_eq_all!(
    <Result<(), &(u32,)> as ReprC>::CType,
    co3::result::CResultErr<(), &(u32,)>
);
assert_type_ne_all!(
    co3::result::CResultOk<u8, ()>,
    co3::result::CResultOk<u8, PhantomData<u8>>,
    co3::result::CResultErr<(), u8>
);
assert_impl_all!(co3::result::CResultOk<u8, core::cell::Cell<u8>>: Copy);

#[test]
fn nonzero_carrier_preserves_integer_abi_and_niche() {
    use core::mem::{align_of, size_of};

    assert_eq!(size_of::<co3::NonZero<u8>>(), size_of::<u8>());
    assert_eq!(align_of::<co3::NonZero<u8>>(), align_of::<u8>());
    assert_impl_all!(co3::NonZero<u8>: CType, co3::CFnArg, co3::CFnReturn);

    let value = NonZeroU8::new(7).unwrap();
    assert_eq!(co3::encode(value), co3::NonZero(7));
    assert_eq!(
        unsafe { co3::decode::<NonZeroU8>(co3::NonZero(7)) },
        Some(value)
    );
    assert_eq!(
        unsafe { co3::decode::<NonZeroU8>(core::mem::zeroed()) },
        None
    );
    assert_eq!(co3::encode(None::<NonZeroU8>).payload(), unsafe {
        core::mem::zeroed()
    });
}

#[test]
fn result_niche_carriers_distinguish_arms() {
    use co3::result::{CResultErr, CResultOk};
    use core::mem::{align_of, size_of};

    assert_eq!(
        size_of::<CResultOk<Box<u32>, ()>>(),
        size_of::<co3::boxed::CBox<u32>>()
    );
    assert_eq!(
        align_of::<CResultErr<(), &u32>>(),
        align_of::<co3::reference::CRef<u32>>()
    );
    assert_impl_all!(CResultOk<Box<u32>, ()>: CType, co3::CFnArg, co3::CFnReturn);
    assert_impl_all!(CResultErr<(), &u32>: CType, co3::CFnArg, co3::CFnReturn);

    let ok = co3::encode(Result::<Box<u32>, ()>::Ok(Box::new(7)));
    assert!(!ok.payload().as_ptr().is_null());
    let ok = unsafe { co3::decode::<Result<Box<u32>, ()>>(ok) }
        .unwrap()
        .unwrap();
    assert_eq!(*ok, 7);

    let err = co3::encode(Result::<(), Box<u32>>::Err(Box::new(9)));
    let err = unsafe { co3::decode::<Result<(), Box<u32>>>(err) }
        .unwrap()
        .unwrap_err();
    assert_eq!(*err, 9);

    let absent_err = co3::encode(Result::<Box<u32>, ()>::Err(()));
    assert!(absent_err.payload().as_ptr().is_null());
    let absent_ok = co3::encode(Result::<(), Box<u32>>::Ok(()));
    assert!(absent_ok.payload().as_ptr().is_null());
}

#[test]
fn optional_preserves_carrier_abi() {
    use core::mem::{align_of, size_of};

    assert_eq!(
        size_of::<co3::option::COptionSome<Box<u32>>>(),
        size_of::<co3::boxed::CBox<u32>>()
    );
    assert_eq!(
        align_of::<co3::option::COptionSome<Box<u32>>>(),
        align_of::<co3::boxed::CBox<u32>>()
    );
    assert_impl_all!(co3::option::COptionSome<Box<u32>>: CType, co3::CFnArg, co3::CFnReturn);

    let encoded = co3::encode(Some(Box::new(7_u32)));
    assert!(!encoded.payload().as_ptr().is_null());
    let decoded = unsafe { co3::decode::<Option<Box<u32>>>(encoded) }
        .unwrap()
        .unwrap();
    assert_eq!(*decoded, 7);

    let value = (11_u32,);
    let mut encode_store = Default::default();
    let encoded = co3::soft_encode(Some(&value), &mut encode_store);
    let mut decode_store = Default::default();
    let decoded = unsafe { co3::soft_decode::<Option<&(u32,)>>(encoded, &mut decode_store) }
        .unwrap()
        .unwrap();
    assert_eq!(decoded.0, 11);
}
