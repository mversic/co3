use co3::{ReprC, encode, niche::Niche, rust_spec::RustSpec, soft_decode};

#[test]
fn tagged_carrier_constructors_preserve_active_payloads() {
    use core::mem::MaybeUninit;

    use co3::{option::abi::Option as AbiOption, result::abi::Result as AbiResult};

    assert_eq!(AbiOption::<u32>::new(Some(7)).into_option(), Some(Some(7)));
    assert_eq!(AbiOption::<u32>::new(None).into_option(), Some(None));
    let option = unsafe { AbiOption::<u32>::from_raw_parts(2, MaybeUninit::uninit()) };
    assert!(option != option);
    assert_eq!(option.partial_cmp(&option), None);
    assert_eq!(option.into_option(), None);
    let option = unsafe { AbiOption::<u32>::from_raw_parts(1, MaybeUninit::new(9)) };
    assert_eq!(option.into_option(), Some(Some(9)));
    assert_eq!(AbiResult::<u32, u16>::new(Ok(7)).into_result(), Some(Ok(7)));
    assert_eq!(
        AbiResult::<u32, u16>::new(Err(4)).into_result(),
        Some(Err(4))
    );
    let result = unsafe { AbiResult::<u32, u16>::from_raw_parts(2, Ok(MaybeUninit::uninit())) };
    assert!(result != result);
    assert_eq!(result.partial_cmp(&result), None);
    assert_eq!(result.into_result(), None);
    let result = unsafe { AbiResult::<u32, u16>::from_raw_parts(1, Err(MaybeUninit::new(5))) };
    assert_eq!(result.into_result(), Some(Err(5)));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
enum SparseInt {
    A(u32) = 1,
    B,
    C = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C, u8)]
