use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, LitStr, Path, visit_mut::VisitMut};

use crate::{
    impl_visitor::{Arg, FnDescriptor, path_symbol_name},
    utils::{gen_resolve_type, gen_store_name},
};

fn prune_fn_declaration_attributes<'a>(attrs: &[&'a syn::Attribute]) -> Vec<&'a syn::Attribute> {
    let mut pruned = Vec::new();

    for attr in attrs {
        if **attr == syn::parse_quote! {#[inline]} {
            continue;
        }
        if attr
            .path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "decarbonate")
        {
            continue;
        }
        if attr.path().is_ident("link_name") {
            continue;
        }

        pruned.push(*attr);
    }

    pruned
}

pub fn gen_declaration(
    impl_generics: &syn::Generics,
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    import_crate_name: Option<&TokenStream>,
    import_fn_name: Option<&LitStr>,
) -> TokenStream {
    let trait_name = trait_path.and_then(|path| path.segments.last().map(|seg| &seg.ident));
    let trait_symbol_name = trait_path.map(path_symbol_name);
    let ffi_fn_attrs = prune_fn_declaration_attributes(&fn_descriptor.attrs);

    let ffi_fn_name = gen_fn_name(fn_descriptor, trait_symbol_name.as_deref());
    let ffi_fn_doc = gen_doc(fn_descriptor, trait_name);
    let fn_signature = gen_fn_signature(&ffi_fn_name, fn_descriptor, impl_generics);
    let link_name = gen_link_name_attr(
        fn_descriptor,
        trait_symbol_name.as_deref(),
        import_crate_name,
        import_fn_name,
    )
    .unwrap_or_else(|| {
        if fn_descriptor.self_ty.is_none() && trait_name.is_none() {
            let fn_name = &fn_descriptor.sig.ident;
            quote! { #[link_name = stringify!(#fn_name)] }
        } else {
            quote! {}
        }
    });

    quote! {
        unsafe extern "C" {
            #[doc = #ffi_fn_doc]
            #link_name
            #(#ffi_fn_attrs)*
            #fn_signature;
        }
    }
}

pub fn gen_definition(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    impl_generics: &syn::Generics,
) -> TokenStream {
    let trait_name = trait_path.and_then(|path| path.segments.last().map(|seg| &seg.ident));
    let trait_symbol_name = trait_path.map(path_symbol_name);
    let ffi_fn_attrs = &fn_descriptor.attrs;

    let ffi_fn_name = gen_fn_name(fn_descriptor, trait_symbol_name.as_deref());
    let ffi_fn_doc = gen_doc(fn_descriptor, trait_name);
    let fn_signature = gen_fn_signature(&ffi_fn_name, fn_descriptor, impl_generics);
    let export_name = gen_export_name_attr(fn_descriptor, trait_symbol_name.as_deref())
        .unwrap_or_else(|| quote! { #[unsafe(no_mangle)] });

    let ffi_fn_body = gen_body(fn_descriptor, trait_path);

    quote! {
        #(#ffi_fn_attrs)*
        #[doc = #ffi_fn_doc]
        #export_name
        unsafe extern "C" #fn_signature {
            let fn_ = || {
                let fn_body = || #ffi_fn_body;

                if let Err(err) = fn_body() {
                    return err;
                }

                co3::FfiReturn::Ok
            };

            match std::panic::catch_unwind(fn_) {
                Ok(res) => res,
                Err(_) => {
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    co3::FfiReturn::UnrecoverableError
                },
            }
        }
    }
}

fn gen_link_name_attr(
    fn_descriptor: &FnDescriptor,
    trait_symbol_name: Option<&str>,
    import_crate_name: Option<&TokenStream>,
    import_fn_name: Option<&LitStr>,
) -> Option<TokenStream> {
    if fn_descriptor.self_ty.is_none() && trait_symbol_name.is_none() {
        let fn_name = import_fn_name.cloned().unwrap_or_else(|| {
            LitStr::new(
                &fn_descriptor.sig.ident.to_string(),
                fn_descriptor.sig.ident.span(),
            )
        });

        return match import_crate_name {
            Some(crate_name) => Some(quote!(#[link_name = concat!(#crate_name, #fn_name)])),
            None if import_fn_name.is_some() => Some(quote!(#[link_name = #fn_name])),
            None => None,
        };
    }

    if let Some(crate_name) = import_crate_name {
        return gen_non_shared_symbol_name(
            fn_descriptor,
            trait_symbol_name,
            crate_name,
            import_fn_name,
        )
        .map(|symbol| quote!(#[link_name = #symbol]));
    }

    import_fn_name.map(|symbol| quote!(#[link_name = #symbol]))
}

fn gen_export_name_attr(
    fn_descriptor: &FnDescriptor,
    trait_symbol_name: Option<&str>,
) -> Option<TokenStream> {
    gen_non_shared_symbol_name(
        fn_descriptor,
        trait_symbol_name,
        &quote!(concat!(env!("CARGO_CRATE_NAME"), "_")),
        None,
    )
    .map(|symbol| quote!(#[unsafe(export_name = #symbol)]))
}

fn gen_non_shared_symbol_name(
    fn_descriptor: &FnDescriptor,
    trait_symbol_name: Option<&str>,
    crate_name: &TokenStream,
    fn_name_override: Option<&LitStr>,
) -> Option<TokenStream> {
    let method_name = fn_name_override.cloned().unwrap_or_else(|| {
        LitStr::new(
            &fn_descriptor.sig.ident.to_string(),
            fn_descriptor.sig.ident.span(),
        )
    });
    if fn_descriptor.self_ty.is_none() && trait_symbol_name.is_none() {
        return Some(quote! {
            concat!(
                #crate_name,
                #method_name
            )
        });
    }

    let self_ty_name = LitStr::new(
        &fn_descriptor.self_ty_symbol_name().unwrap_or_default(),
        fn_descriptor.sig.ident.span(),
    );
    if let Some(trait_name) = trait_symbol_name {
        let trait_name = LitStr::new(trait_name, fn_descriptor.sig.ident.span());
        Some(quote! {
            concat!(
                #crate_name,
                #trait_name,
                "_",
                #self_ty_name,
                "_",
                #method_name
            )
        })
    } else {
        Some(quote! {
            concat!(
                #crate_name,
                #self_ty_name,
                "_",
                #method_name
            )
        })
    }
}

pub fn gen_fn_name(fn_descriptor: &FnDescriptor, trait_symbol_name: Option<&str>) -> Ident {
    let method_name = format!("_{}", &fn_descriptor.sig.ident);
    let self_ty_name = fn_descriptor.self_ty_symbol_name().unwrap_or_default();
    let trait_name =
        trait_symbol_name.map_or_else(Default::default, |trait_name| format!("_{trait_name}"));

    Ident::new(
        &format!("{trait_name}{self_ty_name}{method_name}"),
        proc_macro2::Span::call_site(),
    )
}

fn gen_doc(fn_descriptor: &FnDescriptor, trait_name: Option<&Ident>) -> String {
    let method_name = &fn_descriptor.sig.ident;

    let self_type = fn_descriptor
        .self_ty
        .as_ref()
        .and_then(syn::Path::get_ident);

    let path = self_type.map_or_else(
        || method_name.to_string(),
        |self_ty| {
            trait_name.map_or_else(
                || format!("{self_ty}::{method_name}"),
                // NOTE: Fully-qualified syntax currently not supported
                |trait_| format!("{trait_}::{method_name}"),
            )
        },
    );

    format!(
        " FFI function equivalent of [`{path}`]\n \
          \n \
          # Safety\n \
          \n \
          All of the given pointers must be valid"
    )
}

fn gen_fn_signature(
    ffi_fn_name: &Ident,
    fn_descriptor: &FnDescriptor,
    impl_generics: &syn::Generics,
) -> TokenStream {
    let self_arg = fn_descriptor
        .receiver
        .as_ref()
        .map(gen_input_arg)
        .map_or_else(Vec::new, |self_arg| vec![self_arg]);
    let fn_args: Vec<_> = fn_descriptor.input_args.iter().map(gen_input_arg).collect();
    let output_arg = ffi_output_arg(fn_descriptor).map(gen_out_ptr_arg);

    let mut generics = impl_generics.clone();
    let fn_generics = &fn_descriptor.sig.generics;
    generics.params.extend(fn_generics.params.clone());
    if let Some(fn_where_clause) = &fn_generics.where_clause {
        generics
            .make_where_clause()
            .predicates
            .extend(fn_where_clause.predicates.clone());
    }

    let (impl_generics, _, where_clause) = generics.split_for_impl();

    quote! {
        fn #ffi_fn_name #impl_generics (#(#self_arg,)* #(#fn_args,)* #output_arg) -> co3::FfiReturn #where_clause
    }
}

fn gen_input_arg(arg: &Arg) -> TokenStream {
    let arg_name = arg.name();
    let arg_type = arg.ffi_type_resolved();

    quote! { #arg_name: #arg_type }
}

fn gen_out_ptr_arg(arg: &Arg) -> TokenStream {
    let (arg_name, arg_type) = (arg.name(), arg.src_type_resolved());
    quote! { #arg_name: *mut <#arg_type as co3::out_ptr::OutPtr>::OutPtr }
}

fn gen_body(fn_descriptor: &FnDescriptor, trait_path: Option<&Path>) -> TokenStream {
    let input_conversions = gen_input_conversion_stmts(fn_descriptor);
    let method_call_stmt = gen_method_call_stmt(fn_descriptor, trait_path);
    let output_assignment = gen_output_assignment_stmts(fn_descriptor);
    let store_sync_stmts = gen_store_sync_stmts(fn_descriptor);

    quote! {{
        #input_conversions
        #method_call_stmt
        #output_assignment
        #store_sync_stmts

        Ok(())
    }}
}

fn gen_store_sync_stmts(fn_descriptor: &FnDescriptor) -> TokenStream {
    let mut stmts = quote! {};

    for arg in &fn_descriptor.input_args {
        let store_name = gen_store_name(arg.name());

        stmts.extend(quote! {
            co3::Store::sync(#store_name).ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
    }

    stmts
}

fn gen_input_conversion_stmts(fn_descriptor: &FnDescriptor) -> TokenStream {
    let mut stmts = quote! {};

    if let Some(arg) = &fn_descriptor.receiver {
        stmts = gen_arg_ffi_to_src(arg);
    }

    for arg in &fn_descriptor.input_args {
        stmts.extend(gen_arg_ffi_to_src(arg));
    }

    stmts
}

pub fn gen_arg_ffi_to_src(arg: &Arg) -> TokenStream {
    let (arg_name, src_type) = (arg.name(), arg.src_type_resolved());
    let store_name = gen_store_name(arg_name);

    quote! {
        let mut #store_name = Default::default();
        let #arg_name: #src_type = unsafe { co3::Decode::decode(#arg_name, &mut #store_name) }
            .ok_or(co3::FfiReturn::TrapRepresentation)?;
    }
}

pub struct InjectColon;
impl VisitMut for InjectColon {
    fn visit_angle_bracketed_generic_arguments_mut(
        &mut self,
        i: &mut syn::AngleBracketedGenericArguments,
    ) {
        i.colon2_token = Some(syn::parse_quote!(::));
    }
}

fn gen_method_call_stmt(fn_descriptor: &FnDescriptor, trait_path: Option<&Path>) -> TokenStream {
    let ident = &fn_descriptor.sig.ident;
    let self_type = &fn_descriptor.self_ty;

    let receiver = fn_descriptor.receiver.as_ref();
    let self_arg_name = receiver.map_or_else(Vec::new, |arg| vec![arg.name().clone()]);

    let fn_arg_names = fn_descriptor.input_args.iter().map(Arg::name);
    let self_ty = self_type.clone().map_or_else(
        || quote!(),
        |mut self_ty| {
            let mut inject_colon = InjectColon;
            inject_colon.visit_path_mut(&mut self_ty);

            trait_path.as_ref().map_or_else(
                || quote! {#self_ty::},
                |trait_| quote! {<#self_ty as #trait_>::},
            )
        },
    );
    let method_call = quote! {#self_ty #ident(#(#self_arg_name,)* #(#fn_arg_names),*)};

    fn_descriptor.output_arg.as_ref().map_or_else(
        || quote! {#method_call;},
        |output_arg| {
            let output_arg_name = &output_arg.name();

            if output_arg.src_type_is_empty_tuple() {
                return quote! { let #output_arg_name = #method_call; };
            }

            quote! {
                let __out_ptr = #output_arg_name;
                let #output_arg_name = #method_call;
            }
        },
    )
}

fn gen_output_assignment_stmts(fn_descriptor: &FnDescriptor) -> TokenStream {
    fn_descriptor.output_arg.as_ref().map_or_else(
        || quote! {},
        |out_arg| {
            let (arg_name, arg_type) = (out_arg.name(), out_arg.src_type_resolved());
            let resolve_impl_trait = gen_resolve_type(out_arg);

            if out_arg.src_type_is_empty_tuple() {
                return quote! { #resolve_impl_trait };
            }

            quote! {
                #resolve_impl_trait
                <#arg_type as co3::out_ptr::OutPtrWrite>::write_out(#arg_name, __out_ptr);
            }
        },
    )
}

fn ffi_output_arg<'ast>(fn_descriptor: &'ast FnDescriptor<'ast>) -> Option<&'ast Arg> {
    fn_descriptor.output_arg.as_ref().and_then(|output_arg| {
        if output_arg.src_type_is_empty_tuple() {
            return None;
        }

        if let Some(receiver) = &fn_descriptor.receiver
            && receiver.name() == output_arg.name()
        {
            return None;
        }

        Some(output_arg)
    })
}
