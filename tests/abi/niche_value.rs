use core::cmp::Ordering;

use co3::{Encode, ReprC, export_C, extern_C};
use webassembly_test::webassembly_test;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ReprC)]
#[reprC(opaque)]
#[allow(unused)]
pub enum Opaque {
    A,
}

export_C! {
    impl Drop for Opaque {
        fn drop(&mut self);
    }
}

extern_C! {
    pub type Extern;

    impl Drop for Extern {
        fn drop(&mut self);
    }
}

#[derive(Clone, Copy, ReprC)]
#[allow(unused)]
#[repr(u8)]
pub enum FieldlessUEnum {
    Var1,
    Var2,
    Var3,
    Var4,
}

#[derive(Clone, Copy, ReprC)]
#[allow(unused)]
#[repr(i8)]
pub enum FieldlessIEnum {
    Var1,
    Var2,
    Var3,
    Var4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ReprC)]
pub enum FieldlessNoReprEnum {
    A,
    B,
    C,
    D,
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[allow(unused)]
#[repr(C, i8)]
pub enum ReprCDataEnum<'a, T> {
    A(&'a [u32; 2]),
    B(u32),
    C(T),
    D,
}

#[derive(Debug, Clone, PartialEq, Eq, ReprC)]
#[allow(unused)]
#[repr(i8)]
pub enum DataEnum<'a, T> {
    A(&'a [u32; 2]),
    B(u32),
    C(T),
    D,
}

#[derive(Clone, Copy, ReprC)]
#[allow(unused)]
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

#[cfg(target_family = "wasm")]
#[webassembly_test]
fn wasm_niche_value() {
    assert_eq!(u32::MAX, None::<u8>.encode(&mut ()));
    assert_eq!(i32::MAX, None::<i8>.encode(&mut ()));
    assert_eq!(u32::MAX, None::<u16>.encode(&mut ()));
    assert_eq!(i32::MAX, None::<i16>.encode(&mut ()));
}

#[test]
#[webassembly_test]
fn verify_enum_niche_value() {
    #[cfg(not(target_family = "wasm"))]
    let expected_bool = 2_u8;
    #[cfg(target_family = "wasm")]
    let expected_bool = 2_u32;
    #[cfg(not(target_family = "wasm"))]
    let expected_ord = 2_i8;
    #[cfg(target_family = "wasm")]
    let expected_ord = 2_i32;

    assert_eq!(expected_bool, None::<bool>.encode(&mut ()));
    assert_eq!(expected_ord, None::<Ordering>.encode(&mut ()));

    assert!(None::<Opaque>.encode(&mut ()).is_null());
    assert!(None::<Extern>.encode(&mut ()).is_null());

    #[cfg(not(target_family = "wasm"))]
    let expected_niche_enum_discriminant_u = 4_u8;
    #[cfg(target_family = "wasm")]
    let expected_niche_enum_discriminant_u = 4_u32;
    #[cfg(not(target_family = "wasm"))]
    let expected_niche_enum_discriminant_i = 4_i8;
    #[cfg(target_family = "wasm")]
    let expected_niche_enum_discriminant_i = 4_i32;

    assert_eq!(
        expected_niche_enum_discriminant_u,
        None::<FieldlessUEnum>.encode(&mut ())
    );
    assert_eq!(
        expected_niche_enum_discriminant_i,
        None::<FieldlessIEnum>.encode(&mut ())
    );
    assert_eq!(
        expected_niche_enum_discriminant_u,
        None::<FieldlessNoReprEnum>.encode(&mut ())
    );

    let encoded = None::<ReprCDataEnum<&u8>>.encode(&mut ());
    assert_eq!(expected_niche_enum_discriminant_i, encoded.tag);
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::from_ref(&encoded.payload).cast::<u8>(),
            core::mem::size_of_val(&encoded.payload),
        )
    };
    assert!(bytes.iter().all(|&byte| byte == 0));

    let encoded = None::<DataEnum<&u8>>.encode(&mut ());
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::from_ref(&encoded).cast::<i8>(),
            core::mem::size_of_val(&encoded),
        )
    };
    assert_eq!(expected_niche_enum_discriminant_i, bytes[0]);
    assert!(bytes[1..].iter().all(|&byte| byte == 0));

    #[cfg(not(target_family = "wasm"))]
    let expected_fieldless_large_enum = 256u16;
    #[cfg(target_family = "wasm")]
    let expected_fieldless_large_enum = 256u32;

    assert_eq!(
        expected_fieldless_large_enum,
        None::<FieldlessLargeEnum>.encode(&mut ())
    );
}
