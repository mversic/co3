// Run the binding generator's type-analysis stage on the same source as the Rust build.
use diplomat_core::{
    ast::SpanLocation,
    hir::{BasicAttributeValidator, TypeContext},
};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let path = env::args_os().nth(1).expect("expected a fixture path");
    let source = fs::read_to_string(path).expect("failed to read fixture");
    let file = syn::parse_file(&source).expect("failed to parse fixture");

    let mut backend = BasicAttributeValidator::new("comparison");
    backend.support.free_functions = true;
    backend.support.mutable_slices = true;
    backend.support.memory_sharing = true;
    backend.support.traits = true;
    backend.support.callbacks = true;
    backend.support.option = true;
    backend.support.struct_refs = true;
    backend.support.mut_struct_refs = true;
    backend.support.abi_compatibles = true;
    match TypeContext::from_syn(
        &file,
        Default::default(),
        backend,
        None,
        &SpanLocation::None,
    ) {
        Ok(context) => {
            assert!(
                context.all_free_functions().next().is_some()
                    || context.all_types().any(|(_, ty)| !ty.methods().is_empty()),
                "no Diplomat function or method found"
            );
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}
