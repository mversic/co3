// Exactly one framework must be selected for each comparison.

#[cfg(not(any(
    feature = "abi_stable",
    feature = "stabby",
    feature = "co3",
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
)))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "co3", feature = "uniffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "co3", feature = "diplomat"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "co3", feature = "interoptopus"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "co3", feature = "safer_ffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "co3", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "uniffi", feature = "diplomat"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "uniffi", feature = "interoptopus"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "uniffi", feature = "safer_ffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "uniffi", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "diplomat", feature = "interoptopus"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "diplomat", feature = "safer_ffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "diplomat", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "interoptopus", feature = "safer_ffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "interoptopus", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");
#[cfg(all(feature = "safer_ffi", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "boltffi")]
boltffi::scaffolding!();

#[cfg(all(feature = "abi_stable", feature = "co3"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "abi_stable", feature = "uniffi"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "abi_stable", feature = "diplomat"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "abi_stable", feature = "interoptopus"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "abi_stable", feature = "safer_ffi"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "abi_stable", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "abi_stable", feature = "stabby"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "stabby", feature = "co3"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "stabby", feature = "uniffi"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "stabby", feature = "diplomat"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "stabby", feature = "interoptopus"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "stabby", feature = "safer_ffi"))]
compile_error!("Select exactly one framework feature.");

#[cfg(all(feature = "stabby", feature = "boltffi"))]
compile_error!("Select exactly one framework feature.");

// sabi_extern_fn handles the calling convention and panic boundary only.
// A StableAbi-derived function-pointer field also checks the exchanged types,
// as in an abi_stable module interface. Keep the original type spelling here.
#[cfg(feature = "abi_stable")]
macro_rules! abi_stable_signature {
    ($name:ident: ($($arg:ty),* $(,)?) -> $ret:ty) => {
        const _: () = {
            #[repr(C)]
            #[derive(abi_stable::StableAbi)]
            struct Interface {
                function: extern "C" fn($($arg),*) -> $ret,
            }
            const _: Interface = Interface { function: $name };
        };
    };
}

// The comparison runner supplies a destination when auditing a safer-ffi export.
#[cfg(all(test, feature = "safer_ffi"))]
#[test]
fn comparison_c_header() {
    let mut header = Vec::new();
    safer_ffi::headers::builder()
        .to_writer(&mut header)
        .generate()
        .expect("generate C header");
    if let Some(path) = std::env::var_os("CO3_COMPARISON_HEADER") {
        std::fs::write(path, header).expect("write C header");
    }
}
