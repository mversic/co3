use std::cmp::Ordering;
use std::ffi::{CStr, c_char};

use co3::primitives::{CBool, COrdering};
use co3::reference::CRefMut;
use co3::{
    ReprC, encode, ffi, niche::Niche, option::ReprCOption, rust_spec::RustSpec, soft_decode,
    soft_encode,
};

#[derive(Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(with_custom_niche)]
#[repr_c(NICHE_VALUE = COverlappingCustomNiche(1))]
struct OverlappingCustomNiche(u8);

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "reserved NICHE_VALUE"))]
fn custom_niche_must_not_overlap_an_encoded_value() {
    let _ = encode(Some(OverlappingCustomNiche(1)));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(dead_code)]
pub enum Opaque {
    A,
    B,
}

ffi! {
    #![unsafe(export("C"))]

    type Opaque;

    impl Drop for Opaque {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    pub type Extern;

    impl Drop for Extern {
        #[symbol_name = "abi__Drop__Opaque__drop"]
        fn drop(&mut self);
    }
}

#[test]
fn thin_extern_reference_niche() {
    static_assertions::assert_impl_all!(&Extern: Niche<CType = *const Extern>);
    static_assertions::assert_impl_all!(&mut Extern: Niche<CType = CRefMut<Extern>>);
    // CStr is unsized but also has a thin pointer representation. This catches
    // an accidental Sized bound on the pointer-backed reference Niche impls.
    static_assertions::assert_impl_all!(&CStr: Niche<CType = *const c_char>);

    assert_eq!(encode(None::<&Extern>), std::ptr::null());
    assert_eq!(soft_encode(None::<&mut Extern>, &mut ()), unsafe {
        CRefMut::<Extern>::from_raw(std::ptr::null_mut())
    });
    assert_eq!(encode(None::<&CStr>), std::ptr::null());
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(u8)]
pub enum FieldlessUEnum {
    Var1,
    Var2,
    Var3,
    Var4,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(i8)]
pub enum FieldlessIEnum {
    Var1,
    Var2,
    Var3,
    Var4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(i8)]
pub enum FieldlessExplicitEnum {
    Negative = -2,
    Zero = 0,
    Five = 5,
    Six,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(usize)]
pub enum FieldlessUsizeEnum {
    Zero = 0,
    Five = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
pub enum FieldlessNoReprEnum {
    A,
    B,
    C,
    D,
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C, i8)]
pub enum ReprCDataEnum<'a, T> {
    A(&'a [u32; 2]),
    B(u32),
    C(T),
    D,
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(i8)]
pub enum DataEnum<'a, T> {
    A(&'a [u32; 2]),
    B(u32),
    C(T),
    D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec, ReprC)]
#[repr(usize)]
pub enum DataUsizeEnum {
    A(u8),
    B,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(u16)]
