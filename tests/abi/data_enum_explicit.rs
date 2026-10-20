use co3::{ReprC, encode, niche::Niche, rust_spec::RustSpec, soft_decode};

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
    let a = encode(SparseInt::A(42));
    let b = encode(SparseInt::B);
    let c = encode(SparseInt::C);
    assert_eq!([a.tag(), b.tag(), c.tag()], [1, 2, 5]);
    assert!(a == CSparseInt::A(42));
    assert!(b == CSparseInt::B());
    assert!(c == CSparseInt::C());
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
    assert!(a == CSparseC::A(42));
    assert!(b == CSparseC::B());
    assert!(c == CSparseC::C());
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

    assert_eq!(encode(WithZero::A(7)).tag(), 0);
    assert_eq!(<WithZero as Niche>::NICHE.tag(), u8::MAX);
    assert_eq!(<WithZeroAndMax as Niche>::NICHE.tag(), u8::MAX - 1);
}
