// Exactly one framework must be selected for each comparison.

#[cfg(not(any(
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
