#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use super::*;
use crate::{
    dst::{ExternTypeLike, Sized_},
    external::{ExternRef, ExternRefMut},
    ir::Transmuted,
    option::COption,
};

/// Marker for a ZST(zero-sized type)
///
/// # Safety
///
/// Type must be a ZST
pub unsafe trait Zst {}

unsafe impl Zst for () {}
unsafe impl<T: Zst> Zst for Option<T> {}
unsafe impl<T: Zst, E: Zst> Zst for Result<T, E> {}
unsafe impl<T: Zst, const N: usize> Zst for [T; N] {}
// TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
//unsafe impl<T> Zst for [T; 0] {}
unsafe impl<T> Zst for core::marker::PhantomData<T> {}
unsafe impl<T: Zst> Zst for core::mem::ManuallyDrop<T> {}
unsafe impl<T: Zst> Zst for core::cell::UnsafeCell<T> {}

disjoint_impls! {
    /// Facilitates the use of [`Self`] as out-pointer.
    ///
    /// If a type implements [`Repr`], i.e. has a defined internal representation,
    /// a blanket implementation is provided.
    pub trait OutPtr: ExternC {
        /// Type of the out-pointer
        type OutPtr: ReprC;
    }

    impl<R: ReprC> OutPtr for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type OutPtr = Self::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R> OutPtr for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        type OutPtr = Self::CType;
    }
    impl<R: CheckedTransmute<Target: Sized>> OutPtr for R
    where
        Self: ReprFamily<Kind = Transmuted>,
        <R as CheckedTransmute>::Target: OutPtr,
    {
        type OutPtr = <R::Target as OutPtr>::OutPtr;
    }

    impl<'a, R: ?Sized + DstFamily<Kind = SliceLike> + SliceDst<Elem: ReprC>> OutPtr for &'a R
    where
        Self: ReprFamily<Kind = &'a Robust>,
    {
        type OutPtr = Self::CType;
    }
    //impl<'a, R: ?Sized> OutPtr for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a Opaque>,
    //{
    //    type OutPtr = CBoxedSlice<*const R>;
    //}
    impl<'a, R: ?Sized + CheckedTransmute> OutPtr for &'a R
    where
        &'a <R as CheckedTransmute>::Target: OutPtr,
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type OutPtr = <&'a R::Target as OutPtr>::OutPtr;
    }
    impl<'a, R: CheckedTransmute> OutPtr for &'a R
    where
        Self: ReprFamily<Kind = &'a Transmuted>,
        R: DstFamily<Kind = ExternTypeLike>,
        ExternRef<'a, R>: OutPtr,
    {
        type OutPtr = <ExternRef<'a, R> as OutPtr>::OutPtr;
    }
    impl<'a, R: ExternC, S: Cloned> OutPtr for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: DstFamily<Kind = Sized_>,
    {
        type OutPtr = R::CType;
    }
    //impl<'a, R: ?Sized, S: Cloned> OutPtr for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a S>,
    //    R: SizeFamily<Kind = UnSized>,
    //{
    //    type OutPtr = Self::CType;
    //}

    impl<'a, R: ?Sized + DstFamily<Kind = SliceLike> + SliceDst<Elem: ReprC>> OutPtr for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Robust>,
    {
        type OutPtr = Self::CType;
    }
    impl<'a, R: ?Sized + CheckedTransmute> OutPtr for &'a mut R
    where
        &'a mut <R as CheckedTransmute>::Target: OutPtr,
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = SliceLike>,
    {
        type OutPtr = <&'a mut R::Target as OutPtr>::OutPtr;
    }
    impl<'a, R: CheckedTransmute> OutPtr for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Transmuted>,
        R: DstFamily<Kind = ExternTypeLike>,
        ExternRefMut<'a, R>: OutPtr,
    {
        type OutPtr = <ExternRefMut<'a, R> as OutPtr>::OutPtr;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized + DstFamily<Kind = SliceLike> + SliceDst<Elem: ReprC>> OutPtr for Box<R>
    where
        Self: ReprFamily<Kind = Box<Robust>>,
    {
        type OutPtr = Self::CType;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized + CheckedTransmute> OutPtr for Box<R>
    where
        Box<<R as CheckedTransmute>::Target>: OutPtr,
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = SliceLike>,
    {
        type OutPtr = <Box<R::Target> as OutPtr>::OutPtr;
    }
    #[cfg(feature = "alloc")]
    impl<R: External + OutPtr> OutPtr for Box<R>
    where
        Self: ReprFamily<Kind = Box<Transmuted>>,
        R: DstFamily<Kind = ExternTypeLike>,
    {
        type OutPtr = R::OutPtr;
    }
    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized + Dst> OutPtr for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Opaque>>,
    //{
    //    type OutPtr = CBoxedSlice<CBox<R>>;
    //}
    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: Cloned> OutPtr for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
        R: DstFamily<Kind = Sized_>,
    {
        type OutPtr = R::CType;
    }
    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized, S: Cloned> OutPtr for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //    R: SizeFamily<Kind = UnSized>,
    //{
    //    type OutPtr = CBoxedSlice<R::CType>;
    //}

    #[cfg(feature = "alloc")]
    impl<R, S> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
        Box<[R]>: OutPtr,
    {
        type OutPtr = <Box<[R]> as OutPtr>::OutPtr;
    }

    impl<R: ExternC, S: Cloned, const N: usize> OutPtr for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]>,
    {
        type OutPtr = Self::CType;
    }

    impl<R: OutPtr> OutPtr for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        type OutPtr = COption<R::OutPtr>;
    }
    impl<R: Niche + OutPtr> OutPtr for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    {
        type OutPtr = R::OutPtr;
    }
}

