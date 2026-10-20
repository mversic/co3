use co3::{ReprC, rust_spec::RustSpec};
use static_assertions::assert_type_eq_all;

macro_rules! exhaustive_enums {
    ($first:ident, $($variant:ident),* $(,)?) => {
        #[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
        #[repr(u8)]
        enum AllTags { $first, $($variant,)* }

        #[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
        #[allow(dead_code)]
        #[repr(u8)]
        #[repr_c(identity)]
        enum AllTagsIdentity { $first, $($variant,)* }

        #[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
        #[allow(dead_code)]
        #[repr(u8)]
        #[repr_c(identity)]
        enum AllTagsDataIdentity { $first(u32), $($variant,)* }

        #[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
        #[repr(u8)]
        #[rust_spec(custom_drop)]
        enum AllTagsWithDrop { $first, $($variant,)* }

        impl Drop for AllTagsWithDrop {
            fn drop(&mut self) {}
        }
    };
}

exhaustive_enums! { V0, V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11, V12, V13, V14, V15, V16, V17, V18, V19, V20, V21, V22, V23, V24, V25, V26, V27, V28, V29, V30, V31, V32, V33, V34, V35, V36, V37, V38, V39, V40, V41, V42, V43, V44, V45, V46, V47, V48, V49, V50, V51, V52, V53, V54, V55, V56, V57, V58, V59, V60, V61, V62, V63, V64, V65, V66, V67, V68, V69, V70, V71, V72, V73, V74, V75, V76, V77, V78, V79, V80, V81, V82, V83, V84, V85, V86, V87, V88, V89, V90, V91, V92, V93, V94, V95, V96, V97, V98, V99, V100, V101, V102, V103, V104, V105, V106, V107, V108, V109, V110, V111, V112, V113, V114, V115, V116, V117, V118, V119, V120, V121, V122, V123, V124, V125, V126, V127, V128, V129, V130, V131, V132, V133, V134, V135, V136, V137, V138, V139, V140, V141, V142, V143, V144, V145, V146, V147, V148, V149, V150, V151, V152, V153, V154, V155, V156, V157, V158, V159, V160, V161, V162, V163, V164, V165, V166, V167, V168, V169, V170, V171, V172, V173, V174, V175, V176, V177, V178, V179, V180, V181, V182, V183, V184, V185, V186, V187, V188, V189, V190, V191, V192, V193, V194, V195, V196, V197, V198, V199, V200, V201, V202, V203, V204, V205, V206, V207, V208, V209, V210, V211, V212, V213, V214, V215, V216, V217, V218, V219, V220, V221, V222, V223, V224, V225, V226, V227, V228, V229, V230, V231, V232, V233, V234, V235, V236, V237, V238, V239, V240, V241, V242, V243, V244, V245, V246, V247, V248, V249, V250, V251, V252, V253, V254, V255 }

#[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
#[repr_c(transparent)]
enum TransparentTags {
    #[repr_c(allow_zero_tag)]
    First = 0,
    Second = 1,
}

#[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
#[repr_c(transparent)]
enum TransparentUnit {
    Only,
}

#[test]
fn transparent_fieldless_enums_use_the_tag_or_unit() {
    assert_type_eq_all!(<TransparentTags as ReprC>::CType, u8);
    assert_type_eq_all!(<TransparentUnit as ReprC>::CType, ());
    assert_eq!(co3::encode(TransparentTags::Second), 1u8);
    assert_eq!(
        unsafe { co3::decode::<TransparentTags>(1u8) },
        Some(TransparentTags::Second)
    );
    assert_eq!(unsafe { co3::decode::<TransparentTags>(2u8) }, None);
    assert_eq!(co3::encode(TransparentUnit::Only), ());
    assert_eq!(
        unsafe { co3::decode::<TransparentUnit>(()) },
        Some(TransparentUnit::Only)
    );
}

#[test]
fn exhaustive_enum_carrier_depends_on_drop() {
    assert_type_eq_all!(<AllTags as ReprC>::CType, u8);
    assert_type_eq_all!(<AllTagsIdentity as ReprC>::CType, AllTagsIdentity);
    assert_type_eq_all!(<AllTagsDataIdentity as ReprC>::CType, AllTagsDataIdentity);
    assert_eq!(co3::encode(AllTagsIdentity::V255), AllTagsIdentity::V255);
    assert_eq!(
        co3::encode(AllTagsDataIdentity::V0(42)),
        AllTagsDataIdentity::V0(42)
    );
    assert_eq!(
        unsafe { co3::decode::<AllTagsDataIdentity>(AllTagsDataIdentity::V0(42)) },
        Some(AllTagsDataIdentity::V0(42))
    );
    assert_type_eq_all!(<AllTagsWithDrop as ReprC>::CType, CAllTagsWithDrop);
    static_assertions::assert_not_impl_any!(AllTags: co3::niche::Niche);
    static_assertions::assert_not_impl_any!(AllTagsWithDrop: co3::niche::Niche);

    assert_eq!(co3::encode(AllTags::V0), 0u8);
    assert_eq!(co3::encode(AllTags::V255), 255u8);
    assert_eq!(
        unsafe { co3::decode::<AllTags>(255u8) },
        Some(AllTags::V255)
    );

    assert_eq!(co3::encode(AllTagsWithDrop::V0).tag(), 0u8);
    assert_eq!(co3::encode(AllTagsWithDrop::V255).tag(), 255u8);
    assert_eq!(
        unsafe { co3::decode::<AllTagsWithDrop>(CAllTagsWithDrop::V255()) },
        Some(AllTagsWithDrop::V255),
    );
}
