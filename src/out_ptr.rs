use core::ptr::addr_of_mut;

use super::*;
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
use crate::transmute::{transmute_from_target_boxed_slice, transmute_from_target_vec};
use crate::{
    ir::Transparent,
    transmute::{transmute_from_target_ref_slice, transmute_from_target_slice_mut},
};

disjoint_impls! {
    /// Marker trait indicating that [`Encode::encode`] and [`Decode::decode`] don't
    /// return a reference to the store. This is useful to determine which(and how) types can be
    /// returned from an FFI function considering that, after return, local context is destroyed
    ///
    /// # Example
    ///
    /// 1. `&[u8]` implements [`NonLocal`]
    ///
    /// This type will be converted to [`RefSlice<u8>`] and during conversion will not make use
    /// of the store (in any direction). The corresponding out-pointer will be `*mut RefSlice<u8>`
    ///
    /// 2. `&[Opaque<T>]` doesn't implement [`NonLocal`]
    ///
    /// This type will be converted to [`RefSlice<*const T>`] and during conversion will use the
    /// local store `Vec<*const T>`. The corresponding out-pointer will be `*mut OutBoxedSlice<*const T>`.
    ///
    /// 3. `&(u32, u32)`
    ///
    /// This type will be converted to `*const FfiTuple2<u32, u32>` and during conversion will use the
    /// local store `FfiTuple<u32, u32>`. The corresponding out-pointer will be `*mut FfiTuple2<u32, u32>`
    ///
    /// # Safety
    ///
    /// Type must not make use of the store during conversion into [`ExternC::CType`] via [`Encode::encode`] or [`Decode::decode`]
    pub unsafe trait NonLocal: OutPtr + Clone {}

    unsafe impl<'d, R: Clone> NonLocal for R where R: Decode<'d, Store = ()> + OutPtr {}
    unsafe impl<'d, R: Clone> NonLocal for R where R: Decode<'d, Store = Box<[()]>> + OutPtr {}
    unsafe impl<'d, R: Clone, const N: usize> NonLocal for R where R: Decode<'d, Store = [(); N]> + OutPtr {}
}

