use super::*;
#[cfg(feature = "owned_as_ref")]
use crate::transmute::{transmute_from_target_boxed_slice, transmute_from_target_vec};
use crate::{COption, slice::CBoxedSlice};
use crate::{
    ir::Transmuted,
    transmute::{transmute_from_target_ref_slice, transmute_from_target_slice_mut},
};

disjoint_impls! {
    /// Marker trait indicating that [`Encode::encode`] doesn't return a reference to the store.
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
    /// Type must not make use of the store during conversion into [`ExternC::CType`] via [`Encode::encode`]
    pub unsafe trait NonLocal {}

    unsafe impl<R> NonLocal for R where R: Encode<Store = ()> {}
    unsafe impl<R, Z: Zst> NonLocal for R where R: Encode<Store = Box<[Z]>> {}
    unsafe impl<R, Z: Zst> NonLocal for R where R: Encode<Store = Vec<Z>> {}
    unsafe impl<R, Z: Zst> NonLocal for R where R: Encode<Store = Option<Z>> {}
    unsafe impl<R, Z: Zst, const N: usize> NonLocal for R where R: Encode<Store = [Z; N]> {}
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

    #[cfg(feature = "owned_as_ref")]
    impl<R: CheckedTransmute<Target: ReprC>> OutPtr for R
    where
        Self: ReprFamily<Kind = Box<Robust>>,
    {
        type OutPtr = R::Target;
    }
    impl<R: CheckedTransmute> OutPtr for R
    where
        Self: ReprFamily<Kind = Transmuted>,
        <R as CheckedTransmute>::Target: OutPtr,
    {
        type OutPtr = <R::Target as OutPtr>::OutPtr;
    }
    impl<R: ReprC> OutPtr for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type OutPtr = Self::CType;
    }
    impl<R> OutPtr for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        type OutPtr = Self::CType;
    }

    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> OutPtr for &'a R
    where
        Self: ReprFamily<Kind = &'a S>,
    {
        type OutPtr = R::CType;
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: ExternC, S: Cloned> OutPtr for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        type OutPtr = R::CType;
    }

    impl<'slice, R: CheckedTransmute> OutPtr for &'slice [R]
    where
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
        &'slice [<R as CheckedTransmute>::Target]: OutPtr,
    {
        type OutPtr = <&'slice [R::Target] as OutPtr>::OutPtr;
    }
    impl<'a, R: ReprC> OutPtr for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Robust]>,
    {
        type OutPtr = Self::CType;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R> OutPtr for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Opaque]>,
    {
        type OutPtr = CBoxedSlice<*const R>;
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R: ExternC, S: Cloned> OutPtr for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [S]>,
    {
        type OutPtr = CBoxedSlice<R::CType>;
    }

    impl<'slice, R: CheckedTransmute> OutPtr for &'slice mut [R]
    where
        &'slice mut [<R as CheckedTransmute>::Target]: OutPtr,
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        type OutPtr = <&'slice mut [R::Target] as OutPtr>::OutPtr;
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> OutPtr for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
        Box<[<R as CheckedTransmute>::Target]>: OutPtr,
    {
        type OutPtr = <Box<[R::Target]> as OutPtr>::OutPtr;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprC> OutPtr for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        type OutPtr = CBoxedSlice<R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R> OutPtr for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        type OutPtr = CBoxedSlice<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ExternC, S: Cloned> OutPtr for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        type OutPtr = CBoxedSlice<R::CType>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Transmuted>>,
        Vec<<R as CheckedTransmute>::Target>: OutPtr,
    {
        type OutPtr = <Vec<R::Target> as OutPtr>::OutPtr;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprC> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        type OutPtr = CBoxedSlice<R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        type OutPtr = CBoxedSlice<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    impl<R: ExternC, S: Cloned> OutPtr for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        type OutPtr = CBoxedSlice<R::CType>;
    }

    impl<R, const N: usize> OutPtr for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        type OutPtr = Self::CType;
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

    #[cfg(feature = "owned_as_ref")]
    impl<R: CheckedTransmute<Target: ReprC>> OutPtrWrite for R
    where
        Self: ReprFamily<Kind = Box<Robust>>,
    {
        unsafe fn write_out(self, _out_ptr: *mut Self::OutPtr) {
            unimplemented!()
            //let mut store = Default::default();
            //let _ = self.encode(&mut store);

            //unsafe { _out_ptr.write(store.unwrap()); }
        }
    }
    impl<R: CheckedTransmute> OutPtrWrite for R
    where
        Self: ReprFamily<Kind = Transmuted>,
        <R as CheckedTransmute>::Target: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target(self);
            unsafe { OutPtrWrite::write_out(transmuted, out_ptr) }
        }
    }
    impl<R: ReprC> OutPtrWrite for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());

            unsafe {
                out_ptr.write(encoded);
            }
        }
    }
    impl<R> OutPtrWrite for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());

            unsafe {
                out_ptr.write(encoded);
            }
        }
    }

    #[cfg(feature = "cloned_refs")]
    impl<'itm, R: Encode + NonLocal + Clone, S: Cloned> OutPtrWrite for &'itm R
    where
        Self: ReprFamily<Kind = &'itm S>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);
            let output = store.encoded.unwrap();

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: Encode + NonLocal, S: Cloned> OutPtrWrite for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);
            let output = store.encoded.unwrap();

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    impl<'slice, R: CheckedTransmute> OutPtrWrite for &'slice [R]
    where
        &'slice [<R as CheckedTransmute>::Target]: OutPtrWrite,
        Self: ReprFamily<Kind = &'slice [Transmuted]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_ref_slice(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    impl<'a, R: ReprC> OutPtrWrite for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());

            unsafe {
                out_ptr.write(encoded);
            }
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'a, R> OutPtrWrite for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Opaque]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = CBoxedSlice::from_boxed_slice(store.0);

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "cloned_refs")]
    impl<'itm, R: Encode + NonLocal + Clone, S: Cloned> OutPtrWrite for &'itm [R]
    where
        Self: ReprFamily<Kind = &'itm [S]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = CBoxedSlice::from_boxed_slice(store.ctypes);

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    impl<'slice, R: CheckedTransmute> OutPtrWrite for &'slice mut [R]
    where
        &'slice mut [<R as CheckedTransmute>::Target]: OutPtrWrite,
        Self: ReprFamily<Kind = &'slice mut [Transmuted]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_slice_mut(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> OutPtrWrite for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
        Box<[<R as CheckedTransmute>::Target]>: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_boxed_slice(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprC> OutPtrWrite for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            #[cfg(feature = "owned_as_ref")]
            let output = {
                let mut store = Default::default();
                let _ = self.encode(&mut store);

                CBoxedSlice::from_boxed_slice(store.0)
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let output = self.encode(&mut ());

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R> OutPtrWrite for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Opaque]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            #[cfg(feature = "owned_as_ref")]
            let output = {
                let mut store = Default::default();
                let _ = self.encode(&mut store);

                CBoxedSlice::from_boxed_slice(store.0)
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let output = self.encode(&mut ());

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R: Encode + NonLocal, S: Cloned> OutPtrWrite for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = CBoxedSlice::from_boxed_slice(store.ctypes);

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: CheckedTransmute> OutPtrWrite for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Transmuted>>,
        Vec<<R as CheckedTransmute>::Target>: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_vec(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R: ReprC> OutPtrWrite for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            #[cfg(feature = "owned_as_ref")]
            let output = {
                let mut store = Default::default();
                let _ = self.encode(&mut store);

                CBoxedSlice::from_boxed_slice(store.0)
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let output = self.encode(&mut ());

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R> OutPtrWrite for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            #[cfg(feature = "owned_as_ref")]
            let output = {
                let mut store = Default::default();
                let _ = self.encode(&mut store);

                CBoxedSlice::from_boxed_slice(store.0)
            };
            #[cfg(not(feature = "owned_as_ref"))]
            let output = self.encode(&mut ());

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    impl<R: Encode + NonLocal, S: Cloned> OutPtrWrite for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = CBoxedSlice::from_boxed_slice(store.ctypes);

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    impl<R, const N: usize> OutPtrWrite for [R; N]
    where
        Self: ReprFamily<Kind = [Opaque; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();
            let encoded = self.encode(&mut ());
            unsafe {
                out_ptr.write(encoded);
            }
        }
    }
    impl<R: Encode + NonLocal, S: Cloned, const N: usize> OutPtrWrite for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]> + Encode,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();

            let mut store = Default::default();
            let item = self.encode(&mut store);

            unsafe {
                out_ptr.write(item);
            }
        }
    }

    impl<R: OutPtrWrite> OutPtrWrite for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            match self {
                None => unsafe { out_ptr.write(COption::None()) },
                Some(value) => unsafe {
                    let mut value_out_ptr = core::mem::MaybeUninit::uninit();
                    OutPtrWrite::write_out(value, value_out_ptr.as_mut_ptr());
                    let value_out_ptr = value_out_ptr.assume_init();

                    out_ptr.write(COption::Some(value_out_ptr));
                },
            }
        }
    }
    impl<R: Niche + OutPtrWrite<OutPtr = <R as ExternC>::CType>> OutPtrWrite for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            self.map_or_else(
                || unsafe { out_ptr.write(R::NICHE_VALUE) },
                |v| unsafe { OutPtrWrite::write_out(v, out_ptr) },
            );
        }
    }
}

