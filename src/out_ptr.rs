#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

use super::*;
#[cfg(feature = "alloc")]
use crate::boxed::CBoxedSlice;
use crate::{ir::Transmuted, option::COption};

disjoint_impls! {
    /// Marker trait indicating that [`EncodeWithStore::encode`] doesn't return a reference to the store.
    /// This is useful to determine which(and how) types can be returned from an FFI function
    /// considering that, after return, local context is destroyed
    ///
    /// # Example
    ///
    /// 1. `&[u8]` implements [`NonLocal`]
    ///
    /// This type will be converted to [`CSlice<u8>`] and during conversion will not make use
    /// of the store (in any direction). The corresponding out-pointer will be `*mut CSlice<u8>`
    ///
    /// 2. `&[Opaque<T>]` doesn't implement [`NonLocal`]
    ///
    /// This type will be converted to [`CSlice<*const T>`] and during conversion will use the
    /// local store `Vec<*const T>`. The corresponding out-pointer will be `*mut CBoxedSlice<*const T>`.
    ///
    /// 3. `&(u32, u32)`
    ///
    /// This type will be converted to `*const CTuple2<u32, u32>` and during conversion will use the
    /// local store `CTuple<u32, u32>`. The corresponding out-pointer will be `*mut CTuple2<u32, u32>`
    ///
    /// # Safety
    ///
    /// Type must not make use of the store during conversion into [`ExternC::CType`] via [`EncodeWithStore::encode`]
    pub unsafe trait NonLocal {}

    unsafe impl<R> NonLocal for R where R: EncodeWithStore<Store = ()> {}
    #[cfg(feature = "alloc")]
    unsafe impl<R, Z: Zst> NonLocal for R where R: EncodeWithStore<Store = Box<[Z]>> {}
    #[cfg(feature = "alloc")]
    unsafe impl<R, Z: Zst> NonLocal for R where R: EncodeWithStore<Store = Vec<Z>> {}
    unsafe impl<R, Z: Zst> NonLocal for R where R: EncodeWithStore<Store = Option<Z>> {}
    unsafe impl<R, Z: Zst, const N: usize> NonLocal for R where R: EncodeWithStore<Store = [Z; N]> {}
    // TODO: It's not possbile to implement for specific len yet: https://github.com/mversic/co3/issues/13
    //unsafe impl<R, T> NonLocal for R where R: Encode<Store = [T; 0]> {}
}

