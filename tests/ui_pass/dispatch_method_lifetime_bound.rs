use co3::{ReprC,
    Tag, ffi,
    tag::{Tagged, TagFamily},
};
use co3::rust_spec::RustSpec;

trait Attribute {}

trait Dispatch {
    fn len(values: &[u32], attr: &Self) -> usize;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct CustomAttribute(u32);

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8)]
#[repr(transparent)]
struct CustomAttributeRef<'a>(&'a u32);

impl Attribute for &CustomAttribute {}
impl TagFamily for &CustomAttribute {
    type Kind = u8;
}

unsafe impl Tagged for &CustomAttribute {
    const TAG: u8 = 1;
}

mod provider {
    use super::*;

    trait Dispatch {
        fn len<T>(values: &[u32], attr: &&T) -> usize;
    }

    impl Dispatch for &CustomAttribute {
        fn len<T>(_values: &[u32], _attr: &&T) -> usize {
            3
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<'a, dyn(u8) T: 'a + 'a> Dispatch for T
        where
            T: Attribute + 'a + 'a,
            use<T> @ <&CustomAttribute>,
        {
            #[symbol_name = "len"]
            fn len(values: &[u32], #[soft] attr: &T) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<'a, dyn(u8) T: Attribute + 'a> Dispatch for T
    where
        use<T> @ <&CustomAttribute>,
    {
        #[symbol_name = "len"]
        fn len(tag_id: <dyn T>::TAG, values: &[u32], attr: &T) -> usize;
    }
}

fn main() {
    assert_eq!(<&CustomAttribute>::len(&[1, 2, 3], &&CustomAttribute(0)), 3);
}
