//! Utilities for defining opaque pointer handles and shared handle logic.

/// Type of the handle identifier
pub type Id = u8;

/// Represents an opaque handle in an FFI context
///
/// # Safety
///
/// If two structures implement the same id, it may result in a void pointer cast to a wrong type
pub unsafe trait Handle {
    /// Unique identifier of the handle. Most commonly, it is
    /// used to facilitate generic monomorphization over FFI
    const ID: Id;
}

/// Implements [`Handle`] for a list of types, starting from the given initial ID.
///
/// Each type in the macro invocation is assigned an ID incrementally.
///
/// # Example
///
/// ```rust
/// struct Foo1;
/// struct Foo2;
/// struct Bar1;
/// struct Bar2;
///
/// co3::handles! {Foo1, Foo2, Bar1, Bar2}
///
/// /* will produce:
/// impl Handle for Foo1 {
///     const ID: Id = 0;
/// }
/// impl Handle for Foo2 {
///     const ID: Id = 1;
/// }
/// impl Handle for Bar1 {
///     const ID: Id = 2;
/// }
/// impl Handle for Bar2 {
///     const ID: Id = 3;
/// } */
/// ```
#[macro_export]
macro_rules! handles {
    ( $($other:ty),+ $(,)? ) => {
        $crate::handles! {0, $( $other ),+}
    };
    ( $id:expr, $ty:ty $(, $other:ty)* $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: $crate::handle::Id = $id;
        }

        $crate::handles! {$id + 1, $( $other ),*}
    };
    ( $id:expr, $(,)? ) => {};
}

