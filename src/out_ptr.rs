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
    /// Marker trait indicating that [`FfiConvert::into_ffi`] and [`FfiConvert::try_from_ffi`] don't
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
    /// Type must not make use of the store during conversion into [`FfiType::ReprC`] via [`FfiConvert::into_ffi`] or [`FfiConvert::try_from_ffi`]
    pub unsafe trait NonLocal: FfiOutPtr {}

    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R: ReprC> NonLocal for R where Self: Ir<Type = Robust> {}
    // SAFETY: Type doesn't use store during conversion
    unsafe impl<R> NonLocal for R where Self: Ir<Type = Opaque> {}
    // SAFETY: Type doesn't return a reference to the store if the inner type doesn't
    unsafe impl<R: Transmute> NonLocal for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: NonLocal,
    {
    }

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
    unsafe impl<R, const N: usize> NonLocal for [R; N] where Self: Ir<Type = [Opaque; N]> {}

    // SAFETY: `Option<T>` doesn't use the store if it's inner type doesn't use it
    unsafe impl<R: NonLocal> NonLocal for Option<R> where Self: Ir<Type = Option<WithoutNiche>> {}
    // SAFETY: `Option<T>` doesn't use the store if it's inner type doesn't use it
    unsafe impl<R: Niche<'_> + NonLocal> NonLocal for Option<R> where Self: Ir<Type = Self> {}
}

disjoint_impls! {
    /// Facilitates the use of [`Self`] as out-pointer.
    ///
    /// If a type implements [`Ir`], i.e. has a defined internal representation,
    /// a blanket implementation is provided.
    pub trait FfiOutPtr: FfiType {
        /// Type of the out-pointer
        type OutPtr: ReprC;
    }

    impl<R: ReprC> FfiOutPtr for R
    where
        Self: Ir<Type = Robust>,
    {
        type OutPtr = Self::ReprC;
    }
    impl<R> FfiOutPtr for R
    where
        Self: Ir<Type = Opaque>,
    {
        type OutPtr = Self::ReprC;
    }
    impl<R: Transmute> FfiOutPtr for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: FfiOutPtr,
    {
        type OutPtr = <<R>::Target as FfiOutPtr>::OutPtr;
    }

    impl<'a, R: Ir<Type = S> + NonLocal, S: Cloned> FfiOutPtr for &'a R
    where
        Self: Ir<Type = &'a S>,
    {
        type OutPtr = <R>::ReprC;
    }

    impl<'a, R: ReprC> FfiOutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        type OutPtr = Self::ReprC;
    }
    impl<'a, R> FfiOutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        type OutPtr = OutBoxedSlice<*const R>;
    }
    impl<'slice, R: Transmute> FfiOutPtr for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: FfiOutPtr,
    {
        type OutPtr = <&'slice [<R>::Target] as FfiOutPtr>::OutPtr;
    }
    impl<'a, R: Ir<Type = S> + NonLocal, S: Cloned> FfiOutPtr for &'a [R]
    where
        Self: Ir<Type = &'a [S]>,
    {
        type OutPtr = OutBoxedSlice<<R>::ReprC>;
    }

    impl<'a, R: ReprC> FfiOutPtr for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        type OutPtr = Self::ReprC;
    }
    impl<'a, R> FfiOutPtr for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Opaque]>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    impl<'slice, R: Transmute> FfiOutPtr for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: FfiOutPtr,
    {
        type OutPtr = <&'slice mut [<R>::Target] as FfiOutPtr>::OutPtr;
    }

    impl<R: ReprC> FfiOutPtr for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        type OutPtr = R;
    }
    impl<R> FfiOutPtr for Box<R>
    where
        Self: Ir<Type = Box<Opaque>>,
    {
        type OutPtr = Self::ReprC;
    }
    impl<R: Transmute> FfiOutPtr for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: FfiOutPtr,
    {
        type OutPtr = <Box<<R>::Target> as FfiOutPtr>::OutPtr;
    }
    impl<R: External> FfiOutPtr for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        type OutPtr = Self::ReprC;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> FfiOutPtr for Box<R>
    where
        Self: Ir<Type = Box<S>>,
    {
        type OutPtr = <R>::ReprC;
    }

    impl<R: ReprC> FfiOutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        type OutPtr = OutBoxedSlice<R>;
    }
    impl<R> FfiOutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    impl<R: Transmute> FfiOutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: FfiOutPtr,
    {
        type OutPtr = <Box<[<R>::Target]> as FfiOutPtr>::OutPtr;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> FfiOutPtr for Box<[R]>
    where
        Self: Ir<Type = Box<[S]>>,
    {
        type OutPtr = OutBoxedSlice<<R>::ReprC>;
    }

    impl<R: ReprC> FfiOutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        type OutPtr = OutBoxedSlice<R>;
    }
    impl<R> FfiOutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        type OutPtr = OutBoxedSlice<*mut R>;
    }
    impl<R: Transmute> FfiOutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: FfiOutPtr,
    {
        type OutPtr = <Vec<<R>::Target> as FfiOutPtr>::OutPtr;
    }
    impl<R: Ir<Type = S> + NonLocal, S: Cloned> FfiOutPtr for Vec<R>
    where
        Self: Ir<Type = Vec<S>>,
    {
        type OutPtr = OutBoxedSlice<<R>::ReprC>;
    }

    impl<R, const N: usize> FfiOutPtr for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        type OutPtr = Self::ReprC;
    }

    impl<R: Ir<Type = S> + NonLocal, S: Cloned, const N: usize> FfiOutPtr for [R; N]
    where
        Self: Ir<Type = [S; N]>,
    {
        type OutPtr = Self::ReprC;
    }

    impl<R: FfiOutPtr> FfiOutPtr for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        type OutPtr = FfiTuple2<<u8 as FfiOutPtr>::OutPtr, <R>::OutPtr>;
    }
    impl<R: Niche<'_> + FfiOutPtr> FfiOutPtr for Option<R>
    where
        Self: Ir<Type = Self>,
    {
        type OutPtr = <R>::OutPtr;
    }

    impl<'itm, R: NonLocal + 'itm, S: Cloned> FfiOutPtr for LocalRef<'itm, R>
    where
        &'itm R: Ir<Type = &'itm S> + FfiOutPtr,
        Self: Ir<Type = &'itm S>,
    {
        type OutPtr = <&'itm R as FfiOutPtr>::OutPtr;
    }
    impl<'itm, R: NonLocal + 'itm, S: Cloned> FfiOutPtr for LocalSlice<'itm, R>
    where
        &'itm [R]: Ir<Type = &'itm [S]> + FfiOutPtr,
        Self: Ir<Type = &'itm [S]>,
    {
        type OutPtr = <&'itm [R] as FfiOutPtr>::OutPtr;
    }
    // FIXME: Check comment in FfiType?
    impl<R, S> FfiOutPtr for LocalSlice<'_, R>
    where
        Vec<R>: Ir<Type = Vec<S>> + FfiOutPtr,
        Self: Ir<Type = Vec<S>>,
    {
        type OutPtr = <Vec<R> as FfiOutPtr>::OutPtr;
    }
}