disjoint_impls! {
    /// Facilitates reading from [`Self::OutPtr`] out-pointer.
    pub trait OutPtrRead: OutPtr + Sized {
        /// Read a rust value from the corresponding out-pointer
        ///
        /// # Errors
        ///
        /// Check [`Decode::decode`]
        ///
        /// # Safety
        ///
        /// Check [`Decode::decode`]
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self>;
    }

    impl<R: CheckedTransmute> OutPtrRead for R
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
    impl<R: ReprC> OutPtrRead for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe { Decode::decode(out_ptr, &mut ()) }
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: CheckedTransmute<Target: ReprC>> OutPtrRead for R
    where
        Self: ReprFamily<Kind = Box<Robust>>,
    {
        unsafe fn try_read_out(_out_ptr: Self::OutPtr) -> Option<Self> {
            unimplemented!()
            //Ok(Box::new(_out_ptr))
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Decode<'d> + NonLocal + 'd, S: Cloned> OutPtrRead for Box<R>
    where
        Self: ReprFamily<Kind = Box<S>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            let mut store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut R::Store, &'d mut R::Store>(&mut store)
            };

            unsafe { Decode::decode(out_ptr, store_ref).map(Box::new) }
        }
    }

    impl<'d, R: CheckedTransmute> OutPtrRead for &'d [R]
    where
        &'d [<R as CheckedTransmute>::Target]: OutPtrRead,
        Self: ReprFamily<Kind = &'d [Transmuted]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe { <&[R::Target]>::try_read_out(out_ptr) }
                .and_then(|output| transmute_from_target_ref_slice(output))
        }
    }
    impl<'a, R: ReprC> OutPtrRead for &'a [R]
    where
        Self: ReprFamily<Kind = &'a [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe { out_ptr.into_rust() }
        }
    }

    impl<'d, R: CheckedTransmute> OutPtrRead for &'d mut [R]
    where
        &'d mut [<R as CheckedTransmute>::Target]: OutPtrRead,
        Self: ReprFamily<Kind = &'d mut [Transmuted]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe { <&mut [R::Target]>::try_read_out(out_ptr) }
                .and_then(|output| transmute_from_target_slice_mut(output))
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: CheckedTransmute> OutPtrRead for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Transmuted]>>,
        Box<[<R as CheckedTransmute>::Target]>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe {
                <Box<[R::Target]>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrRead for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[Robust]>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe {
                let res = Decode::decode(out_ptr.into(), &mut ());

                if !out_ptr.deallocate() {
                    return None;
                }

                res
            }
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ExternC + NonLocal + 'd, S: Cloned> OutPtrRead for Box<[R]>
    where
        Self: ReprFamily<Kind = Box<[S]>> + Decode<'d, CType = CSliceMut<<R as ExternC>::CType>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            let mut store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut <Self as Decode>::Store, &'d mut <Self as Decode>::Store>(
                    &mut store,
                )
            };

            unsafe {
                let res = Decode::decode(out_ptr.into(), store_ref);

                if !out_ptr.deallocate() {
                    return None;
                }

                res
            }
        }
    }

    #[cfg(feature = "owned_as_ref")]
    impl<R: CheckedTransmute> OutPtrRead for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Transmuted>>,
        Vec<<R as CheckedTransmute>::Target>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe {
                <Vec<R::Target>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrRead for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            unsafe {
                let res = Decode::decode(out_ptr.into(), &mut ());

                if !out_ptr.deallocate() {
                    return None;
                }

                res
            }
        }
    }
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: ExternC + NonLocal + 'd, S: Cloned> OutPtrRead for Vec<R>
    where
        Self: ReprFamily<Kind = Vec<S>> + Decode<'d, CType = CSliceMut<<R as ExternC>::CType>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            let mut store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut <Self as Decode>::Store, &'d mut <Self as Decode>::Store>(
                    &mut store,
                )
            };

            unsafe {
                let res = Decode::decode(out_ptr.into(), store_ref);

                if !out_ptr.deallocate() {
                    return None;
                }

                res
            }
        }
    }

    impl<'d, R: ExternC + NonLocal + 'd, S: Cloned, const N: usize> OutPtrRead for [R; N]
    where
        Self: ReprFamily<Kind = [S; N]> + Decode<'d>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            assert_arr_has_non_zero_len::<N>();
            let mut store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut <Self as Decode>::Store, &'d mut <Self as Decode>::Store>(
                    &mut store,
                )
            };

            unsafe { Decode::decode(out_ptr, store_ref) }
        }
    }

    impl<R: OutPtrRead> OutPtrRead for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithoutNiche>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
            let option = TryInto::<Option<_>>::try_into(out_ptr).ok()?;
            match option {
                Some(payload) => unsafe { R::try_read_out(payload) }.map(Some),
                None => Some(None),
            }
        }
    }
    impl<R: Niche + OutPtrRead> OutPtrRead for Option<R>
    where
        Self: ReprFamily<Kind = Option<WithCustomNiche>>,
        //<R as ExternC>::CType: PartialEq,
    {
        unsafe fn try_read_out(_out_ptr: Self::OutPtr) -> Option<Self> {
            unimplemented!()
            //if _out_ptr == R::NICHE_VALUE {
            //    return Ok(None);
            //}

            //unsafe { R::try_read_out(_out_ptr).map(Some) }
        }
    }
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_impl_all;
    #[cfg(feature = "owned_as_ref")]
    use static_assertions::assert_not_impl_any;

    use super::*;

    #[test]
    fn non_local_types() {
        #[cfg(feature = "owned_as_ref")]
        {
            // FIXME:
            //assert_not_impl_any!(Vec<u8>: OutPtrWrite);
            assert_not_impl_any!(Option<Vec<u8>>: OutPtrWrite);
            assert_not_impl_any!(&Vec<u8>: OutPtrWrite);
            assert_not_impl_any!(&Option<Vec<u8>>: OutPtrWrite);
        }

        #[cfg(not(feature = "owned_as_ref"))]
        {
            assert_impl_all!(Vec<u8>: OutPtrWrite);
            assert_impl_all!(Option<Vec<u8>>: OutPtrWrite);
            assert_impl_all!(&Vec<u8>: OutPtrWrite);
            assert_impl_all!(&Option<Vec<u8>>: OutPtrWrite);
        }
    }
}