/// Generate FFI equivalent implementation of requested trait methods (e.g. Clone, Eq, Ord).
///
/// One `[prefix]<fn_name>` is generated per invocation of this macro. User should ensure
/// function names don't collide by using a globally (per dynamic library) unique prefix.
///
/// If handle IDs are globally unique across the linked crates, only one crate should export a
/// given shared function symbol and all other crates should import that symbol. This approach is
/// somewhat brittle: an ID collision across crates can cause mismatched dispatch and potential UB.
///
/// If IDs are only unique per crate, each crate should export its own prefixed symbols. In this
/// mode symbol name collisions are caught by the linker when duplicate exports share the same name.
///
/// The only exception to the rule is `__co3_dealloc` function which is forced to be globally
/// unique, i.e. it's not possible to define multiple versions by setting a prefix.
#[macro_export]
macro_rules! def_fns {
    (@catch_unwind $block:block ) => {
        match std::panic::catch_unwind(|| unsafe { $block }) {
            Ok(res) => match res {
                Ok(()) => $crate::FfiReturn::Ok,
                Err(err) => err.into(),
            },
            Err(_) => {
                // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                $crate::FfiReturn::UnrecoverableError
            },
        }
    };
    ( $($fn_name:ident: {$($other:ty),+ $(,)?}),+ $(,)?) => {
        $crate::def_fns! { link_prefix = "__co3_" $( $fn_name: {$( $other ),+ }),+ }
    };
    ( link_prefix = $prefix:literal $($fn_name:ident: {$($other:ty),+ $(,)?}),+ $(,)?) => {
        mod __co3_export {
            use super::*;

            $( $crate::def_fns! {@def: $prefix $fn_name: $( $other ),+ } )+
        }
    };
    ( @def: $prefix:literal Clone: $( $other:ty ),+ $(,)? ) => {
        /// FFI function equivalent of [`Clone::clone`]
        ///
        /// # Safety
        ///
        /// All of the given pointers must be valid and the given handle id must match the expected
        /// pointer type
        #[unsafe(export_name = concat!($prefix, "clone"))]
        unsafe extern "C" fn clone(
            handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
            handle_ptr: *const core::ffi::c_void,
            out_ptr: *mut *mut core::ffi::c_void
        ) -> $crate::FfiReturn {
            $crate::def_fns!(@catch_unwind {
                match $crate::Decode::decode(handle_id, &mut ())
                    .ok_or($crate::FfiReturn::TrapRepresentation)? {
                    $( <$other as $crate::handle::Handle>::ID => {
                        let handle_ref: &$other = $crate::Decode::decode(
                            handle_ptr as <&$other as $crate::ExternC>::CType,
                            &mut ()
                        ).ok_or($crate::FfiReturn::TrapRepresentation)?;
                        <$other as $crate::out_ptr::OutPtrWrite>::write_out(Clone::clone(handle_ref), out_ptr.cast::<<$other as $crate::ExternC>::CType>());
                    } )+
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    _ => return Err($crate::FfiReturn::UnknownHandle),
                }

                Ok(())
            })
        }
    };
    ( @def: $prefix:literal Default: $( $other:ty ),+ $(,)? ) => {
        /// FFI function equivalent of [`Default::default`]
        ///
        /// # Safety
        ///
        /// All of the given pointers must be valid and the given handle id must match the expected
        /// pointer type
        #[unsafe(export_name = concat!($prefix, "default"))]
        unsafe extern "C" fn default(
            handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
            out_ptr: *mut *mut core::ffi::c_void
        ) -> $crate::FfiReturn {
            $crate::def_fns!(@catch_unwind {
                match $crate::Decode::decode(handle_id, &mut ())
                    .ok_or($crate::FfiReturn::TrapRepresentation)? {
                    $( <$other as $crate::handle::Handle>::ID => {
                        let default_value = Default::default();

                        let out_ptr = out_ptr.cast::<<$other as $crate::ExternC>::CType>();
                        <$other as $crate::out_ptr::OutPtrWrite>::write_out(default_value, out_ptr);
                    } )+
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    _ => return Err($crate::FfiReturn::UnknownHandle),
                }

                Ok(())
            })
        }
    };
    ( @def: $prefix:literal Eq: $( $other:ty ),+ $(,)? ) => {
        /// FFI function equivalent of [`Eq::eq`]
        ///
        /// # Safety
        ///
        /// All of the given pointers must be valid and the given handle id must match the expected
        /// pointer type
        #[unsafe(export_name = concat!($prefix, "eq"))]
        unsafe extern "C" fn eq(
            handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
            left_handle_ptr: *const core::ffi::c_void,
            right_handle_ptr: *const core::ffi::c_void,
            out_ptr: *mut <bool as $crate::out_ptr::OutPtr>::OutPtr,
        ) -> $crate::FfiReturn {
            $crate::def_fns!(@catch_unwind {
                match $crate::Decode::decode(handle_id, &mut ())
                    .ok_or($crate::FfiReturn::TrapRepresentation)? {
                    $( <$other as $crate::handle::Handle>::ID => {
                        let (lhandle_ptr, rhandle_ptr) = (
                            left_handle_ptr as <&$other as $crate::ExternC>::CType,
                            right_handle_ptr as <&$other as $crate::ExternC>::CType
                        );

                        let mut lhandle_store = Default::default();
                        let mut rhandle_store = Default::default();

                        let lhandle: &$other = $crate::Decode::decode(lhandle_ptr, &mut lhandle_store)
                            .ok_or($crate::FfiReturn::TrapRepresentation)?;
                        let rhandle: &$other = $crate::Decode::decode(rhandle_ptr, &mut rhandle_store)
                            .ok_or($crate::FfiReturn::TrapRepresentation)?;

                        <bool as $crate::out_ptr::OutPtrWrite>::write_out(lhandle == rhandle, out_ptr);
                    } )+
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    _ => return Err($crate::FfiReturn::UnknownHandle),
                }

                Ok(())
            })
        }
    };
    ( @def: $prefix:literal Ord: $( $other:ty ),+ $(,)? ) => {
        /// FFI function equivalent of [`Ord::ord`]
        ///
        /// # Safety
        ///
        /// All of the given pointers must be valid and the given handle id must match the expected
        /// pointer type
        #[unsafe(export_name = concat!($prefix, "ord"))]
        unsafe extern "C" fn ord(
            handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
            left_handle_ptr: *const core::ffi::c_void,
            right_handle_ptr: *const core::ffi::c_void,
            out_ptr: *mut <core::cmp::Ordering as $crate::out_ptr::OutPtr>::OutPtr,
        ) -> $crate::FfiReturn {
            $crate::def_fns!(@catch_unwind {
                match $crate::Decode::decode(handle_id, &mut ())
                    .ok_or($crate::FfiReturn::TrapRepresentation)? {
                    $( <$other as $crate::handle::Handle>::ID => {
                        let (lhandle_ptr, rhandle_ptr) = (
                            left_handle_ptr as <&$other as $crate::ExternC>::CType,
                            right_handle_ptr as <&$other as $crate::ExternC>::CType
                        );

                        let mut lhandle_store = Default::default();
                        let mut rhandle_store = Default::default();

                        let lhandle: &$other = $crate::Decode::decode(lhandle_ptr, &mut lhandle_store)
                            .ok_or($crate::FfiReturn::TrapRepresentation)?;
                        let rhandle: &$other = $crate::Decode::decode(rhandle_ptr, &mut rhandle_store)
                            .ok_or($crate::FfiReturn::TrapRepresentation)?;

                        <core::cmp::Ordering as $crate::out_ptr::OutPtrWrite>::write_out(lhandle.cmp(rhandle), out_ptr);
                    } )+
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    _ => return Err($crate::FfiReturn::UnknownHandle),
                }

                Ok(())
            })
        }
    };
    ( @def: $prefix:literal Drop: $( $other:ty ),+ $(,)? ) => {
        /// FFI function equivalent of [`Drop::drop`]
        ///
        /// # Safety
        ///
        /// All of the given pointers must be valid and the given handle id must match the expected
        /// pointer type
        #[unsafe(export_name = concat!($prefix, "drop"))]
        unsafe extern "C" fn drop(
            handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
            handle_ptr: *mut core::ffi::c_void,
        ) -> $crate::FfiReturn {
            $crate::def_fns!(@catch_unwind {
                match $crate::Decode::decode(handle_id, &mut ())
                    .ok_or($crate::FfiReturn::TrapRepresentation)? {
                    $( <$other as $crate::handle::Handle>::ID => {
                        let handle_ptr = handle_ptr as <$other as $crate::ExternC>::CType;
                        let _handle: $other = $crate::Decode::decode(handle_ptr, &mut ())
                            .ok_or($crate::FfiReturn::TrapRepresentation)?;
                    } )+
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    _ => return Err($crate::FfiReturn::UnknownHandle),
                }

                Ok(())
            })
        }
    };
    ( dealloc ) => {
        /// FFI function equivalent of [`alloc::alloc::dealloc`]
        ///
        /// # Safety
        ///
        /// See [`GlobalAlloc::dealloc`]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn __co3_dealloc(ptr: *mut u8, size: usize, align: usize) -> $crate::FfiReturn {
            if ptr.is_null() {
                return $crate::FfiReturn::TrapRepresentation;
            }

            if let Ok(layout) = core::alloc::Layout::from_size_align(size, align) {
                unsafe {
                    alloc::dealloc(ptr, layout);
                }

                return $crate::FfiReturn::Ok;
            }

            $crate::FfiReturn::TrapRepresentation
        }
    };
}

