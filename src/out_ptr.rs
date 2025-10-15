use core::ptr::addr_of_mut;

use super::*;
use crate::{
    repr_c::read_non_local,
    transmute::{
        transmute_from_target_box, transmute_from_target_boxed_slice,
        transmute_from_target_ref_slice, transmute_from_target_slice_mut,
        transmute_from_target_vec,
    },
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
    pub unsafe trait NonLocal: OutPtr {}

    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R: Ir<Type = Robust> + ReprC> NonLocal for R where {}
    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R: Ir<Type = Opaque>> NonLocal for R {}
    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<R: Ir<Type = Transparent> + Transmute> NonLocal for R
    where
        <Self as Transmute>::Target: NonLocal
    {
    }
    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R: Ir<Type = Extern> + External> NonLocal for R {}

    // SAFETY: Type doesn't use store during conversion
    unsafe impl<'a, R: ReprC> NonLocal for &'a [R] where Self: Ir<Type = &'a [Robust]> {}

    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<'slice, R: Transmute> NonLocal for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: NonLocal,
    {
    }

    // SAFETY: Type doesn't use store during conversion
    unsafe impl<'a, R: ReprC> NonLocal for &'a mut [R] where Self: Ir<Type = &'a mut [Robust]> {}
    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<'slice, R: Transmute> NonLocal for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: NonLocal,
    {
    }

    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R> NonLocal for Box<R> where Self: Ir<Type = Box<Opaque>> {}
    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<R: Transmute> NonLocal for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: NonLocal,
    {
    }
    unsafe impl<R: External> NonLocal for Box<R> where Self: Ir<Type = Box<Extern>> {}

    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<R: Transmute> NonLocal for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: NonLocal,
    {
    }

    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<R: Transmute> NonLocal for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: NonLocal,
    {
    }

    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R: ReprC, const N: usize> NonLocal for [R; N] where Self: Ir<Type = [Robust; N]> {}
    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R, const N: usize> NonLocal for [R; N] where Self: Ir<Type = [Opaque; N]> {}

    // SAFETY: `Option<T>` doesn't use the store if it's inner type doesn't use it
    unsafe impl<R: NonLocal> NonLocal for Option<R> where Self: Ir<Type = Option<WithoutNiche>> {}
    // SAFETY: `Option<T>` doesn't use the store if it's inner type doesn't use it
    unsafe impl<R: Niche + NonLocal> NonLocal for Option<R> where Self: Ir<Type = Self> {}
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
    impl<R: Transmute> OutPtr for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: OutPtr,
    {
        type OutPtr = <<R>::Target as OutPtr>::OutPtr;
    }
    impl<R: Ir<Type = Extern> + External> OutPtr for R {
        type OutPtr = Self::CType;
    }

    impl<'a, R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type OutPtr = <R>::CType;
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
    impl<'slice, R: Transmute> OutPtr for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: OutPtr,
    {
        type OutPtr = <&'slice [<R>::Target] as OutPtr>::OutPtr;
    }
    impl<'a, R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [S]>,
    {
        type OutPtr = OutBoxedSlice<<R>::CType>;
    }

    impl<'a, R: ReprC> OutPtr for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type OutPtr = Self::CType;
    }
    impl<'a, R> OutPtr for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Opaque]>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    impl<'slice, R: Transmute> OutPtr for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: OutPtr,
    {
        type OutPtr = <&'slice mut [<R>::Target] as OutPtr>::OutPtr;
    }

    impl<R: ReprC> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type OutPtr = R;
    }
    impl<R> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<Opaque>>,
    {
        type OutPtr = Self::CType;
    }
    impl<R: Transmute> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: OutPtr,
    {
        type OutPtr = <Box<<R>::Target> as OutPtr>::OutPtr;
    }
    impl<R: External> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type OutPtr = Self::CType;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type OutPtr = <R>::CType;
    }

    impl<R: ReprC> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type OutPtr = OutBoxedSlice<R>;
    }
    impl<R> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    impl<R: Transmute> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: OutPtr,
    {
        type OutPtr = <Box<[<R>::Target]> as OutPtr>::OutPtr;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type OutPtr = OutBoxedSlice<<R>::CType>;
    }

    impl<R: ReprC> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type OutPtr = OutBoxedSlice<R>;
    }
    impl<R> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    impl<R: Transmute> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: OutPtr,
    {
        type OutPtr = <Vec<<R>::Target> as OutPtr>::OutPtr;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> OutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type OutPtr = OutBoxedSlice<<R>::CType>;
    }

    impl<R: ReprC, const N: usize> OutPtr for [R; N]
    where
        Self: Ir<Type = [Robust; N]>,
    {
        type OutPtr = Self::CType;
    }
    impl<R, const N: usize> OutPtr for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type OutPtr = Self::CType;
    }

    impl<R: Ir<Type = S> + NonLocal, S: Cloned, const N: usize> OutPtr for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type OutPtr = Self::CType;
    }

    impl<R: OutPtr> OutPtr for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type OutPtr = FfiTuple2<<u8 as OutPtr>::OutPtr, <R>::OutPtr>;
    }
    impl<R: Niche + OutPtr> OutPtr for Option<R>
    where
        Self: Ir<Type = Self>,
    {
        type OutPtr = <R>::OutPtr;
    }

    impl<'itm, R: NonLocal + 'itm, S: Cloned> OutPtr for LocalRef<'itm, R>
    where
        &'itm R: Ir<Type = &'itm S> + OutPtr,
        Self: Ir<Type = &'itm S>,
    {
        type OutPtr = <&'itm R as OutPtr>::OutPtr;
    }
    impl<'itm, R: NonLocal + 'itm, S: Cloned> OutPtr for LocalSlice<'itm, R>
    where
        &'itm [R]: Ir<Type = &'itm [S]> + OutPtr,
        Self: Ir<Type = &'itm [S]>,
    {
        type OutPtr = <&'itm [R] as OutPtr>::OutPtr;
    }
    // FIXME: Check comment in ExternC?
    impl<R, S> OutPtr for LocalSlice<'_, R>
    where
        Vec<R>: Ir<Type = Vec<S>> + OutPtr,
        Self: Ir<Type = Vec<S>>,
    {
        type OutPtr = <Vec<R> as OutPtr>::OutPtr;
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
        Self: Ir<Type = Robust>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, Robust>(self, out_ptr);
            }
        }
    }
    impl<R> OutPtrWrite for R
    where
        Self: Ir<Type = Opaque>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, Opaque>(self, out_ptr);
            }
        }
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

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone, S: Cloned + 'itm> OutPtrWrite for &'itm R
    where
        R: Encode<'itm>,
        Self: Ir<Type = &'itm S>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                Encode::encode(self, store_borrow);

                // NOTE: None value indicates a bug in the implementation
                out_ptr.write(store.0.expect("Store must be initialized"));
            }
        }
    }

    impl<'a, R: ReprC> OutPtrWrite for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, &'a [Robust]>(self, out_ptr);
            }
        }
    }
    impl<'a, R: Clone> OutPtrWrite for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unimplemented!();
            //let mut store = Default::default();

            //Encode::encode(self, &mut store);
            //let output = OutBoxedSlice::from_boxed_slice(Some(store));

            //unsafe {
            //    out_ptr.write(output);
            //}
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
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone, S: Cloned> OutPtrWrite for &'itm [R]
    where
        R: Encode<'itm>,
        Self: Ir<Type = &'itm [S]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                Encode::encode(self, store_borrow);
                let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

                out_ptr.write(output);
            }
        }
    }

    impl<'a, R: ReprC> OutPtrWrite for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, &'a mut [Robust]>(self, out_ptr);
            }
        }
    }
    impl<'a, R: Clone> OutPtrWrite for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Opaque]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unimplemented!();
            //let mut store = Default::default();
            //Encode::encode(self, &mut store);
            //let output = OutBoxedSlice::from_boxed_slice(Some(store));

            //unsafe {
            //    out_ptr.write(output);
            //}
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
    impl<R> OutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, Box<Opaque>>(self, out_ptr);
            }
        }
    }
    impl<R: Transmute> OutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: OutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_box(self);

            unsafe {
                OutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrWrite for Box<R>
    where
        R: Encode<'itm>,
        Self: Ir<Type = Box<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                Encode::encode(self, store_borrow);

                // NOTE: None value indicates a bug in the implementation
                out_ptr.write(store.0.expect("Store must be initialized"));
            }
        }
    }

    impl<R: ReprC> OutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            Encode::encode(self, &mut store);

            unsafe {
                out_ptr.write(OutBoxedSlice::from_boxed_slice(Some(store)));
            }
        }
    }
    impl<R> OutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            Encode::encode(self, &mut store);

            unsafe {
                out_ptr.write(OutBoxedSlice::from_boxed_slice(Some(store)));
            }
        }
    }
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
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrWrite for Box<[R]>
    where
        R: Encode<'itm>,
        Self: Ir<Type = Box<[S]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                Encode::encode(self, store_borrow);
                let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

                out_ptr.write(output);
            }
        }
    }

    impl<R: ReprC> OutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            Encode::encode(self, &mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<R> OutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            Encode::encode(self, &mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrWrite for Vec<R>
    where
        R: Encode<'itm>,
        Self: Ir<Type = Vec<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);

                Encode::encode(self, store_borrow);
                let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

                out_ptr.write(output);
            }
        }
    }

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

    impl<R: ReprC, const N: usize> OutPtrWrite for [R; N]
    where
        Self: Ir<Type = [Robust; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();

            unsafe {
                write_non_local::<_, [Robust; N]>(self, out_ptr);
            }
        }
    }
    impl<R, const N: usize> OutPtrWrite for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();

            unsafe {
                write_non_local::<_, [Opaque; N]>(self, out_ptr);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm, const N: usize> OutPtrWrite for [R; N]
    where
        R: Encode<'itm>,
        [<R>::Store; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            assert_arr_has_non_zero_len::<N>();
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let item = Self::encode(self, store_borrow);

                out_ptr.write(item);
            }
        }
    }

    impl<R: OutPtrWrite> OutPtrWrite for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            match self {
                None => {
                    let mut discriminant_out_ptr = core::mem::MaybeUninit::uninit();
                    unsafe {
                        OutPtrWrite::write_out(0u8, discriminant_out_ptr.as_mut_ptr());
                        let discriminant_out_ptr = discriminant_out_ptr.assume_init() ;

                        // TODO: No need to zero the memory because it must never be read
                        out_ptr.write(FfiTuple2(discriminant_out_ptr, core::mem::zeroed()));
                    }
                }
                Some(value) => {
                    unsafe {
                        let mut discriminant_out_ptr = core::mem::MaybeUninit::uninit();
                        OutPtrWrite::write_out(1u8, discriminant_out_ptr.as_mut_ptr());
                        let discriminant_out_ptr = discriminant_out_ptr.assume_init();

                        let mut value_out_ptr = core::mem::MaybeUninit::uninit();
                        OutPtrWrite::write_out(value, value_out_ptr.as_mut_ptr());
                        let value_out_ptr = value_out_ptr.assume_init();

                        out_ptr.write(FfiTuple2(discriminant_out_ptr, value_out_ptr));
                    }
                }
            }
        }
    }
    impl<R: Niche + OutPtrWrite<OutPtr = <R as ExternC>::CType>> OutPtrWrite for Option<R>
    where
        Self: Ir<Type = Self>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                self.map_or_else(
                    || out_ptr.write(<R>::NICHE_VALUE),
                    |value| OutPtrWrite::write_out(value, out_ptr),
                );
            }
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

    impl<R: ReprC> OutPtrRead for R
    where
        Self: Ir<Type = Robust>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, Robust>(out_ptr) }
        }
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
    impl<R: Ir<Type = Extern> + External> OutPtrRead for R {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, Extern>(out_ptr) }
        }
    }

    impl<'a, R: ReprC> OutPtrRead for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, &'a [Robust]>(out_ptr) }
        }
    }
    impl<'itm, R: Transmute> OutPtrRead for &'itm [R]
    where
        Self: Ir<Type = &'itm [Transparent]>,
        &'itm [<R>::Target]: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <&[<R>::Target]>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_ref_slice(output))
            }
        }
    }

    impl<'a, R: ReprC> OutPtrRead for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, &mut [Robust]>(out_ptr) }
        }
    }
    impl<'itm, R: Transmute> OutPtrRead for &'itm mut [R]
    where
        Self: Ir<Type = &'itm mut [Transparent]>,
        &'itm mut [<R>::Target]: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <&mut [<R>::Target]>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_slice_mut(output))
            }
        }
    }

    impl<R: ReprC> OutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unimplemented!()
            //unsafe { Decode::decode(out_ptr, &mut ()).map(Box::new) }
        }
    }
    impl<R: External> OutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, Box<Extern>>(out_ptr) }
        }
    }
    impl<R: Transmute> OutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                Box::<<R>::Target>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_box(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrRead for Box<R>
    where
        R: Decode<'itm>,
        Self: Ir<Type = Box<S>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            let item = unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                <R>::decode(out_ptr, store_borrow)?
            };

            Ok(Box::new(item))
        }
    }

    impl<R: ReprC> OutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());
                let res = Decode::decode(slice, &mut ());

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }
    impl<R: Transmute> OutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <Box<[<R>::Target]>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrRead for Box<[R]>
    where
        R: Decode<'itm>,
        Self: Ir<Type = Box<[S]>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

                let mut store = Default::default();
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let res = Self::decode(slice, store_borrow);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }

    impl<R: ReprC> OutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());
                let res = Decode::decode(slice, &mut ());

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }
    impl<R: Transmute> OutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: OutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <Vec<<R>::Target>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrRead for Vec<R>
    where
        R: Decode<'itm>,
        Self: Ir<Type = Vec<S>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

                let mut store = Default::default();
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let res = Self::decode(slice, store_borrow);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }

    impl<R: ReprC, const N: usize> OutPtrRead for [R; N]
    where
        Self: Ir<Type = [Robust; N]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            assert_arr_has_non_zero_len::<N>();
            unsafe { read_non_local::<_, [Robust; N]>(out_ptr) }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm, const N: usize>
        OutPtrRead for [R; N]
    where
        R: Decode<'itm>,
        // FIXME: What is this bound?
        //[R; N]: Decode<'itm>
        [<R>::Store; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            assert_arr_has_non_zero_len::<N>();
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                Self::decode(out_ptr, store_borrow)
            }
        }
    }

    impl<R: OutPtrRead> OutPtrRead for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            match unsafe { <u8 as OutPtrRead>::try_read_out(out_ptr.0)? } {
                0 => Ok(None),
                1 => Ok(Some(unsafe { <R>::try_read_out(out_ptr.1)? })),
                _ => Err(FfiReturn::TrapRepresentation),
            }
        }
    }
    impl<R: Niche + OutPtrRead<OutPtr = <R as ExternC>::CType>> OutPtrRead for Option<R>
    where
        Self: Ir<Type = Self>,
        <R>::CType: PartialEq,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            if out_ptr == <R>::NICHE_VALUE {
                return Ok(None);
            }

            unsafe { <R>::try_read_out(out_ptr).map(Some) }
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrRead
        for LocalSlice<'itm, R>
    where
        R: Decode<'itm>,
        Self: Ir<Type = &'itm [S]>,
    {
        unsafe fn try_read_out(out_ptr: OutBoxedSlice<<R>::CType>) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let res = <&[R]>::decode(slice, store_borrow);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res?;
            }

            Ok(Self::new(store.0))
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> OutPtrRead
        for LocalRef<'itm, R>
    where
        R: Decode<'itm>,
        Self: Ir<Type = &'itm S>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            let item = unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                <R>::decode(out_ptr, store_borrow)?
            };

            Ok(Self::new(item))
        }
    }
}
