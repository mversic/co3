//! C-compatible representations and conversions for [`core::cell`].
use crate::stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked};
use crate::{CFnArg, CFnReturn, CType, Decode, Encode, ReprC};
use core::cell::{Cell, UnsafeCell};
use rust_spec::RustSpec;
impl<R: ReprC + ?Sized> ReprC for UnsafeCell<R> {
    type CType = core::cell::UnsafeCell<R::CType>;
}

impl<R: Encode> Encode for UnsafeCell<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for UnsafeCell<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for UnsafeCell<R> where Self: DecodeMove<'d> {}

unsafe impl<T: EmptyStore> EmptyStore for UnsafeCell<T> {}

unsafe impl<R: CType + ?Sized> CType for UnsafeCell<R> {}
unsafe impl<T: CFnArg> CFnArg for UnsafeCell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for UnsafeCell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

impl<R: ReprC + ?Sized> ReprC for Cell<R> {
    type CType = core::cell::Cell<R::CType>;
}

impl<R: Encode> Encode for Cell<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for Cell<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for Cell<R> where Self: DecodeMove<'d> {}

unsafe impl<T: EmptyStore> EmptyStore for Cell<T> {}

unsafe impl<R: CType + ?Sized> CType for Cell<R> {}
unsafe impl<T: CFnArg> CFnArg for Cell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for Cell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

