// Return the backing address without reading the shared cell.
include!("../support/mod.rs");

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn storage(input: &core::cell::UnsafeCell<u32>) -> u64 {
    input.get() as usize as u64
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn storage(input: &core::cell::UnsafeCell<u32>) -> u64 {
        input.get() as usize as u64
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn storage(input: &core::cell::UnsafeCell<u32>) -> u64;
}

#[cfg(feature = "co3")]
unsafe extern "C" {
    #[link_name = "shared_cell__storage"]
    fn exported_storage(cell: *mut u32) -> u64;
}

// Exercise the reverse direction through CO3's generated import wrapper.
#[cfg(feature = "co3")]
#[unsafe(export_name = "comparison_cell_store")]
unsafe extern "C" fn store_raw(cell: *mut u32, value: u32) -> u64 {
    unsafe { cell.write(value) };
    cell as usize as u64
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(extern("C"))]
    #[symbol_name = "comparison_cell_store"]
    unsafe fn store(cell: &core::cell::UnsafeCell<u32>, value: u32) -> u64;
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(storage: (&core::cell::UnsafeCell<u32>) -> u64);

fn main() {
    #[cfg(feature = "co3")]
    {
        let cell = core::cell::UnsafeCell::new(3u32);
        let alias = &cell;
        let address = unsafe { exported_storage(cell.get()) };
        assert_eq!(address, alias.get() as usize as u64);
        // The foreign call has sole access to the contents during this write.
        assert_eq!(unsafe { store(&cell, 7) }, address);
        assert_eq!(unsafe { exported_storage(alias.get()) }, address);
        assert_eq!(cell.into_inner(), 7);
    }
}