pub enum FieldlessLargeEnum {
    Var1,
    Var2,
    Var3,
    Var4,
    Var5,
    Var6,
    Var7,
    Var8,
    Var9,
    Var10,
    Var11,
    Var12,
    Var13,
    Var14,
    Var15,
    Var16,
    Var17,
    Var18,
    Var19,
    Var20,
    Var21,
    Var22,
    Var23,
    Var24,
    Var25,
    Var26,
    Var27,
    Var28,
    Var29,
    Var30,
    Var31,
    Var32,
    Var33,
    Var34,
    Var35,
    Var36,
    Var37,
    Var38,
    Var39,
    Var40,
    Var41,
    Var42,
    Var43,
    Var44,
    Var45,
    Var46,
    Var47,
    Var48,
    Var49,
    Var50,
    Var51,
    Var52,
    Var53,
    Var54,
    Var55,
    Var56,
    Var57,
    Var58,
    Var59,
    Var60,
    Var61,
    Var62,
    Var63,
    Var64,
    Var65,
    Var66,
    Var67,
    Var68,
    Var69,
    Var70,
    Var71,
    Var72,
    Var73,
    Var74,
    Var75,
    Var76,
    Var77,
    Var78,
    Var79,
    Var80,
    Var81,
    Var82,
    Var83,
    Var84,
    Var85,
    Var86,
    Var87,
    Var88,
    Var89,
    Var90,
    Var91,
    Var92,
    Var93,
    Var94,
    Var95,
    Var96,
    Var97,
    Var98,
    Var99,
    Var100,
    Var101,
    Var102,
    Var103,
    Var104,
    Var105,
    Var106,
    Var107,
    Var108,
    Var109,
    Var110,
    Var111,
    Var112,
    Var113,
    Var114,
    Var115,
    Var116,
    Var117,
    Var118,
    Var119,
    Var120,
    Var121,
    Var122,
    Var123,
    Var124,
    Var125,
    Var126,
    Var127,
    Var128,
    Var129,
    Var130,
    Var131,
    Var132,
    Var133,
    Var134,
    Var135,
    Var136,
    Var137,
    Var138,
    Var139,
    Var140,
    Var141,
    Var142,
    Var143,
    Var144,
    Var145,
    Var146,
    Var147,
    Var148,
    Var149,
    Var150,
    Var151,
    Var152,
    Var153,
    Var154,
    Var155,
    Var156,
    Var157,
    Var158,
    Var159,
    Var160,
    Var161,
    Var162,
    Var163,
    Var164,
    Var165,
    Var166,
    Var167,
    Var168,
    Var169,
    Var170,
    Var171,
    Var172,
    Var173,
    Var174,
    Var175,
    Var176,
    Var177,
    Var178,
    Var179,
    Var180,
    Var181,
    Var182,
    Var183,
    Var184,
    Var185,
    Var186,
    Var187,
    Var188,
    Var189,
    Var190,
    Var191,
    Var192,
    Var193,
    Var194,
    Var195,
    Var196,
    Var197,
    Var198,
    Var199,
    Var200,
    Var201,
    Var202,
    Var203,
    Var204,
    Var205,
    Var206,
    Var207,
    Var208,
    Var209,
    Var210,
    Var211,
    Var212,
    Var213,
    Var214,
    Var215,
    Var216,
    Var217,
    Var218,
    Var219,
    Var220,
    Var221,
    Var222,
    Var223,
    Var224,
    Var225,
    Var226,
    Var227,
    Var228,
    Var229,
    Var230,
    Var231,
    Var232,
    Var233,
    Var234,
    Var235,
    Var236,
    Var237,
    Var238,
    Var239,
    Var240,
    Var241,
    Var242,
    Var243,
    Var244,
    Var245,
    Var246,
    Var247,
    Var248,
    Var249,
    Var250,
    Var251,
    Var252,
    Var253,
    Var254,
    Var255,
    Var256,
}

#[test]
fn verify_enum_niche_value() {
    assert_eq!(CBool::NICHE, encode(None::<bool>));
    assert_eq!(COrdering::NICHE, encode(None::<Ordering>));

    assert_eq!(encode(None::<Opaque>), ReprCOption::None());
    assert!(encode(None::<OwnedExtern>).0.is_null());

    let expected_niche_enum_discriminant_u = 4_u8;
    let expected_niche_enum_discriminant_i = 4_i8;

    assert_eq!(
        expected_niche_enum_discriminant_u,
        encode(None::<FieldlessUEnum>).0
    );
    assert_eq!(
        expected_niche_enum_discriminant_i,
        encode(None::<FieldlessIEnum>).0
    );
    assert_eq!(
        expected_niche_enum_discriminant_u,
        encode(None::<FieldlessNoReprEnum>).0
    );

    let encoded = soft_encode(None::<ReprCDataEnum<&u8>>, &mut Default::default());
    assert_eq!(expected_niche_enum_discriminant_i, encoded.tag);
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::from_ref(&encoded.payload).cast::<u8>(),
            core::mem::size_of_val(&encoded.payload),
        )
    };
    assert!(bytes.iter().all(|&byte| byte == 0));

    let encoded = soft_encode(None::<DataEnum<&u8>>, &mut Default::default());
    assert_eq!(expected_niche_enum_discriminant_i, unsafe { encoded.D.tag });

    let expected_fieldless_large_enum = 256u16;

    assert_eq!(
        expected_fieldless_large_enum,
        encode(None::<FieldlessLargeEnum>).0
    );
}