disjoint_impls! {
    /// Facilitates writing [`Self`] into [`Self::OutPtr`].
    pub trait OutPtrWrite: OutPtr {
        /// Write the given rust value into the corresponding out-pointer
        ///
        /// # Safety
        ///
        /// [`*mut Self::OutPtr`] must be valid
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr);
    }

    impl<R: ReprC> OutPtrWrite for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let ctype = EncodeWithStore::encode(self, &mut ());

            unsafe {
                out_ptr.write(ctype);
            }
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ReprFamily<Kind = Opaque>> OutPtrWrite for R {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let ctype = EncodeWithStore::encode(self, &mut ());

            unsafe {
                out_ptr.write(ctype);
            }
        }
    }
    impl<R: CheckedTransmute<Target: Sized>> OutPtrWrite for R
    where
        Self: ReprFamily<Kind = Transmuted>,
        <R as CheckedTransmute>::Target: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target(self);
            unsafe { OutPtrWrite::write_out(transmuted, out_ptr) }
        }
    }

    //impl<'itm, R: Encode + Clone, S: Cloned> OutPtrWrite for &'itm R
    //where
    //    Self: ReprFamily<Kind = &'itm S>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let mut store = Default::default();
    //        let _ = self.encode(&mut store);
    //        let output = store.ctype.unwrap();

    //        unsafe {
    //            out_ptr.write(output);
    //        }
    //    }
    //}

    //#[cfg(feature = "alloc")]
    //impl<R: Encode, S: Cloned> OutPtrWrite for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //{
    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
    //        unimplemented!()
    //        //let mut store = Default::default();
    //        //let _ = self.encode(&mut store);
    //        //let output = store.ctype.unwrap();

    //        //unsafe {
    //        //    out_ptr.write(output);
    //        //}
    //    }
    //}

    //impl<'a, R: ?Sized + Dst<Data: ReprC>> OutPtrWrite for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a Robust>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let ctypes = self.encode(&mut ());

    //        unsafe {
    //            out_ptr.write(ctypes);
    //        }
    //    }
    //}
    //impl<R: ?Sized> OutPtrWrite for &R
    //where
    //    Self: ReprFamily<Kind = Opaque>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let mut store = Default::default();
    //        let _ = self.encode(&mut store);

    //        let output = CBoxedSlice::from_boxed_slice(store.0);

    //        unsafe {
    //            out_ptr.write(output);
    //        }
    //    }
    //}
    //impl<'a, R: CheckedTransmute<Target: Sized + 'a>> OutPtrWrite for &'a R
    //where
    //    &'a <R as CheckedTransmute>::Target: OutPtrWrite,
    //    Self: ReprFamily<Kind = &'a Transmuted>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let transmuted = transmute_into_target_ref_slice(self);

    //        unsafe {
    //            OutPtrWrite::write_out(transmuted, out_ptr);
    //        }
    //    }
    //}
    //#[cfg(feature = "unstable-refs")]
    //impl<R: Encode + Clone, S: Cloned> OutPtrWrite for &R
    //where
    //    Self: ReprFamily<Kind = S>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let mut store = Default::default();
    //        let _ = self.encode(&mut store);

    //        let output = CBoxedSlice::from_boxed_slice(store.ctypes);

    //        unsafe {
    //            out_ptr.write(output);
    //        }
    //    }
    //}

    //impl<'a, R: CheckedTransmute<Target: Sized + 'a>> OutPtrWrite for &'a mut R
    //where
    //    &'a mut <R as CheckedTransmute>::Target: OutPtrWrite,
    //    Self: ReprFamily<Kind = &'a mut Transmuted>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let transmuted = transmute_into_target_slice_mut(self);

    //        unsafe {
    //            OutPtrWrite::write_out(transmuted, out_ptr);
    //        }
    //    }
    //}

    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized + Dst<Data: ReprC>> OutPtrWrite for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Robust>>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let output = self.encode(&mut ());

    //        unsafe {
    //            out_ptr.write(output);
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized> OutPtrWrite for Box<R>
    //where
    //    Self: ReprFamily<Kind = Opaque>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let output = self.encode(&mut ());

    //        unsafe {
    //            out_ptr.write(output);
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: CheckedTransmute<Target: Sized>> OutPtrWrite for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<Transmuted>>,
    //    Box<<R as CheckedTransmute>::Target>: OutPtrWrite,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let transmuted = transmute_into_target_boxed_slice(self);

    //        unsafe {
    //            OutPtrWrite::write_out(transmuted, out_ptr);
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: Encode, S: Cloned> OutPtrWrite for Box<R>
    //where
    //    Self: ReprFamily<Kind = S>,
    //{
    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
    //        unimplemented!()
    //        //let mut store = Default::default();
    //        //let _ = self.encode(&mut store);

    //        //let output = CBoxedSlice::from_boxed_slice(store.ctypes);

    //        //unsafe {
    //        //    out_ptr.write(output);
    //        //}
    //    }
    //}

    //#[cfg(feature = "alloc")]
    //impl<R, S> OutPtrWrite for Vec<R>
    //where
    //    Self: ReprFamily<Kind = Vec<S>>,
    //    Box<[R]>: OutPtrWrite,
    //{
    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
    //        unimplemented!()
    //    }
    //}

    //impl<R: Encode, S: Cloned, const N: usize> OutPtrWrite for [R; N]
    //where
    //    Self: ReprFamily<Kind = [S; N]> + Encode,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        assert_arr_has_non_zero_len::<N>();

    //        let mut store = Default::default();
    //        let item = self.encode(&mut store);

    //        unsafe {
    //            out_ptr.write(item);
    //        }
    //    }
    //}

    //impl<R: OutPtrWrite> OutPtrWrite for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithoutNiche>>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        match self {
    //            None => unsafe { out_ptr.write(COption::None()) },
    //            Some(value) => unsafe {
    //                let mut value_out_ptr = core::mem::MaybeUninit::uninit();
    //                OutPtrWrite::write_out(value, value_out_ptr.as_mut_ptr());
    //                let value_out_ptr = value_out_ptr.assume_init();

    //                out_ptr.write(COption::Some(value_out_ptr));
    //            },
    //        }
    //    }
    //}
    //impl<R: Niche + OutPtrWrite<OutPtr = <R as ExternC>::CType>> OutPtrWrite for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        self.map_or_else(
    //            || unsafe { out_ptr.write(R::NICHE_VALUE) },
    //            |v| unsafe { OutPtrWrite::write_out(v, out_ptr) },
    //        );
    //    }
    //}
}

//#[cfg(test)]
//mod tests {
//    #[cfg(feature = "alloc")]
//    use static_assertions::assert_impl_all;
//
//    use super::*;
//
//    #[test]
//    #[cfg(feature = "alloc")]
//    fn non_local_types() {
//        //#[cfg(feature = "owned-as-ref")]
//        //{
//        //    // FIXME:
//        //    //assert_not_impl_any!(Vec<u8>: OutPtrWrite);
//        //    assert_not_impl_any!(Option<Vec<u8>>: OutPtrWrite);
//        //    assert_not_impl_any!(&Vec<u8>: OutPtrWrite);
//        //    assert_not_impl_any!(&Option<Vec<u8>>: OutPtrWrite);
//        //}
//
//        {
//            assert_impl_all!(Vec<u8>: OutPtrWrite);
//            assert_impl_all!(Option<Vec<u8>>: OutPtrWrite);
//            assert_impl_all!(&Vec<u8>: OutPtrWrite);
//            assert_impl_all!(&Option<Vec<u8>>: OutPtrWrite);
//        }
//    }
//
//    // TODO:
//    //#[test]
//    //pub fn nested_owned() {
//    //    unimplemented!()
//    //}
//}
