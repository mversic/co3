#![allow(unused_parens)]

use co3::{ReprC, ffi, rust_spec::RustSpec, slice::Unpack2};

trait ExportUnpackLen {
    fn export_trait_unpack_len(&self, _: *const u32, len: usize) -> usize;
}

trait ImportUnpackLen {
    fn import_trait_unpack_len(&self, values: &[u32]) -> usize;
}

trait TargetType {
    type Value: ?Sized;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Counter(usize);

#[derive(RustSpec, ReprC, co3::Tag)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct ByteTarget(u8);

impl TargetType for ByteTarget {
    type Value = [u8];
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct OdbcStr<C>([C]);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct PairParts(u8, u8);

impl Unpack2<u32, i32> for PairParts {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u32, i32), Self::Error> {
        Ok((value.0.into(), value.1.into()))
    }
}

mod c_symbols {
    use super::*;

    fn unpack_len_impl(_: *const u32, len: usize) -> usize {
        len
    }

    fn unpack_convert_export(data: u32, metadata: i32) -> u32 {
        data + metadata as u32
    }

    fn optional_slice_len_impl(data: *const u32, len: usize) -> usize {
        assert_eq!(data.is_null(), len == 0);
        len
    }

    fn optional_slice_mut_len_impl(data: *mut u32, len: usize) -> usize {
        assert_eq!(data.is_null(), len == 0);
        len
    }

    #[unsafe(no_mangle)]
    extern "C" fn dispatch_associated_unpack(_: u8, data: *mut u8, len: usize) -> usize {
        assert_eq!(data.is_null(), len == 0);
        len
    }

    impl Counter {
        fn inherent_unpack_len(&self, _: *const u32, len: usize) -> usize {
            self.0 + len
        }
    }

    impl ExportUnpackLen for Counter {
        fn export_trait_unpack_len(&self, _: *const u32, len: usize) -> usize {
            self.0 + len + 1
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[symbol_name = "unpack_len_impl"]
        fn unpack_len_impl(_data: *const u32, len: usize) -> usize;

        #[symbol_name = "unpack_convert"]
        fn unpack_convert_export(data: u32, metadata: i32) -> u32;

        #[symbol_name = "optional_slice_len_impl"]
        fn optional_slice_len_impl(data: *const u32, len: usize) -> usize;

        #[symbol_name = "optional_slice_mut_len_impl"]
        fn optional_slice_mut_len_impl(data: *mut u32, len: usize) -> usize;

        impl Counter {
            #[symbol_name = "inherent_unpack_len"]
            fn inherent_unpack_len(&self, _data: *const u32, len: usize) -> usize;
        }

        impl ExportUnpackLen for Counter {
            #[symbol_name = "export_trait_unpack_len"]
            fn export_trait_unpack_len(&self, _data: *const u32, len: usize) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "unpack_len_impl"]
    fn unpack_len(#[unpack(_, _)] values: &[u32]) -> usize;

    #[symbol_name = "unpack_convert"]
    fn unpack_convert(#[unpack(u32, i32)] value: move PairParts) -> u32;

    fn unary_inferred(#[unpack(_)] value: u32);

    #[symbol_name = "optional_slice_len_impl"]
    fn optional_slice_len(#[unpack(_, _)] values: Option<&[u32]>) -> usize;

    #[symbol_name = "optional_slice_mut_len_impl"]
    fn optional_slice_mut_len(#[unpack(_, usize)] values: Option<&mut [u32]>) -> usize;

    fn borrowed_box(#[unpack(*const u32, usize)] value: Box<[u32]>);
    fn moved_box(#[unpack(_, _)] value: move Box<[u32]>);
    fn optional_moved_box(#[unpack(_, _)] value: move Option<Box<[u32]>>);
    fn parenthesized_ref(#[unpack(_, _)] value: (&[u32]));
    fn parenthesized_tuple(#[unpack(_, _)] value: move ((u8, u16)));

    #[symbol_name = "static_unpack_{C}"]
    fn static_unpack<C>(
        #[unpack(_, _)] value: move (C, u8),
    )
    where
        use<C> @ (<u8> | <u16>);

    #[symbol_name = "dispatch_associated_unpack"]
    fn dispatch_associated_unpack<dyn(u8) T: TargetType>(
        #[unpack(*mut u8, usize)] value: Option<&mut <T as TargetType>::Value>,
    ) -> usize
    where
        use<T> @ <ByteTarget>;

    impl Counter {
        #[symbol_name = "inherent_unpack_len"]
        fn imported_inherent_unpack_len(&self, #[unpack(_, _)] values: &[u32]) -> usize;
    }

    impl ImportUnpackLen for Counter {
        #[symbol_name = "export_trait_unpack_len"]
        fn import_trait_unpack_len(&self, #[unpack(_, usize)] values: &[u32]) -> usize;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(i16, unsafe(1))]
    type Statement;

    impl Statement {
        #[symbol_name = "static_method_unpack_dst_{C}"]
        fn prepare<C>(&self, #[unpack(_, i16)] text: &OdbcStr<C>)
        where
            use<C> @ (<u8> | <u16>);

        #[symbol_name = "static_method_unpack_dst_{C}_2"]
        fn prepare2<C>(&self, #[unpack(*const C, i16)] text: &OdbcStr<C>)
        where
            use<C> @ (<u8> | <u16>);

        #[symbol_name = "static_method_multiple_unpack_dst_{C}"]
        fn prepare_pair<C>(
            &self,
            #[unpack(_, i16)] first: &OdbcStr<C>,
            #[unpack(_, i16)] second: &OdbcStr<C>,
        )
        where
            use<C> @ (<u8> | <u16>);
    }
}

mod aliased_core_option {
    use super::*;

    type Option<T> = core::option::Option<T>;

    ffi! {
        #![unsafe(extern("C"))]

        fn aliased_optional_slice(#[unpack(_, _)] values: Option<&[u32]>);
    }
}

fn main() {
    assert_eq!(unpack_len(&[1, 2, 3]), 3);
    assert_eq!(unpack_convert(PairParts(2, 3)), 5);
    assert_eq!(optional_slice_len(Some(&[1, 2, 3])), 3);
    assert_eq!(optional_slice_len(None), 0);

    let mut values = [1, 2, 3, 4];
    assert_eq!(optional_slice_mut_len(Some(&mut values)), 4);
    assert_eq!(optional_slice_mut_len(None), 0);

    let mut bytes = [1, 2, 3];
    assert_eq!(
        dispatch_associated_unpack::<ByteTarget>(Some(&mut bytes)),
        3
    );
    assert_eq!(dispatch_associated_unpack::<ByteTarget>(None), 0);

    let counter = Counter(10);
    assert_eq!(counter.imported_inherent_unpack_len(&[1, 2, 3]), 13);
    assert_eq!(counter.import_trait_unpack_len(&[1, 2, 3]), 14);
}
