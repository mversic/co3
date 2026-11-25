use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
struct Sample {
    reading: u32,
    valid: bool,
}

fn sum_valid(samples: &[Sample]) -> u32 {
    samples.iter().filter(|s| s.valid).map(|s| s.reading).sum()
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "shared_samples"]
    fn sum_valid(#[soft] samples: &[Sample]) -> u32;
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "shared_samples_rust"]
    #![lang = "Rust"]
    fn sum_valid(#[soft] samples: &[Sample]) -> u32;
}

mod client {
    use super::Sample;

    co3::ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "shared_samples"]
        pub(super) fn sum_valid(#[soft] samples: &[Sample]) -> u32;
    }

    co3::ffi! {
        #![unsafe(extern("C"))]
        #![lang = "Rust"]
        #[symbol_name = "shared_samples_rust__sum_valid"]
        pub(super) fn sum_valid_rust(#[soft] samples: &[Sample]) -> u32;
    }
}

#[test]
fn shared_slice_of_non_clone_records_crosses_the_abi() {
    let samples = [
        Sample {
            reading: 7,
            valid: true,
        },
        Sample {
            reading: 9,
            valid: false,
        },
    ];
    assert_eq!(client::sum_valid(&samples), 7);
    assert_eq!(client::sum_valid_rust(&samples), 7);
}