/// Marker for a ZST(zero-sized type)
///
/// # Safety
///
/// Type must be a ZST
///
/// This is because implementations of [`NonLocal`](which is an unsafe trait) depend on it
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

    impl<'a, R: ?Sized + Dst<Elem: ReprC>> OutPtr for &'a R
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
    {
        type OutPtr = <&'a R::Target as OutPtr>::OutPtr;
    }
    #[cfg(feature = "unstable-refs")]
    impl<'a, R: ExternC, S: Cloned> OutPtr for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
        R: SizeFamily<Kind = Sized_>,
    {
        type OutPtr = R::CType;
    }
    //#[cfg(feature = "unstable-refs")]
    //impl<'a, R: ?Sized, S: Cloned> OutPtr for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a S>,
    //    R: SizeFamily<Kind = UnSized>,
    //{
    //    type OutPtr = Self::CType;
    //}

    impl<'a, R: ?Sized + Dst<Elem: ReprC>> OutPtr for &'a mut R
    where
        Self: ReprFamily<Kind = &'a mut Robust>,
    {
        type OutPtr = Self::CType;
    }
    impl<'a, R: ?Sized + CheckedTransmute> OutPtr for &'a mut R
    where
        &'a mut <R as CheckedTransmute>::Target: OutPtr,
        Self: ReprFamily<Kind = &'a mut Transmuted>,
    {
        type OutPtr = <&'a mut R::Target as OutPtr>::OutPtr;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized + crate::Dst<Elem: ReprC>> OutPtr for Box<R>
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
    {
        type OutPtr = <Box<R::Target> as OutPtr>::OutPtr;
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
        R: SizeFamily<Kind = Sized_>,
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
    impl<R: ReprC> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        type OutPtr = CBoxedSlice<R>;
    }
    #[cfg(feature = "alloc")]
    impl<R: CheckedTransmute<Target: Sized>> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Transmuted>>,
        Vec<<R as CheckedTransmute>::Target>: OutPtr,
    {
        type OutPtr = <Vec<R::Target> as OutPtr>::OutPtr;
    }
    #[cfg(feature = "alloc")]
    impl<R: ExternC, S: Cloned> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        type OutPtr = CBoxedSlice<R::CType>;
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

    //#[cfg(feature = "unstable-refs")]
    //impl<'itm, R: Encode + NonLocal + Clone, S: Cloned> OutPtrWrite for &'itm R
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
    //impl<R: Encode + NonLocal, S: Cloned> OutPtrWrite for Box<R>
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
    //impl<R: Encode + NonLocal + Clone, S: Cloned> OutPtrWrite for &R
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
    //impl<R: Encode + NonLocal, S: Cloned> OutPtrWrite for Box<R>
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
    //impl<R: ReprC> OutPtrWrite for Vec<R>
    //where
    //    Self: ReprFamily<Kind = Vec<Robust>>,
    //{
    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
    //        unimplemented!()
    //        //let output = self.encode(&mut ());

    //        //unsafe {
    //        //    out_ptr.write(output);
    //        //}
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: CheckedTransmute<Target: Sized>> OutPtrWrite for Vec<R>
    //where
    //    Vec<<R as CheckedTransmute>::Target>: OutPtrWrite,
    //    Self: ReprFamily<Kind = Vec<Transmuted>>,
    //{
    //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
    //        let transmuted = transmute_into_target_vec(self);

    //        unsafe {
    //            OutPtrWrite::write_out(transmuted, out_ptr);
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: Encode + NonLocal, S: Cloned> OutPtrWrite for Vec<R>
    //where
    //    Self: ReprFamily<Kind = Vec<S>>,
    //{
    //    unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
    //        unimplemented!()
    //        //let mut store = Default::default();
    //        //let _ = self.encode(&mut store);

    //        //let output = CBoxedSlice::from_boxed_slice(store.ctypes);

    //        //unsafe { out_ptr.write(output); }
    //    }
    //}

    //impl<R: Encode + NonLocal, S: Cloned, const N: usize> OutPtrWrite for [R; N]
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

disjoint_impls! {
    /// Facilitates reading from [`Self::OutPtr`] out-pointer.
    pub trait OutPtrRead: OutPtr {
        /// Read a rust value from the corresponding out-pointer
        ///
        /// # Errors
        ///
        /// Check [`DecodeWithStore::decode`]
        ///
        /// # Safety
        ///
        /// Check [`DecodeWithStore::decode`]
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self>;
    }

    impl<R: ReprC> OutPtrRead for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe { <Self as DecodeWithStore>::decode(out_ptr, &mut ()) }
        }
    }
    #[cfg(feature = "alloc")]
    impl<'d, R: 'd> OutPtrRead for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe { <Self as DecodeWithStore>::decode(out_ptr, &mut ()) }
        }
    }
    impl<R: CheckedTransmute<Target: Sized>> OutPtrRead for R
    where
        Self: ReprFamily<Kind = Transmuted>,
        <R as CheckedTransmute>::Target: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe {
                OutPtrRead::try_read_out(out_ptr).and_then(|output| transmute_from_target(output))
            }
        }
    }

    //#[cfg(feature = "alloc")]
    //impl<'d, R: Decode + NonLocal + 'd, S: Cloned> OutPtrRead for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<S>>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        let mut store = Default::default();

    //        let store_ref = unsafe {
    //            core::mem::transmute::<&mut R::Store, &'d mut R::Store>(&mut store)
    //        };

    //        unsafe { DecodeWithStore::decode(out_ptr, store_ref).map(Box::new) }
    //    }
    //}

    //impl<'a, R: ?Sized + Dst<Elem: ReprC>> OutPtrRead for &'a R
    //where
    //    Self: ReprFamily<Kind = &'a [Robust]>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unsafe { out_ptr.into_rust() }
    //    }
    //}
    //impl<'d, R: CheckedTransmute<Target: Sized + 'd>> OutPtrRead for &'d [R]
    //where
    //    &'d [<R as CheckedTransmute>::Target]: OutPtrRead,
    //    Self: ReprFamily<Kind = [Transmuted]>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unsafe { <&[R::Target]>::try_read_out(out_ptr) }
    //            .and_then(|output| transmute_from_target_ref_slice(output))
    //    }
    //}

    //impl<'d, R: CheckedTransmute<Target: Sized + 'd>> OutPtrRead for &'d mut [R]
    //where
    //    &'d mut [<R as CheckedTransmute>::Target]: OutPtrRead,
    //    Self: ReprFamily<Kind = [Transmuted]>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unsafe { <&mut [R::Target]>::try_read_out(out_ptr) }
    //            .and_then(|output| transmute_from_target_slice_mut(output))
    //    }
    //}

    //#[cfg(feature = "alloc")]
    //impl<R: ?Sized + Dst<Eleme: ReprC>> OutPtrRead for Box<R>
    //where
    //    Self: ReprFamily<Kind = Box<[Robust]>>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unsafe {
    //            let res = DecodeWithStore::decode(out_ptr, &mut ());

    //            if !out_ptr.deallocate() {
    //                return None;
    //            }

    //            res
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: CheckedTransmute<Target: Sized>> OutPtrRead for Box<[R]>
    //where
    //    Self: ReprFamily<Kind = [Transmuted]>,
    //    Box<[<R as CheckedTransmute>::Target]>: OutPtrRead,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unsafe {
    //            <Box<[R::Target]>>::try_read_out(out_ptr)
    //                .and_then(|output| transmute_from_target_boxed_slice(output))
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<'d, R: ExternC + NonLocal + 'd, S: Cloned> OutPtrRead for Box<[R]>
    //where
    //    Self: ReprFamily<Kind = [S]> + Decode<'d, CType = CSliceMut<<R as ExternC>::CType>>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unimplemented!()
    //        //let mut store = Default::default();

    //        //let store_ref = unsafe {
    //        //    core::mem::transmute::<&mut <Self as Decode>::Store, &'d mut <Self as Decode>::Store>(
    //        //        &mut store,
    //        //    )
    //        //};

    //        //unsafe {
    //        //    let res = DecodeWithStore::decode(out_ptr.into(), store_ref);

    //        //    if !out_ptr.deallocate() {
    //        //        return None;
    //        //    }

    //        //    res
    //        //}
    //    }
    //}

    //#[cfg(feature = "alloc")]
    //impl<R: ReprC> OutPtrRead for Vec<R>
    //where
    //    Self: ReprFamily<Kind = Vec<Robust>>,
    //{
    //    unsafe fn try_read_out(_out_ptr: Self::OutPtr) -> Option<Self> {
    //        unimplemented!()
    //        //unsafe {
    //        //    let res = DecodeWithStore::decode(out_ptr.into(), &mut ());

    //        //    if !out_ptr.deallocate() {
    //        //        return None;
    //        //    }

    //        //    res
    //        //}
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<R: CheckedTransmute<Target: Sized>> OutPtrRead for Vec<R>
    //where
    //    Self: ReprFamily<Kind = [Transmuted]>,
    //    Vec<<R as CheckedTransmute>::Target>: OutPtrRead,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unsafe {
    //            <Vec<R::Target>>::try_read_out(out_ptr)
    //                .and_then(|output| transmute_from_target_vec(output))
    //        }
    //    }
    //}
    //#[cfg(feature = "alloc")]
    //impl<'d, R: ExternC + NonLocal + 'd, S: Cloned> OutPtrRead for Vec<R>
    //where
    //    Self: ReprFamily<Kind = [S]> + Decode<'d, CType = CSliceMut<<R as ExternC>::CType>>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        unimplemented!()
    //        //let mut store = Default::default();

    //        //let store_ref = unsafe {
    //        //    core::mem::transmute::<&mut <Self as Decode>::Store, &'d mut <Self as Decode>::Store>(
    //        //        &mut store,
    //        //    )
    //        //};

    //        //unsafe {
    //        //    let res = DecodeWithStore::decode(out_ptr.into(), store_ref);

    //        //    if !out_ptr.deallocate() {
    //        //        return None;
    //        //    }

    //        //    res
    //        //}
    //    }
    //}

    //impl<'d, R: ExternC + NonLocal + 'd, S: Cloned, const N: usize> OutPtrRead for [R; N]
    //where
    //    Self: ReprFamily<Kind = [S]> + Decode,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        assert_arr_has_non_zero_len::<N>();
    //        let mut store = Default::default();

    //        let store_ref = unsafe {
    //            core::mem::transmute::<&mut <Self as Decode>::Store, &'d mut <Self as Decode>::Store>(
    //                &mut store,
    //            )
    //        };

    //        unsafe { DecodeWithStore::decode(out_ptr, store_ref) }
    //    }
    //}

    //impl<R: OutPtrRead> OutPtrRead for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithoutNiche>>,
    //{
    //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
    //        let option = TryInto::<Option<_>>::try_into(out_ptr).ok()?;
    //        match option {
    //            Some(payload) => unsafe { R::try_read_out(payload) }.map(Some),
    //            None => Some(None),
    //        }
    //    }
    //}
    //impl<R: Niche + OutPtrRead> OutPtrRead for Option<R>
    //where
    //    Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    //    //<R as ExternC>::CType: PartialEq,
    //{
    //    unsafe fn try_read_out(_out_ptr: Self::OutPtr) -> Option<Self> {
    //        unimplemented!()
    //        //if _out_ptr == R::NICHE_VALUE {
    //        //    return Ok(None);
    //        //}

    //        //unsafe { R::try_read_out(_out_ptr).map(Some) }
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