enum SparseC {
    A(u32) = 1,
    B,
    C = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
enum SparseFieldless {
    A = 1,
    B = 5,
}

const CONST_INT: <SparseInt as ReprC>::CType = <SparseInt as ReprC>::CType::A(42);
const CONST_INT_NICHE: <SparseInt as ReprC>::CType = <SparseInt as ReprC>::CType::niche();
const CONST_FIELDLESS_NICHE: <SparseFieldless as ReprC>::CType =
    <SparseFieldless as ReprC>::CType::niche();
const CONST_INT_TAG: u8 = CONST_INT.tag();
const CONST_C: <SparseC as ReprC>::CType = <SparseC as ReprC>::CType::A(42);
const CONST_C_NICHE: <SparseC as ReprC>::CType = <SparseC as ReprC>::CType::niche();
const CONST_C_TAG: u8 = CONST_C.tag();
const CONST_OPTION: Option<Option<u32>> = co3::option::abi::Option::<u32>::Some(7).into_option();
const CONST_RESULT: Option<Result<u32, u16>> =
    co3::result::abi::Result::<u32, u16>::Err(4).into_result();
const CONST_NULL_REF: Option<&'static u8> =
    unsafe { co3::reference::SoftRef::<u8>::null().into_ref() };

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
enum WithZero {
    #[repr_c(allow_zero_tag)]
    A(u32) = 0,
    B = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
enum WithZeroAndMax {
    #[repr_c(allow_zero_tag)]
    A(u32) = 0,
    B = 255,
}

#[test]
fn explicit_tags_drive_encoding_decoding_and_niches() {
    static_assertions::assert_not_impl_any!(<SparseInt as ReprC>::CType: Eq, Ord);
    static_assertions::assert_not_impl_any!(<SparseC as ReprC>::CType: Eq, Ord);
    static_assertions::assert_not_impl_any!(<SparseFieldless as ReprC>::CType: Eq, Ord);
    assert!(<SparseFieldless as Niche>::NICHE.is_niche());
    assert!(CONST_FIELDLESS_NICHE.is_niche());
    assert!(CONST_FIELDLESS_NICHE != CONST_FIELDLESS_NICHE);
    assert!(
        <SparseFieldless as ReprC>::CType::from_u8(3)
            != <SparseFieldless as ReprC>::CType::from_u8(3)
    );
    assert_eq!(
        CONST_FIELDLESS_NICHE.tag(),
        <SparseFieldless as Niche>::NICHE.tag()
    );
    assert!(!encode(SparseFieldless::A).is_niche());
    assert_eq!(
        unsafe { co3::decode::<Option<SparseFieldless>>(encode(None::<SparseFieldless>)) },
        Some(None)
    );
    assert_eq!((CONST_INT_TAG, CONST_C_TAG), (1, 1));
    assert!(CONST_INT_NICHE.is_niche());
    assert!(CONST_C_NICHE.is_niche());
    assert_eq!(CONST_INT_NICHE.tag(), <SparseInt as Niche>::NICHE.tag());
    assert_eq!(CONST_C_NICHE.tag(), <SparseC as Niche>::NICHE.tag());
    assert_eq!(CONST_OPTION, Some(Some(7)));
    assert_eq!(CONST_RESULT, Some(Err(4)));
    assert_eq!(CONST_NULL_REF, None);
    let a = encode(SparseInt::A(42));
    assert!(!a.is_niche());
    let raw =
        unsafe { <SparseInt as ReprC>::CType::from_raw_parts(1, core::mem::MaybeUninit::new(a)) };
    assert!(raw == a);
    let raw_niche =
        unsafe { <SparseInt as ReprC>::CType::from_raw_parts(0, core::mem::MaybeUninit::uninit()) };
    assert!(raw_niche.is_niche());
    assert!(raw_niche != <SparseInt as Niche>::NICHE);
    let b = encode(SparseInt::B);
    let c = encode(SparseInt::C);
    assert_eq!([a.tag(), b.tag(), c.tag()], [1, 2, 5]);
    assert!(a == <SparseInt as ReprC>::CType::A(42));
    assert!(b == <SparseInt as ReprC>::CType::B());
    assert!(c == <SparseInt as ReprC>::CType::C());
    assert!(<SparseInt as ReprC>::CType::A(1) < <SparseInt as ReprC>::CType::A(2));
    assert!(<SparseInt as ReprC>::CType::A(42) < <SparseInt as ReprC>::CType::B());
    assert_eq!(
        format!("{:?}", <SparseInt as ReprC>::CType::A(42)),
        "SparseIntCType::A(42)"
    );
    let niche = <SparseInt as Niche>::NICHE;
    assert_eq!(niche.partial_cmp(&niche), None);
    assert_eq!(niche.partial_cmp(&a), None);
    assert_eq!(
        format!("{:?}", niche),
        "SparseIntCType::<invalid> { tag: 0 }"
    );
    assert_eq!(
        unsafe { soft_decode::<SparseInt>(a, &mut None) },
        Some(SparseInt::A(42))
    );
    assert_eq!(
        unsafe { soft_decode::<SparseInt>(b, &mut None) },
        Some(SparseInt::B)
    );
    assert_eq!(
        unsafe { soft_decode::<SparseInt>(c, &mut None) },
        Some(SparseInt::C)
    );
    assert_eq!(<SparseInt as Niche>::NICHE.tag(), 0);

    let a = encode(SparseC::A(42));
    let b = encode(SparseC::B);
    let c = encode(SparseC::C);
    assert_eq!([a.tag(), b.tag(), c.tag()], [1, 2, 5]);
    assert!(a == <SparseC as ReprC>::CType::A(42));
    assert!(b == <SparseC as ReprC>::CType::B());
    assert!(c == <SparseC as ReprC>::CType::C());
    assert_eq!(
        unsafe { soft_decode::<SparseC>(a, &mut None) },
        Some(SparseC::A(42))
    );
    assert_eq!(
        unsafe { soft_decode::<SparseC>(b, &mut None) },
        Some(SparseC::B)
    );
    assert_eq!(
        unsafe { soft_decode::<SparseC>(c, &mut None) },
        Some(SparseC::C)
    );
    assert_eq!(<SparseC as Niche>::NICHE.tag(), 0);
    let uninitialized =
        unsafe { <SparseC as ReprC>::CType::from_raw_parts(0, core::mem::MaybeUninit::uninit()) };
    assert!(uninitialized.is_niche());
    let other_invalid =
        unsafe { <SparseC as ReprC>::CType::from_raw_parts(3, core::mem::MaybeUninit::uninit()) };
    assert!(!other_invalid.is_niche());
    assert!(uninitialized != <SparseC as Niche>::NICHE);
    assert_eq!(
        unsafe { soft_decode::<SparseC>(uninitialized, &mut None) },
        None
    );

    assert_eq!(encode(WithZero::A(7)).tag(), 0);
    assert_eq!(<WithZero as Niche>::NICHE.tag(), u8::MAX);
    assert_eq!(<WithZeroAndMax as Niche>::NICHE.tag(), u8::MAX - 1);
    assert!(<WithZeroAndMax as Niche>::NICHE.is_niche());
}