#[test]
fn fieldless_enum_explicit_discriminants_round_trip() {
    assert_eq!(encode(FieldlessExplicitEnum::Negative).0, -2);
    assert_eq!(encode(FieldlessExplicitEnum::Zero).0, 0);
    assert_eq!(encode(FieldlessExplicitEnum::Five).0, 5);
    assert_eq!(encode(FieldlessExplicitEnum::Six).0, 6);

    assert_eq!(
        unsafe { co3::decode(encode(FieldlessExplicitEnum::Five)) },
        Some(FieldlessExplicitEnum::Five)
    );
    assert_eq!(
        unsafe { co3::decode::<FieldlessExplicitEnum>(CFieldlessExplicitEnum(1)) },
        None
    );
    // Zero is occupied, so the generated niche selects the next invalid tag.
    assert_eq!(encode(None::<FieldlessExplicitEnum>).0, 1);
}

#[test]
fn fieldless_enum_carrier_conversions() {
    assert_eq!(CBool::TRUE, encode(true));
    assert_eq!(CBool::FALSE, encode(false));
    assert_eq!(COrdering::LESS, encode(Ordering::Less));
    assert_eq!(COrdering::EQUAL, encode(Ordering::Equal));
    assert_eq!(COrdering::GREATER, encode(Ordering::Greater));
    assert_eq!(CFieldlessExplicitEnum::NEGATIVE.0, -2);
    assert_eq!(CFieldlessExplicitEnum::ZERO.0, 0);
    assert_eq!(CFieldlessExplicitEnum::FIVE.0, 5);
    assert_eq!(CFieldlessExplicitEnum::SIX.0, 6);
    assert_eq!(CFieldlessNoReprEnum::A.0, 0);
    assert_eq!(CFieldlessNoReprEnum::B.0, 1);
    assert_eq!(CFieldlessNoReprEnum::C.0, 2);
    assert_eq!(CFieldlessNoReprEnum::D.0, 3);
    let explicit: CFieldlessExplicitEnum = FieldlessExplicitEnum::Five.into();
    assert_eq!(explicit.0, 5);
    assert_eq!(
        FieldlessExplicitEnum::try_from(explicit),
        Ok(FieldlessExplicitEnum::Five)
    );
    assert_eq!(
        FieldlessExplicitEnum::try_from(CFieldlessExplicitEnum(1)),
        Err(())
    );

    let inferred: CFieldlessNoReprEnum = FieldlessNoReprEnum::C.into();
    assert_eq!(
        FieldlessNoReprEnum::try_from(inferred),
        Ok(FieldlessNoReprEnum::C)
    );
    assert_eq!(
        FieldlessNoReprEnum::try_from(CFieldlessNoReprEnum(4)),
        Err(())
    );
}

#[test]
fn usize_repr_enums_round_trip() {
    const _: () = assert!(
        core::mem::size_of::<<FieldlessUsizeEnum as ReprC>::CType>()
            == core::mem::size_of::<usize>()
    );

    assert_eq!(encode(FieldlessUsizeEnum::Zero).0, 0usize);
    assert_eq!(encode(FieldlessUsizeEnum::Five).0, 5usize);
    assert_eq!(
        unsafe { co3::decode(encode(FieldlessUsizeEnum::Five)) },
        Some(FieldlessUsizeEnum::Five)
    );

    let mut encode_store = Default::default();
    let encoded = soft_encode(DataUsizeEnum::A(7), &mut encode_store);
    let mut decode_store = Default::default();
    assert_eq!(
        unsafe { soft_decode(encoded, &mut decode_store) },
        Some(DataUsizeEnum::A(7))
    );

    let mut encode_store = Default::default();
    let encoded = soft_encode(DataUsizeEnum::B, &mut encode_store);
    let mut decode_store = Default::default();
    assert_eq!(
        unsafe { soft_decode(encoded, &mut decode_store) },
        Some(DataUsizeEnum::B)
    );
}
