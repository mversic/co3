use co3::{ReprC, marker::AbiCompatible, niche::Niche, rust_spec::RustSpec};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(RustSpec, ReprC)]
#[repr(C, align(16))]
struct AlignedStruct(u8);

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[repr(align(16))]
struct SeparatelyAlignedStruct(u8);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct GenericCarrier<T>(T);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct ViewFields {
    first: u8,
    second: u32,
}

#[derive(RustSpec, ReprC)]
#[repr(u8, align(16))]
enum AlignedFieldlessEnum {
    #[repr_c(allow_zero_tag)]
    First,
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(u8, align(16))]
enum AlignedDataEnum {
    #[repr_c(allow_zero_tag)]
    First(u32),
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum MixedDataEnum {
    #[repr_c(allow_zero_tag)]
    First(u8, u32),
    Second {
        small: u16,
        large: u64,
    },
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8, align(32))]
enum ExplicitCAlignedDataEnum {
    #[repr_c(allow_zero_tag)]
    First(u32),
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(align(32))]
struct RustLayoutAligned(u8, u32);

fn assert_same_layout<T: ReprC<CType: Sized>>() {
    assert_eq!(core::mem::size_of::<T>(), core::mem::size_of::<T::CType>());
    assert_eq!(
        core::mem::align_of::<T>(),
        core::mem::align_of::<T::CType>()
    );
}

fn assert_nontrivial_alignment<
    T: RustSpec<__Alignment = co3::rust_spec::Gt<co3::rust_spec::One>>,
>() {
}

#[test]
fn explicit_repr_alignment_is_preserved_by_companion_types() {
    assert_same_layout::<AlignedStruct>();
    assert_same_layout::<SeparatelyAlignedStruct>();
    assert_same_layout::<AlignedFieldlessEnum>();
    assert_same_layout::<AlignedDataEnum>();
    assert_same_layout::<MixedDataEnum>();
    assert_same_layout::<ExplicitCAlignedDataEnum>();

    assert_nontrivial_alignment::<<AlignedStruct as ReprC>::CType>();
    assert_nontrivial_alignment::<<SeparatelyAlignedStruct as ReprC>::CType>();
    assert_nontrivial_alignment::<<AlignedFieldlessEnum as ReprC>::CType>();
    assert_nontrivial_alignment::<<AlignedDataEnum as ReprC>::CType>();
    assert_nontrivial_alignment::<<ExplicitCAlignedDataEnum as ReprC>::CType>();

    assert_impl_all!(AlignedStruct: AbiCompatible);
    assert_impl_all!(SeparatelyAlignedStruct: AbiCompatible);
    assert_impl_all!(AlignedFieldlessEnum: AbiCompatible);
    assert_impl_all!(AlignedDataEnum: AbiCompatible);
    assert_impl_all!(ExplicitCAlignedDataEnum: AbiCompatible);
}

#[test]
fn data_enum_carriers_construct_variants_and_report_tags() {
    assert_eq!(<AlignedDataEnum as ReprC>::CType::First(7).tag(), 0);
    assert_eq!(<AlignedDataEnum as ReprC>::CType::Second().tag(), 1);
    assert_eq!(
        <ExplicitCAlignedDataEnum as ReprC>::CType::First(7).tag(),
        0
    );
    assert_eq!(
        <ExplicitCAlignedDataEnum as ReprC>::CType::Second().tag(),
        1
    );
    assert_eq!(<MixedDataEnum as ReprC>::CType::First(3, 7).tag(), 0);
    assert_eq!(<MixedDataEnum as ReprC>::CType::Second(5, 11).tag(), 1);
    assert_eq!(
        format!("{:?}", <MixedDataEnum as ReprC>::CType::Second(5, 11)),
        "MixedDataEnumCType::Second { small: 5, large: 11 }"
    );
    assert!(
        <MixedDataEnum as ReprC>::CType::First(3, 7) < <MixedDataEnum as ReprC>::CType::First(3, 8)
    );

    let niche = <MixedDataEnum as Niche>::NICHE;
    assert_eq!(niche.tag(), u8::MAX);
    assert!(unsafe { co3::decode::<MixedDataEnum>(niche) }.is_none());

    let carrier = unsafe { <AlignedStruct as ReprC>::CType::from_fields(3) };
    let decoded: AlignedStruct = unsafe { co3::decode(carrier).unwrap() };
    assert_eq!(decoded.0, 3);

    let carrier = unsafe { <GenericCarrier<u32> as ReprC>::CType::from_fields(5) };
    let (field,) = carrier.into_fields();
    assert_eq!(field, 5);

    let carrier = unsafe { <GenericCarrier<u32> as ReprC>::CType::from_fields(field) };
    let decoded: GenericCarrier<u32> = unsafe { co3::decode(carrier).unwrap() };
    assert_eq!(decoded.0, 5);
}

#[test]
fn generated_carrier_exposes_fields_methods() {
    let carrier = co3::encode(ViewFields {
        first: 3,
        second: 4,
    });
    assert_eq!(carrier.into_fields(), (3, 4));
}

#[test]
fn alignment_without_a_stable_layout_is_preserved_without_transmute() {
    assert_eq!(core::mem::align_of::<RustLayoutAligned>(), 32);
    assert_eq!(
        core::mem::align_of::<<RustLayoutAligned as ReprC>::CType>(),
        32
    );
    assert_nontrivial_alignment::<<RustLayoutAligned as ReprC>::CType>();
    assert_not_impl_any!(RustLayoutAligned: AbiCompatible);
}