disjoint_impls! {
    /// Facilitates writing [`Self`] into [`Self::OutPtr`].
    pub trait FfiOutPtrWrite: FfiOutPtr {
        /// Write the given rust value into the corresponding out-pointer
        ///
        /// # Safety
        ///
        /// [`*mut Self::OutPtr`] must be valid
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr);
    }

    impl<R: ReprC> FfiOutPtrWrite for R
    where
        Self: Ir<Type = Robust>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, Robust>(self, out_ptr);
            }
        }
    }
    impl<R> FfiOutPtrWrite for R
    where
        Self: Ir<Type = Opaque>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, Opaque>(self, out_ptr);
            }
        }
    }
    impl<R: Transmute> FfiOutPtrWrite for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: FfiOutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target(self);

            unsafe {
                FfiOutPtrWrite::write_out(transmuted, out_ptr)
            }
        }
    }

    impl<'a, R: ReprC> FfiOutPtrWrite for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, &'a [Robust]>(self, out_ptr);
            }
        }
    }
    impl<'a, R: Clone> FfiOutPtrWrite for &'a [R]
    where
        Self: Ir<Type = &'a [Opaque]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            CTypeConvert::<&[Opaque], _>::into_repr_c(self, &mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<'slice, R: Transmute> FfiOutPtrWrite for &'slice [R]
    where
        Self: Ir<Type = &'slice [Transparent]>,
        &'slice [<R>::Target]: FfiOutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_ref_slice(self);

            unsafe {
                FfiOutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone, S: Cloned> FfiOutPtrWrite for &'itm [R]
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = &'itm [S]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                CTypeConvert::<&[S], _>::into_repr_c(self, store_borrow);
                let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

                out_ptr.write(output);
            }
        }
    }

    impl<'a, R: ReprC> FfiOutPtrWrite for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, &'a mut [Robust]>(self, out_ptr);
            }
        }
    }
    impl<'a, R: Clone> FfiOutPtrWrite for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Opaque]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            CTypeConvert::<&'a mut [Opaque], _>::into_repr_c(self, &mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<'slice, R: Transmute> FfiOutPtrWrite for &'slice mut [R]
    where
        Self: Ir<Type = &'slice mut [Transparent]>,
        &'slice mut [<R>::Target]: FfiOutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_slice_mut(self);

            unsafe {
                FfiOutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }

    impl<R: ReprC> FfiOutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                out_ptr.write(*self);
            }
        }
    }
    impl<R> FfiOutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, Box<Opaque>>(self, out_ptr);
            }
        }
    }
    impl<R: Transmute> FfiOutPtrWrite for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: FfiOutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_box(self);

            unsafe {
                FfiOutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrWrite for Box<R>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = Box<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                CTypeConvert::<Box<S>, _>::into_repr_c(self, store_borrow);

                // NOTE: None value indicates a bug in the implementation
                out_ptr.write(store.0.expect("Store must be initialized"));
            }
        }
    }

    impl<R: ReprC> FfiOutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            CTypeConvert::<Box<[Robust]>, _>::into_repr_c(self, &mut store);

            unsafe {
                out_ptr.write(OutBoxedSlice::from_boxed_slice(Some(store)));
            }
        }
    }
    impl<R> FfiOutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Opaque]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            CTypeConvert::<Box<[Opaque]>, _>::into_repr_c(self, &mut store);

            unsafe {
                out_ptr.write(OutBoxedSlice::from_boxed_slice(Some(store)));
            }
        }
    }
    impl<R: Transmute> FfiOutPtrWrite for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: FfiOutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_boxed_slice(self);

            unsafe {
                FfiOutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrWrite for Box<[R]>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = Box<[S]>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                CTypeConvert::<Box<[S]>, _>::into_repr_c(self, store_borrow);
                let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

                out_ptr.write(output);
            }
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone, S: Cloned + 'itm> FfiOutPtrWrite for &'itm R
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = &'itm S>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                CTypeConvert::<&S, _>::into_repr_c(self, store_borrow);

                // NOTE: None value indicates a bug in the implementation
                out_ptr.write(store.0.expect("Store must be initialized"));
            }
        }
    }

    impl<R: ReprC> FfiOutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();
            CTypeConvert::<Vec<Robust>, _>::into_repr_c(self, &mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<R> FfiOutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Opaque>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            CTypeConvert::<Vec<Opaque>, _>::into_repr_c(self, &mut store);
            let output = OutBoxedSlice::from_boxed_slice(Some(store));

            unsafe {
                out_ptr.write(output);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrWrite for Vec<R>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = Vec<S>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);

                CTypeConvert::<Vec<S>, _>::into_repr_c(self, store_borrow);
                let output = OutBoxedSlice::from_boxed_slice(Some(store.0));

                out_ptr.write(output);
            }
        }
    }

    impl<R: Transmute> FfiOutPtrWrite for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: FfiOutPtrWrite,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let transmuted = transmute_into_target_vec(self);

            unsafe {
                FfiOutPtrWrite::write_out(transmuted, out_ptr);
            }
        }
    }

    impl<R, const N: usize> FfiOutPtrWrite for [R; N]
    where
        Self: Ir<Type = [Opaque; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                write_non_local::<_, [Opaque; N]>(self, out_ptr);
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm, const N: usize> FfiOutPtrWrite for [R; N]
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        [<R>::RustStore; N]: Default,
        [<R>::FfiStore; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::into_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let item = Self::into_repr_c(self, store_borrow);

                out_ptr.write(item);
            }
        }
    }

    impl<R: FfiOutPtrWrite> FfiOutPtrWrite for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            // NOTE: Makes the code much more readable
            #[allow(clippy::option_if_let_else)]
            match self {
                None => {
                    let mut discriminant_out_ptr = core::mem::MaybeUninit::uninit();
                    unsafe {
                        FfiOutPtrWrite::write_out(0u8, discriminant_out_ptr.as_mut_ptr());
                        let discriminant_out_ptr = discriminant_out_ptr.assume_init() ;

                        // TODO: No need to zero the memory because it must never be read
                        out_ptr.write(FfiTuple2(discriminant_out_ptr, core::mem::zeroed()));
                    }
                }
                Some(value) => {
                    unsafe {
                        let mut discriminant_out_ptr = core::mem::MaybeUninit::uninit();
                        FfiOutPtrWrite::write_out(1u8, discriminant_out_ptr.as_mut_ptr());
                        let discriminant_out_ptr = discriminant_out_ptr.assume_init();

                        let mut value_out_ptr = core::mem::MaybeUninit::uninit();
                        FfiOutPtrWrite::write_out(value, value_out_ptr.as_mut_ptr());
                        let value_out_ptr = value_out_ptr.assume_init();

                        out_ptr.write(FfiTuple2(discriminant_out_ptr, value_out_ptr));
                    }
                }
            }
        }
    }
    impl<R: Niche<'_> + FfiOutPtrWrite<OutPtr = <R as FfiType>::ReprC>> FfiOutPtrWrite for Option<R>
    where
        Self: Ir<Type = Self>,
    {
        unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
            unsafe {
                self.map_or_else(
                    || out_ptr.write(<R>::NICHE_VALUE),
                    |value| FfiOutPtrWrite::write_out(value, out_ptr),
                );
            }
        }
    }
}

