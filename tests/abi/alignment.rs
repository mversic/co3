use co3::{ReprC, rust_spec::RustSpec, transmute::CheckedTransmute};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(RustSpec, ReprC)]
#[repr(C, align(16))]
struct AlignedStruct(u8);

#[derive(ReprC)]
#[repr(C)]
#[repr(align(16))]
struct SeparatelyAlignedStruct(u8);

#[derive(RustSpec, ReprC)]
#[repr(u8, align(16))]
enum AlignedFieldlessEnum {
    First,
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(u8, align(16))]
enum AlignedDataEnum {
    First(u32),
    Second,
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8, align(32))]
enum ExplicitCAlignedDataEnum {
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

fn assert_nontrivial_alignment<T: RustSpec<Alignment = co3::rust_spec::Gt<co3::rust_spec::One>>>() {
}

#[test]
fn explicit_repr_alignment_is_preserved_by_companion_types() {
    assert_same_layout::<AlignedStruct>();
    assert_same_layout::<SeparatelyAlignedStruct>();
    assert_same_layout::<AlignedFieldlessEnum>();
    assert_same_layout::<AlignedDataEnum>();
    assert_same_layout::<ExplicitCAlignedDataEnum>();

    assert_nontrivial_alignment::<<AlignedStruct as ReprC>::CType>();
    assert_nontrivial_alignment::<<SeparatelyAlignedStruct as ReprC>::CType>();
    assert_nontrivial_alignment::<<AlignedFieldlessEnum as ReprC>::CType>();
    assert_nontrivial_alignment::<<AlignedDataEnum as ReprC>::CType>();
    assert_nontrivial_alignment::<<ExplicitCAlignedDataEnum as ReprC>::CType>();

    assert_impl_all!(AlignedStruct: CheckedTransmute);
    assert_impl_all!(SeparatelyAlignedStruct: CheckedTransmute);
    assert_impl_all!(AlignedFieldlessEnum: CheckedTransmute);
    assert_impl_all!(AlignedDataEnum: CheckedTransmute);
    assert_impl_all!(ExplicitCAlignedDataEnum: CheckedTransmute);
}

#[test]
fn alignment_without_a_stable_layout_is_preserved_without_transmute() {
    assert_eq!(core::mem::align_of::<RustLayoutAligned>(), 32);
    assert_eq!(
        core::mem::align_of::<<RustLayoutAligned as ReprC>::CType>(),
        32
    );
    assert_nontrivial_alignment::<<RustLayoutAligned as ReprC>::CType>();
    assert_not_impl_any!(RustLayoutAligned: CheckedTransmute);
}