use crate::{EncodeUnchecked, stored::EmptyStore};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transmute::AbiCompatible;
    #[cfg(feature = "alloc")]
    use crate::{boxed::CBox, wide::CWideBox};
    use crate::{niche::Niche, option::COption};
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec, vec::Vec};
    use core::{mem::ManuallyDrop, num::NonZero};
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    #[test]
    fn cell_preserves_its_constructor_in_the_carrier() {
        use crate::primitives::CBool;

        assert_impl_all!(Cell<bool>: ReprC<CType = core::cell::Cell<crate::primitives::CBool>>);
        assert_impl_all!(UnsafeCell<bool>: ReprC<CType = core::cell::UnsafeCell<crate::primitives::CBool>>);

        let encoded = crate::encode(Cell::new(true));
        assert_eq!(encoded.into_inner(), CBool::TRUE);
        let decoded =
            unsafe { crate::decode::<Cell<bool>>(core::cell::Cell::new(CBool::TRUE)) }.unwrap();
        assert!(decoded.into_inner());
        assert!(unsafe { crate::decode::<Cell<bool>>(core::cell::Cell::new(CBool(2))) }.is_none());
    }

    #[test]
    fn robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<u8>: AbiCompatible);
        assert_impl_all!(Cell<u8>: AbiCompatible);
        assert_impl_all!([UnsafeCell<u8>; 2]: AbiCompatible);
        assert_not_impl_any!((UnsafeCell<u8>, u8): AbiCompatible);
        assert_impl_all!(ManuallyDrop<UnsafeCell<u8>>: AbiCompatible);

        assert_impl_all!(UnsafeCell<u8>:
            ReprC<CType = core::cell::UnsafeCell<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<u8>:
            Niche<CType = crate::reference::CRefMut<'static, UnsafeCell<u8>, false>>,
            AbiCompatible,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<u8>: Decode<'static>);
        assert_impl_all!(&mut UnsafeCell<u8>:
            Niche<CType = crate::reference::CRefMut<'static, UnsafeCell<u8>, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>:
            Niche<CType = CBox<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>: AbiCompatible);
        assert_impl_all!(&[UnsafeCell<u8>]:
            Niche<CType = crate::wide::CWideRefMut<'static, [UnsafeCell<u8>], false>>,
            Encode,
        );
        assert_impl_all!(&[UnsafeCell<u8>]: Decode<'static>);
        assert_impl_all!(&mut [UnsafeCell<u8>]:
            Niche<CType = crate::wide::CWideRefMut<'static, [UnsafeCell<u8>], true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<u8>]>:
            Niche<CType = CWideBox<[UnsafeCell<u8>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<u8>>:
            Niche<CType = crate::vec::CVec<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<u8>; 2]:
            ReprC<CType = [core::cell::UnsafeCell<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<u8>>:
            ReprC<CType = COption<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(UnsafeCell<u8>: CType);
    }

    #[test]
    fn mutable_unsafe_cell_reference_round_trip() {
        let mut cell = UnsafeCell::new(7_u8);
        let encoded = crate::encode(&mut cell);
        unsafe { encoded.as_ptr().write(core::cell::UnsafeCell::new(9)) };
        assert_eq!(*cell.get_mut(), 9);

        let source = unsafe {
            crate::reference::CRefMut::from_raw(cell.get().cast::<core::cell::UnsafeCell<u8>>())
        };
        let decoded = unsafe { crate::decode::<&mut UnsafeCell<u8>>(source) }.unwrap();
        *decoded.get_mut() = 11;
        assert_eq!(*cell.get_mut(), 11);

        let value = UnsafeCell::new(NonZero::new(3_u8).unwrap());
        let source = unsafe {
            crate::reference::CSoftRefMut::from_raw(
                value
                    .get()
                    .cast::<core::cell::UnsafeCell<crate::num::NonZero<u8>>>(),
            )
        };
        let decoded = unsafe { crate::decode::<&mut UnsafeCell<NonZero<u8>>>(source) }.unwrap();
        assert_eq!(decoded.get_mut().get(), 3);

        let invalid = UnsafeCell::new(0_u8);
        let source = unsafe {
            crate::reference::CSoftRefMut::from_raw(
                invalid
                    .get()
                    .cast::<core::cell::UnsafeCell<crate::num::NonZero<u8>>>(),
            )
        };
        assert!(unsafe { crate::decode::<&mut UnsafeCell<NonZero<u8>>>(source) }.is_none());
    }

    #[test]
    fn exclusive_slice_pointers_cover_cells_and_empty_slices() {
        let mut cells = [UnsafeCell::new(1_u8)];
        let view = crate::encode(&mut cells[..]);
        unsafe { view.as_ptr().write(core::cell::UnsafeCell::new(2)) };
        assert_eq!(unsafe { *cells[0].get() }, 2);

        let mut cells = [Cell::new(3_u8)];
        let view = crate::encode(&mut cells[..]);
        unsafe { view.as_ptr().write(core::cell::Cell::new(4)) };
        assert_eq!(cells[0].get(), 4);

        let mut empty_unsafe_cells: [UnsafeCell<u8>; 0] = [];
        assert_eq!(crate::encode(&mut empty_unsafe_cells[..]).metadata(), 0);
        let mut empty_cells: [Cell<u8>; 0] = [];
        assert_eq!(crate::encode(&mut empty_cells[..]).metadata(), 0);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn interior_mutable_slice_carriers_round_trip() {
        let mut cells = [UnsafeCell::new(3_u8), UnsafeCell::new(4_u8)];
        let view = crate::encode(&mut cells[..]);
        unsafe { view.as_ptr().write(core::cell::UnsafeCell::new(9)) };
        assert_eq!(unsafe { *cells[0].get() }, 9);

        let boxed = crate::encode(vec![UnsafeCell::new(5_u8)].into_boxed_slice());
        unsafe { boxed.as_ptr().write(core::cell::UnsafeCell::new(7)) };
        let boxed = unsafe { crate::decode::<Box<[UnsafeCell<u8>]>>(boxed) }.unwrap();
        assert_eq!(unsafe { *boxed[0].get() }, 7);

        let vector = crate::encode(vec![UnsafeCell::new(6_u8)]);
        let vector = unsafe { crate::decode::<Vec<UnsafeCell<u8>>>(vector) }.unwrap();
        assert_eq!(unsafe { *vector[0].get() }, 6);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn boxed_cell_borrow_and_round_trip() {
        let encoded = crate::encode(Box::new(UnsafeCell::new(4u8)));
        unsafe { encoded.as_ptr().write(core::cell::UnsafeCell::new(8)) };
        let decoded = unsafe { crate::decode::<Box<UnsafeCell<u8>>>(encoded) }.unwrap();
        assert_eq!((*decoded).into_inner(), 8);

        let encoded = crate::encode(Box::new(Cell::new(4u8)));
        unsafe { encoded.as_ptr().write(core::cell::Cell::new(6)) };
        let decoded = unsafe { crate::decode::<Box<Cell<u8>>>(encoded) }.unwrap();
        assert_eq!(decoded.get(), 6);

        let none = crate::encode(None::<Box<UnsafeCell<u8>>>);
        assert!(none.0.as_ptr().is_null());
        assert!(matches!(
            unsafe { crate::decode::<Option<Box<UnsafeCell<u8>>>>(none) },
            Some(None)
        ));
    }

    #[test]
    fn non_robust_mutable_cell_syncs_after_soft_encode() {
        use crate::stored::Store;

        let mut value = Cell::new(NonZero::new(3_u8).unwrap());
        let mut store = Default::default();
        let encoded = crate::soft_encode(&mut value, &mut store);
        unsafe {
            encoded
                .as_ptr()
                .write(core::cell::Cell::new(crate::num::NonZero(4)))
        };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value.get().get(), 4);
    }

    #[test]
    fn mutable_non_robust_soft_carrier_keeps_unchecked_encoding_direct() {
        use crate::stored::Store;
        type CellValue = UnsafeCell<NonZero<u8>>;
        assert_impl_all!(&'static mut CellValue:
            crate::stored::EncodeMoveUnchecked<SoftStore = ()>,
            ReprC<CType = crate::reference::CSoftRefMut<CellValue, true>>
        );

        let mut value = UnsafeCell::new(NonZero::new(3_u8).unwrap());
        let original = core::ptr::from_mut(&mut value).cast();
        let unchecked: crate::reference::CSoftRefMut<CellValue, true> =
            unsafe { crate::soft_encode_unchecked(&mut value, &mut ()) };
        assert_eq!(unchecked.as_ptr(), original);

        let mut store = Default::default();
        let checked: crate::reference::CSoftRefMut<CellValue, true> =
            crate::soft_encode(&mut value, &mut store);
        assert_ne!(checked.as_ptr(), original);
        unsafe {
            checked
                .as_ptr()
                .write(core::cell::UnsafeCell::new(crate::num::NonZero(0)))
        };
        assert_eq!(store.sync(), None);
        assert_eq!(value.get_mut().get(), 3);
    }

    #[test]
    fn non_robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<NonZero<u8>>:
            ReprC<CType = core::cell::UnsafeCell<crate::num::NonZero<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<NonZero<u8>>:
            Niche<CType = crate::reference::CRefMut<'static, UnsafeCell<NonZero<u8>>, false>>,
        );
        assert_not_impl_any!(&UnsafeCell<NonZero<u8>>: Decode<'static>, AbiCompatible);
        assert_impl_all!(&mut UnsafeCell<NonZero<u8>>:
            Niche<CType = crate::reference::CSoftRefMut<UnsafeCell<NonZero<u8>>, true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<NonZero<u8>>>:
            Niche<CType = CBox<UnsafeCell<NonZero<u8>>>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        {
            // SAFETY: this test constructs the carrier for an interior-mutable cell value.
            let valid = CBox::from_box(Box::new(core::cell::UnsafeCell::new(crate::num::NonZero(
                7u8,
            ))));
            let decoded = unsafe { crate::decode::<Box<UnsafeCell<NonZero<u8>>>>(valid) }.unwrap();
            assert_eq!((*decoded).into_inner().get(), 7);

            // SAFETY: this test constructs the carrier for an interior-mutable cell value.
            let invalid = CBox::from_box(Box::new(core::cell::UnsafeCell::new(
                crate::num::NonZero(0u8),
            )));
            // Miri verifies that rejection also releases this owned allocation.
            assert!(unsafe { crate::decode::<Box<UnsafeCell<NonZero<u8>>>>(invalid) }.is_none());

            // A null carrier is rejected without attempting to reclaim an allocation.
            assert!(
                unsafe { crate::decode::<Box<UnsafeCell<NonZero<u8>>>>(CBox::null()) }.is_none()
            );
        }
        assert_impl_all!(&[UnsafeCell<NonZero<u8>>]:
            Niche<CType = crate::wide::CWideRefMut<'static, [UnsafeCell<NonZero<u8>>], false>>,
        );
        assert_not_impl_any!(&[UnsafeCell<NonZero<u8>>]: Decode<'static>);
        assert_impl_all!(&mut [UnsafeCell<NonZero<u8>>]:
            Niche<CType = crate::wide::CSoftWideRefMut<[UnsafeCell<NonZero<u8>>], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZero<u8>>]>:
            Niche<CType = CWideBox<[UnsafeCell<NonZero<u8>>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZero<u8>>>:
            Niche<CType = crate::vec::CVec<UnsafeCell<core::num::NonZeroU8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        {
            let invalid = crate::vec::CVec::from_vec(vec![core::cell::UnsafeCell::new(
                crate::num::NonZero(0_u8),
            )]);
            assert!(unsafe { crate::decode::<Vec<UnsafeCell<NonZero<u8>>>>(invalid) }.is_none());

            let invalid = CWideBox::from_box(
                vec![core::cell::UnsafeCell::new(crate::num::NonZero(0_u8))].into_boxed_slice(),
            );
            assert!(unsafe { crate::decode::<Box<[UnsafeCell<NonZero<u8>>]>>(invalid) }.is_none());
        }
        assert_impl_all!([UnsafeCell<NonZero<u8>>; 2]:
            ReprC<CType = [core::cell::UnsafeCell<crate::num::NonZero<u8>>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZero<u8>>>:
            ReprC<CType = COption<UnsafeCell<core::num::NonZeroU8>>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<NonZero<u8>>: CType);
    }
}