disjoint_impls! {
    /// Facilitates reading from [`Self::OutPtr`] out-pointer.
    pub trait FfiOutPtrRead: FfiOutPtr + Sized {
        /// Read a rust value from the corresponding out-pointer
        ///
        /// # Errors
        ///
        /// Check [`FfiConvert::try_from_ffi`]
        ///
        /// # Safety
        ///
        /// Check [`FfiConvert::try_from_ffi`]
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self>;
    }

    impl<R: ReprC> FfiOutPtrRead for R
    where
        Self: Ir<Type = Robust>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, Robust>(out_ptr) }
        }
    }
    impl<R: Transmute> FfiOutPtrRead for R
    where
        Self: Ir<Type = Transparent>,
        <R>::Target: FfiOutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                FfiOutPtrRead::try_read_out(out_ptr).and_then(|output| transmute_from_target(output))
            }
        }
    }

    impl<'a, R: ReprC> FfiOutPtrRead for &'a [R]
    where
        Self: Ir<Type = &'a [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, &'a [Robust]>(out_ptr) }
        }
    }
    impl<'itm, R: Transmute> FfiOutPtrRead for &'itm [R]
    where
        Self: Ir<Type = &'itm [Transparent]>,
        &'itm [<R>::Target]: FfiOutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <&[<R>::Target]>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_ref_slice(output))
            }
        }
    }

    impl<'a, R: ReprC> FfiOutPtrRead for &'a mut [R]
    where
        Self: Ir<Type = &'a mut [Robust]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, &mut [Robust]>(out_ptr) }
        }
    }
    impl<'itm, R: Transmute> FfiOutPtrRead for &'itm mut [R]
    where
        Self: Ir<Type = &'itm mut [Transparent]>,
        &'itm mut [<R>::Target]: FfiOutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <&mut [<R>::Target]>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_slice_mut(output))
            }
        }
    }

    impl<R: ReprC> FfiOutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { CTypeConvert::<Robust, _>::try_from_repr_c(out_ptr, &mut ()).map(Box::new) }
        }
    }
    impl<R: External> FfiOutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Extern>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe { read_non_local::<_, Box<Extern>>(out_ptr) }
        }
    }
    impl<R: Transmute> FfiOutPtrRead for Box<R>
    where
        Self: Ir<Type = Box<Transparent>>,
        Box<<R>::Target>: FfiOutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                Box::<<R>::Target>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_box(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrRead for Box<R>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = Box<S>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            let item = unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                <R>::try_from_repr_c(out_ptr, store_borrow)?
            };

            Ok(Box::new(item))
        }
    }

    impl<R: ReprC> FfiOutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[Robust]>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefMutSlice::from_raw_parts_mut(out_ptr.as_mut_ptr(), out_ptr.len());
                let res = CTypeConvert::<Box<[Robust]>, _>::try_from_repr_c(slice, &mut ());

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }
    impl<R: Transmute> FfiOutPtrRead for Box<[R]>
    where
        Self: Ir<Type = Box<[Transparent]>>,
        Box<[<R>::Target]>: FfiOutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <Box<[<R>::Target]>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_boxed_slice(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrRead for Box<[R]>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = Box<[S]>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefMutSlice::from_raw_parts_mut(out_ptr.as_mut_ptr(), out_ptr.len());

                let mut store = Default::default();
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let res = Self::try_from_repr_c(slice, store_borrow);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }

    impl<R: ReprC> FfiOutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<Robust>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefMutSlice::from_raw_parts_mut(out_ptr.as_mut_ptr(), out_ptr.len());
                let res = CTypeConvert::<Vec<Robust>, _>::try_from_repr_c(slice, &mut ());

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }
    impl<R: Transmute> FfiOutPtrRead for Vec<R>
    where
        Self: Ir<Type = Vec<Transparent>>,
        Vec<<R>::Target>: FfiOutPtrRead,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                <Vec<<R>::Target>>::try_read_out(out_ptr)
                    .and_then(|output| transmute_from_target_vec(output))
            }
        }
    }
    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrRead for Vec<R>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = Vec<S>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            unsafe {
                let slice = RefMutSlice::from_raw_parts_mut(out_ptr.as_mut_ptr(), out_ptr.len());

                let mut store = Default::default();
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let res = Self::try_from_repr_c(slice, store_borrow);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res
            }
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm, const N: usize>
        FfiOutPtrRead for [R; N]
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        // FIXME: What is this bound?
        //[R; N]: CTypeConvert<'itm, [S; N], [R; N]::ReprC>
        [<R>::RustStore; N]: Default,
        [<R>::FfiStore; N]: Default,
        Self: Ir<Type = [S; N]>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                Self::try_from_repr_c(out_ptr, store_borrow)
            }
        }
    }

    impl<R: FfiOutPtrRead> FfiOutPtrRead for Option<R>
    where
        Self: Ir<Type = Option<WithoutNiche>>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            match unsafe { <u8 as FfiOutPtrRead>::try_read_out(out_ptr.0)? } {
                0 => Ok(None),
                1 => Ok(Some(unsafe { <R>::try_read_out(out_ptr.1)? })),
                _ => Err(FfiReturn::TrapRepresentation),
            }
        }
    }
    impl<R: Niche<'_> + FfiOutPtrRead<OutPtr = <R as FfiType>::ReprC>> FfiOutPtrRead for Option<R>
    where
        Self: Ir<Type = Self>,
        <R>::ReprC: PartialEq,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            if out_ptr == <R>::NICHE_VALUE {
                return Ok(None);
            }

            unsafe { <R>::try_read_out(out_ptr).map(Some) }
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrRead
        for LocalSlice<'itm, R>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = &'itm [S]>,
    {
        unsafe fn try_read_out(out_ptr: OutBoxedSlice<<R>::ReprC>) -> Result<Self> {
            let slice = RefSlice::from_raw_parts(out_ptr.as_mut_ptr(), out_ptr.len());

            let mut store = Default::default();

            unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                let res = <&[R]>::try_from_repr_c(slice, store_borrow);

                if !out_ptr.deallocate() {
                    return Err(FfiReturn::TrapRepresentation);
                }

                res?;
            }

            Ok(Self(store.0, core::marker::PhantomData))
        }
    }

    impl<'itm, R: Ir<Type = S> + NonLocal + Clone + 'itm, S: Cloned + 'itm> FfiOutPtrRead
        for LocalRef<'itm, R>
    where
        R: CTypeConvert<'itm, S, <R>::ReprC>,
        Self: Ir<Type = &'itm S>,
    {
        unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
            let mut store = Default::default();

            let item = unsafe {
                // NOTE: Bypasses the erroneous lifetime check.
                // Correct as long as `R::try_from_repr_c` doesn't return a reference to the store (`R: NonLocal`)
                let store_borrow = &mut *addr_of_mut!(store);
                <R>::try_from_repr_c(out_ptr, store_borrow)?
            };

            Ok(Self(item, core::marker::PhantomData))
        }
    }
}