disjoint_impls! {
    /// Facilitates the use of [`Self`] as out-pointer.
    ///
    /// If a type implements [`Ir`], i.e. has a defined internal representation,
    /// a blanket implementation is provided.
    pub trait OutPtr: ExternC {
        /// Type of the out-pointer
        type OutPtr: ReprC;
    }

    impl<R: Transmute> OutPtr for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: OutPtr,
    {
        // TODO: Why is it not Self::CType here?
        type OutPtr = <R::Target as OutPtr>::OutPtr;
    }
    impl<R: ReprC> OutPtr for R
    where
        Self: Ir<Type = Robust>,
    {
        type OutPtr = Self::CType;
    }
    impl<R> OutPtr for R
    where
        Self: Ir<Type = Opaque>,
    {
        type OutPtr = Self::CType;
    }
    impl<R: Ir<Type = Extern> + External> OutPtr for R {
        type OutPtr = Self::CType;
    }

    impl<'a, R: Ir<Type = Extern> + External> OutPtr for &'a R
    where
        Self: Ir<Type = &'a Extern>,
    {
        type OutPtr = Self::CType;
    }
    impl<'a, R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type OutPtr = R::CType;
    }

    impl<'a, R: Ir<Type = Extern> + External> OutPtr for &'a mut R
    where
        Self: Ir<Type = &'a mut Extern>,
    {
        type OutPtr = Self::CType;
    }

    impl<'slice, R: Transmute> OutPtr for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: OutPtr,
    {
        type OutPtr = <&'slice [R::Target] as OutPtr>::OutPtr;
    }
    impl<'a, R: ReprC> OutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type OutPtr = Self::CType;
    }
    impl<'a, R> OutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        type OutPtr = OutBoxedSlice<*const R>;
    }
    impl<'a, R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [S]>,
    {
        type OutPtr = OutBoxedSlice<R::CType>;
    }

    impl<'slice, R: Transmute> OutPtr for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: OutPtr,
    {
        type OutPtr = <&'slice mut [R::Target] as OutPtr>::OutPtr;
    }
    impl<'a, R: ReprC> OutPtr for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type OutPtr = Self::CType;
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type OutPtr = R;
    }
    impl<R: External> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type OutPtr = Self::CType;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type OutPtr = R::CType;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: OutPtr,
    {
        type OutPtr = <Box<[R::Target]> as OutPtr>::OutPtr;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type OutPtr = OutBoxedSlice<R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type OutPtr = OutBoxedSlice<<R>::CType>;
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: OutPtr,
    {
        type OutPtr = <Vec<R::Target> as OutPtr>::OutPtr;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type OutPtr = OutBoxedSlice<R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type OutPtr = OutBoxedSlice<R::CType>;
    }

    impl<R, const N: usize> OutPtr for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type OutPtr = Self::CType;
    }
    impl<R, const N: usize> OutPtr for [R; N]
    where
        Self: Ir<Type = [Extern; N]>,
    {
        type OutPtr = Self::CType;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned, const N: usize> OutPtr for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type OutPtr = Self::CType;
    }

    impl<R: Optional> OutPtr for R
    where
        Self: Ir<Type = Option<Transparent>>,
    {
        type OutPtr = R::Inner;
    }
    impl<R: OutPtr> OutPtr for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        type OutPtr = FfiTuple2<<u8 as OutPtr>::OutPtr, R::OutPtr>;
    }
    impl<R: OutPtr> OutPtr for Option<R>
    where
        Self: Ir<Type = Option<Opaque>>,
    {
        type OutPtr = *mut R;
    }
    impl<R: Niche + OutPtr, S: Cloned> OutPtr for Option<R>
    where
        Self: Ir<Type = Option<S>>,
    {
        type OutPtr = R::OutPtr;
    }

    impl<'itm, R, S: Cloned> OutPtr for LocalRef<'itm, R>
    where
        &'itm R: Ir<Type = &'itm S> + OutPtr,
        Self: Ir<Type = &'itm S>,
    {
        type OutPtr = <&'itm R as OutPtr>::OutPtr;
    }
    impl<'itm, R, S: Cloned> OutPtr for LocalSlice<'itm, R>
    where
        &'itm [R]: Ir<Type = &'itm [S]> + OutPtr,
        Self: Ir<Type = &'itm [S]>,
    {
        type OutPtr = <&'itm [R] as OutPtr>::OutPtr;
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

    impl<R: Transmute> OutPtrWrite for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr)
            }
        }
    }
    impl<R: ReprC> OutPtrWrite for R
    where
        Self: Ir<Type = Robust>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());
            unsafe { out_ptr.write(encoded); }
        }
    }
    impl<R> OutPtrWrite for R
    where
        Self: Ir<Type = Opaque>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());
            unsafe { out_ptr.write(encoded); }
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Encode, S: Cloned> OutPtrWrite for &'itm R
    where
        Self: Ir<Type = &'itm S>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);
            let output = store.0.unwrap();

            unsafe { out_ptr.write(output); }
        }
    }

    impl<'slice, R: Transmute> OutPtrWrite for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: OutPtrWrite,
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
        Self: Ir<Type = &'a [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());
            unsafe { out_ptr.write(encoded); }
        }
    }
    impl<'a, R: Clone> OutPtrWrite for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Encode, S: Cloned> OutPtrWrite for &'itm [R]
    where
        Self: Ir<Type = &'itm [S]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    impl<'slice, R: Transmute> OutPtrWrite for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_slice_mut(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    impl<'a, R: ReprC> OutPtrWrite for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let encoded = self.encode(&mut ());
            unsafe { out_ptr.write(encoded); }
        }
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                out_ptr.write(*self);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + NonLocal + Encode, S: Cloned> OutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let output = self.encode(&mut store);

            unsafe { out_ptr.write(output); }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> OutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_boxed_slice(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            self.encode(&mut store);

            unsafe {
                out_ptr.write(OutBoxedSlice::from_boxed_slice(Some(store)));
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> OutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            self.encode(&mut store);

            unsafe {
                out_ptr.write(OutBoxedSlice::from_boxed_slice(Some(store)));
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + NonLocal + Encode, S: Cloned> OutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _  = self.encode(&mut store);

            let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> OutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_vec(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            self.encode(&mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R> OutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            self.encode(&mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: Ir<Type = S> + NonLocal + Encode, S: Cloned> OutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            let _ = self.encode(&mut store);

            let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

            unsafe {
                out_ptr.write(output);
            }
        }
    }

    impl<R, const N: usize> OutPtrWrite for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();
            let encoded = self.encode(&mut ());
            unsafe { out_ptr.write(encoded); }
        }
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned, const N: usize> OutPtrWrite for [R; N]
    where
        Self: Ir<Type = [S; N]> + Encode,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();
            let mut store = Default::default();
            let item = self.encode(&mut store);

            unsafe { out_ptr.write(item); }
        }
    }

    impl<R: Optional> OutPtrWrite for R
    where
        Self: Ir<Type = Option<Transparent>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unimplemented!();
        }
    }
    impl<R: OutPtrWrite> OutPtrWrite for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            match self {
                None => {
                    let mut discriminant_out_ptr = core::mem::MaybeUninit::uninit();
                    unsafe {
                        OutPtrWrite::write_out(0u8, discriminant_out_ptr.as_mut_ptr());
                        let discriminant_out_ptr = discriminant_out_ptr.assume_init();

                        // TODO: No need to zero the memory because it must never be read
                        out_ptr.write(FfiTuple2(discriminant_out_ptr, core::mem::zeroed()));
                    }
                }
                Some(value) => unsafe {
                    let mut discriminant_out_ptr = core::mem::MaybeUninit::uninit();
                    OutPtrWrite::write_out(1u8, discriminant_out_ptr.as_mut_ptr());
                    let discriminant_out_ptr = discriminant_out_ptr.assume_init();

                    let mut value_out_ptr = core::mem::MaybeUninit::uninit();
                    OutPtrWrite::write_out(value, value_out_ptr.as_mut_ptr());
                    let value_out_ptr = value_out_ptr.assume_init();

                    out_ptr.write(FfiTuple2(discriminant_out_ptr, value_out_ptr));
                },
            }
        }
    }
    impl<R: OutPtrWrite> OutPtrWrite for Option<R>
    where
        Self: Ir<Type = Option<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unimplemented!()
        }
    }
    impl<R: Niche + OutPtrWrite, S: Cloned> OutPtrWrite for R
    where
        Self: Ir<Type = Option<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unimplemented!()
            //unsafe {
            //    self.map_or_else(
            //        || out_ptr.write(R::NICHE_VALUE),
            //        |value| OutPtrWrite::write_out(value, out_ptr),
            //    );
            //}
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
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self>;
    }

    impl<R: Transmute> OutPtrRead for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                OutPtrRead::try_read_out(out_ptr).and_then(|output| transmute_from_target(output))
            }
        }
    }
    impl<R: ReprC> OutPtrRead for R
    where
        Self: Ir<Type = Robust>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { Decode::decode(out_ptr, &mut ()) }
        }
    }
    impl<R: Ir<Type = Extern> + External> OutPtrRead for R {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { Decode::decode(out_ptr, &mut ()) }
        }
    }

    impl<'d, R: Ir<Type = S> + NonLocal + Decode<'d>, S: Cloned + 'd> OutPtrRead
        for LocalRef<'d, R>
    where
        Self: Ir<Type = &'d S>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            let item = unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_ref = &mut *addr_of_mut!(store);
                R::decode(out_ptr, store_ref)?
            };

            Ok(Self::new(item))
        }
    }

    impl<'d, R: Ir<Type = S> + NonLocal + Decode<'d>, S: Cloned + 'd> OutPtrRead for LocalSlice<'d, R>
    where
        Self: Ir<Type = &'d [S]>,
    {
        unsafe fn try_read_out(out_ptr: OutBoxedSlice<<R>::CType>) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_ref = &mut *addr_of_mut!(store);
                let res = <&[R]>::decode(slice, store_ref);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res?;
            }

            Ok(Self::new(store.0))
        }
    }
    impl<'d, R: Transmute> OutPtrRead for &'d [R]
    where
        Self: Ir<Type = &'d [Transparent]>,
        &'d [<R>::Target]: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <&[R::Target]>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_ref_slice(output))
            }
        }
    }
    impl<'a, R: ReprC> OutPtrRead for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { out_ptr.into_rust() }.ok_or(FfiReturn::ArgIsNull)
        }
    }

    impl<'d, R: Transmute> OutPtrRead for &'d mut [R]
    where
        Self: Ir<Type = &'d mut [Transparent]>,
        &'d mut [<R>::Target]: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <&mut [R::Target]>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_slice_mut(output))
            }
        }
    }
    impl<'a, R: ReprC> OutPtrRead for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { out_ptr.into_rust() }.ok_or(FfiReturn::ArgIsNull)
        }
    }

    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            Ok(Box::new(out_ptr))
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Ir<Type = S> + NonLocal + Decode<'d> + 'd, S: Cloned> OutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<&mut R::Store, &'d mut R::Store>(&mut store)
            };

            unsafe { Decode::decode(out_ptr, store_ref) }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> OutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <Box<[R::Target]>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

            unsafe {
                let res = Decode::decode(slice, &mut ());

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Ir<Type = S> + NonLocal + 'd, S: Cloned + 'd> OutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>> + Decode<'d, CType = RefSlice<<R as ExternC>::CType>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

            let mut store = Default::default();

            let store_ref = unsafe {
                core::mem::transmute::<
                    &mut <Self as Decode>::Store,
                    &'d mut <Self as Decode>::Store
                >(&mut store)
            };

            unsafe {
                let res = Decode::decode(slice, store_ref);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }

    #[cfg(feature = "owned_types")]
    impl<R: Transmute> OutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <Vec<R::Target>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<R: ReprC> OutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

            unsafe {
                let res = Decode::decode(slice, &mut ());

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }
    #[cfg(feature = "owned_types")]
    #[cfg(feature = "owned_as_ref")]
    impl<'d, R: Ir<Type = S> + NonLocal + 'd, S: Cloned> OutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<S>> + Decode<'d, CType = RefSlice<<R as ExternC>::CType>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());
            let mut store = <Self as Decode>::Store::default();

            let store_ref = unsafe {
                core::mem::transmute::<
                    &mut <Self as Decode>::Store,
                    &'d mut <Self as Decode>::Store
                >(&mut store)
            };

            unsafe {
                let res = Decode::decode(slice, store_ref);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }

    impl<'d, R: Ir<Type = S> + NonLocal + 'd, S: Cloned, const N: usize> OutPtrRead for [R; N]
    where
        Self: Ir<Type = [S; N]> + Decode<'d>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            assert_arr_has_non_zero_len::<N>();
            let mut store = <Self as Decode>::Store::default();

            let store_ref = unsafe {
                core::mem::transmute::<
                    &mut <Self as Decode>::Store,
                    &'d mut <Self as Decode>::Store
                >(&mut store)
            };

            unsafe { Decode::decode(out_ptr, store_ref) }
        }
    }

    impl<R: Optional> OutPtrRead for R
    where
        Self: Ir<Type = Option<Transparent>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unimplemented!()
        }
    }
    impl<R: OutPtrRead> OutPtrRead for Option<R>
    where
        Self: Ir<Type = Option<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            match unsafe { <u8 as OutPtrRead>::try_read_out(out_ptr.0)? } {
                0 => Ok(None),
                1 => Ok(Some(unsafe { R::try_read_out(out_ptr.1)? })),
                _ => Err(FfiReturn::TrapRepresentation),
            }
        }
    }
    impl<R: Niche + OutPtrRead, S: Cloned> OutPtrRead for Option<R>
    where
        Self: Ir<Type = Option<S>>,
        //<R>::CType: PartialEq,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unimplemented!()
            //if out_ptr == R::NICHE_VALUE {
            //    return Ok(None);
            //}

            //unsafe { R::try_read_out(out_ptr).map(Some) }
        }
    }
}