/// Generate declarations of FFI functions for he requested trait methods (e.g. Clone, Eq, Ord)
#[macro_export]
macro_rules! decl_fns {
    ( dealloc ) => {
        unsafe extern "C" {
            /// FFI function equivalent of [`alloc::alloc::dealloc`]
            ///
            /// # Safety
            ///
            /// See [`GlobalAlloc::dealloc`]
            pub(crate) fn __co3_dealloc(ptr: *mut u8, size: usize, align: usize) -> $crate::FfiReturn;
        }
    };
    ( $($fn_names:ident),+ ) => {
        $crate::decl_fns!{ link_prefix = "__co3_" $( $fn_names ),+ }
    };
    ( link_prefix = $prefix:literal $($fn_names:ident),+ ) => {
        pub(crate) mod __co3_import {
            $( $crate::decl_fns!{ @decl: $prefix $fn_names } )+
        }
    };
    ( @decl: $prefix:literal Clone ) => {
        unsafe extern "C" {
            /// FFI function equivalent of [`Clone::clone`]
            ///
            /// # Safety
            ///
            /// All of the given pointers must be valid and the given handle id must match the expected
            /// pointer type
            #[link_name = concat!($prefix, "clone")]
            pub(crate) fn clone(
                handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
                handle_ptr: *const $crate::external::Extern,
                out_ptr: *mut *mut $crate::external::Extern
            ) -> $crate::FfiReturn;
        }
    };
    ( @decl: $prefix:literal Default ) => {
        unsafe extern "C" {
            /// FFI function equivalent of [`Default::default`]
            ///
            /// # Safety
            ///
            /// All of the given pointers must be valid and the given handle id must match the expected
            /// pointer type
            #[link_name = concat!($prefix, "default")]
            pub(crate) fn default(
                handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
                out_ptr: *mut *mut $crate::external::Extern
            ) -> $crate::FfiReturn;
        }
    };
    ( @decl: $prefix:literal Eq ) => {
        unsafe extern "C" {
            /// FFI function equivalent of [`Eq::eq`]
            ///
            /// # Safety
            ///
            /// All of the given pointers must be valid and the given handle id must match the expected
            /// pointer type
            #[link_name = concat!($prefix, "eq")]
            pub(crate) fn eq(
                handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
                left_handle_ptr: *const $crate::external::Extern,
                right_handle_ptr: *const $crate::external::Extern,
                out_ptr: *mut u8,
            ) -> $crate::FfiReturn;
        }
    };
    ( @decl: $prefix:literal Ord ) => {
        unsafe extern "C" {
            /// FFI function equivalent of [`Ord::ord`]
            ///
            /// # Safety
            ///
            /// All of the given pointers must be valid and the given handle id must match the expected
            /// pointer type
            #[link_name = concat!($prefix, "ord")]
            pub(crate) fn ord(
                handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
                left_handle_ptr: *const $crate::external::Extern,
                right_handle_ptr: *const $crate::external::Extern,
                out_ptr: *mut i8,
            ) -> $crate::FfiReturn;
        }
    };
    ( @decl: $prefix:literal Drop ) => {
        unsafe extern "C" {
            /// FFI function equivalent of [`Drop::drop`]
            ///
            /// # Safety
            ///
            /// All of the given pointers must be valid and the given handle id must match the expected
            /// pointer type
            #[link_name = concat!($prefix, "drop")]
            pub(crate) fn drop(
                handle_id: <$crate::handle::Id as $crate::ExternC>::CType,
                handle_ptr: *mut $crate::external::Extern,
            ) -> $crate::FfiReturn;
        }
    };
}
