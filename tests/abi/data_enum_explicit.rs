use co3::{ReprC, encode, lang, niche::Niche, rust_spec::RustSpec, soft_decode};

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
    assert!(!unsafe { encode::<_, lang::C>(SparseFieldless::A) }.is_niche());
    assert_eq!(
        unsafe {
            co3::decode::<Option<SparseFieldless>, lang::C>(encode::<_, lang::C>(
                None::<SparseFieldless>,
            ))
        },
        Some(None)
    );
    assert_eq!((CONST_INT_TAG, CONST_C_TAG), (1, 1));
    assert!(CONST_INT_NICHE.is_niche());
    assert!(CONST_C_NICHE.is_niche());
    assert_eq!(CONST_INT_NICHE.tag(), <SparseInt as Niche>::NICHE.tag());
    assert_eq!(CONST_C_NICHE.tag(), <SparseC as Niche>::NICHE.tag());
    let a = unsafe { encode::<_, lang::C>(SparseInt::A(42)) };
    assert!(!a.is_niche());
    let raw =
        unsafe { <SparseInt as ReprC>::CType::from_raw_parts(1, core::mem::MaybeUninit::new(a)) };
    assert!(raw == a);
    let raw_niche =
        unsafe { <SparseInt as ReprC>::CType::from_raw_parts(0, core::mem::MaybeUninit::uninit()) };
    assert!(raw_niche.is_niche());
    assert!(raw_niche != <SparseInt as Niche>::NICHE);
    let b = unsafe { encode::<_, lang::C>(SparseInt::B) };
    let c = unsafe { encode::<_, lang::C>(SparseInt::C) };
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
        unsafe { soft_decode::<SparseInt, lang::C>(a, &mut None) },
        Some(SparseInt::A(42))
    );
    assert_eq!(
        unsafe { soft_decode::<SparseInt, lang::C>(b, &mut None) },
        Some(SparseInt::B)
    );
    assert_eq!(
        unsafe { soft_decode::<SparseInt, lang::C>(c, &mut None) },
        Some(SparseInt::C)
    );
    assert_eq!(<SparseInt as Niche>::NICHE.tag(), 0);

    let a = unsafe { encode::<_, lang::C>(SparseC::A(42)) };
    let b = unsafe { encode::<_, lang::C>(SparseC::B) };
    let c = unsafe { encode::<_, lang::C>(SparseC::C) };
    assert_eq!([a.tag(), b.tag(), c.tag()], [1, 2, 5]);
    assert!(a == <SparseC as ReprC>::CType::A(42));
    assert!(b == <SparseC as ReprC>::CType::B());
    assert!(c == <SparseC as ReprC>::CType::C());
    assert_eq!(
        unsafe { soft_decode::<SparseC, lang::C>(a, &mut None) },
        Some(SparseC::A(42))
    );
    assert_eq!(
        unsafe { soft_decode::<SparseC, lang::C>(b, &mut None) },
        Some(SparseC::B)
    );
    assert_eq!(
        unsafe { soft_decode::<SparseC, lang::C>(c, &mut None) },
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
        unsafe { soft_decode::<SparseC, lang::C>(uninitialized, &mut None) },
        None
    );

    assert_eq!(unsafe { encode::<_, lang::C>(WithZero::A(7)) }.tag(), 0);
    assert_eq!(<WithZero as Niche>::NICHE.tag(), u8::MAX);
    assert_eq!(<WithZeroAndMax as Niche>::NICHE.tag(), u8::MAX - 1);
    assert!(<WithZeroAndMax as Niche>::NICHE.is_niche());
}
