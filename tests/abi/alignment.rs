use core::cell::Cell;

use co3::{ReprC, rust_spec::RustSpec, transmute::CheckedTransmute};
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
struct InteriorMutableField(Cell<u8>);

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
fn data_enum_carriers_construct_variants_and_report_tags() {
    assert_eq!(CAlignedDataEnum::First(7).tag(), 0);
    assert_eq!(CAlignedDataEnum::Second().tag(), 1);
    assert_eq!(CExplicitCAlignedDataEnum::First(7).tag(), 0);
    assert_eq!(CExplicitCAlignedDataEnum::Second().tag(), 1);

    let carrier = unsafe { CAlignedStruct::from_fields(3) };
    let decoded: AlignedStruct = unsafe { co3::decode(carrier).unwrap() };
    assert_eq!(decoded.0, 3);

    let carrier = unsafe { CGenericCarrier::<u32>::from_fields(5) };
    let (field,) = carrier.into_fields();
    assert_eq!(field, 5);

    let carrier = unsafe { CGenericCarrier::<u32>::from_fields(field) };
    let decoded: GenericCarrier<u32> = unsafe { co3::decode(carrier).unwrap() };
    assert_eq!(decoded.0, 5);
}

#[test]
fn generated_view_structs_expose_fields_methods() {
    let view = unsafe { ViewFieldsView::from_fields(1, 2) };
    assert_eq!(view.into_fields(), (1, 2));

    let const_view = unsafe { CViewFieldsConstView::from_fields(3, 4) };
    assert_eq!(const_view.into_fields(), (3, 4));

    let mut_view = unsafe { CViewFieldsMutView::from_fields(5, 6) };
    assert_eq!(mut_view.into_fields(), (5, 6));
}

#[test]
fn generated_view_keeps_owner_axes_and_recomputes_mutability() {
    assert_impl_all!(InteriorMutableField:
        RustSpec<Mutability = co3::rust_spec::mutability::Interior>,
    );
    assert_impl_all!(InteriorMutableFieldView<'static>:
        RustSpec<
            Layout = <InteriorMutableField as RustSpec>::Layout,
            Size = <InteriorMutableField as RustSpec>::Size,
            Alignment = <InteriorMutableField as RustSpec>::Alignment,
            Trap = <InteriorMutableField as RustSpec>::Trap,
            Niche = <InteriorMutableField as RustSpec>::Niche,
            Drop = co3::rust_spec::drop::NoDrop,
            Mutability = co3::rust_spec::mutability::Exclusive,
            __IndirectTrap = <InteriorMutableField as RustSpec>::__IndirectTrap,
        >,
    );
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
