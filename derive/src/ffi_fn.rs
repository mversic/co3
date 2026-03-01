use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Ident, LitStr, Path, Type, visit_mut::VisitMut};

use crate::{
    impl_visitor::{Arg, FnDescriptor, path_symbol_name},
    is_unsafe_no_mangle_attr, parse_unsafe_export_name_attr,
    utils::{gen_resolve_type, gen_store_name},
    wrapper::HandleIdSpec,
};

fn has_link_name_attr(attrs: &[&syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("link_name"))
}

fn gen_extern_block_attrs(attrs: &[&syn::Attribute]) -> Vec<TokenStream> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("link"))
        .map(|attr| {
            let meta = &attr.meta;
            quote!(#![#meta])
        })
        .collect()
}

fn prune_fn_definition_attributes<'a>(attrs: &[&'a syn::Attribute]) -> Vec<&'a syn::Attribute> {
    let _ = attrs;
    Vec::new()
}

#[cfg(feature = "getset")]
pub fn gen_declaration(
    impl_generics: &syn::Generics,
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    import_crate_name: Option<&TokenStream>,
    import_fn_name: Option<&LitStr>,
) -> TokenStream {
    let trait_name = trait_path.and_then(|path| path.segments.last().map(|seg| &seg.ident));
    let trait_symbol_name = trait_path.map(path_symbol_name);
    let has_explicit_link_name = has_link_name_attr(&fn_descriptor.attrs);
    let extern_block_attrs = gen_extern_block_attrs(&fn_descriptor.attrs);

    let ffi_fn_name = gen_fn_name(fn_descriptor, trait_symbol_name.as_deref());
    let ffi_fn_doc = gen_doc(fn_descriptor, trait_name);
    let fn_signature =
        gen_fn_signature(&ffi_fn_name, fn_descriptor, impl_generics, trait_path, &[]);
    let link_name = gen_link_name_attr(
        fn_descriptor,
        trait_symbol_name.as_deref(),
        import_crate_name,
        import_fn_name,
    )
    .unwrap_or_else(|| {
        if fn_descriptor.self_ty.is_none() && trait_name.is_none() && !has_explicit_link_name {
            let fn_name = &fn_descriptor.sig.ident;
            quote! { #[link_name = stringify!(#fn_name)] }
        } else {
            quote! {}
        }
    });
    quote! {
        unsafe extern "C" {
            #(#extern_block_attrs)*
            #[doc = #ffi_fn_doc]
            #link_name
            #fn_signature;
        }
    }
}

pub fn gen_inline_declaration(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    import_crate_name: Option<&TokenStream>,
    import_fn_name: Option<&LitStr>,
    ffi_fn_name: &Ident,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let trait_name = trait_path.and_then(|path| path.segments.last().map(|seg| &seg.ident));
    let trait_symbol_name = trait_path.map(path_symbol_name);
    let has_explicit_link_name = has_link_name_attr(&fn_descriptor.attrs);
    let extern_block_attrs = gen_extern_block_attrs(&fn_descriptor.attrs);

    let ffi_fn_doc = gen_doc(fn_descriptor, trait_name);
    let fn_signature = gen_fn_signature(
        ffi_fn_name,
        fn_descriptor,
        &Default::default(),
        trait_path,
        handle_id_specs,
    );
    let link_name = gen_link_name_attr(
        fn_descriptor,
        trait_symbol_name.as_deref(),
        import_crate_name,
        import_fn_name,
    )
    .unwrap_or_else(|| {
        if fn_descriptor.self_ty.is_none() && trait_name.is_none() && !has_explicit_link_name {
            let fn_name = &fn_descriptor.sig.ident;
            quote! { #[link_name = stringify!(#fn_name)] }
        } else {
            quote! {}
        }
    });
    quote! {
        unsafe extern "C" {
            #(#extern_block_attrs)*
            #[doc = #ffi_fn_doc]
            #link_name
            #fn_signature;
        }
    }
}

pub fn gen_inline_passthrough_declaration(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    import_crate_name: Option<&TokenStream>,
    import_fn_name: Option<&LitStr>,
    import_abi: &syn::Abi,
    ffi_fn_name: &Ident,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let trait_name = trait_path.and_then(|path| path.segments.last().map(|seg| &seg.ident));
    let trait_symbol_name = trait_path.map(path_symbol_name);
    let has_explicit_link_name = has_link_name_attr(&fn_descriptor.attrs);
    let extern_block_attrs = gen_extern_block_attrs(&fn_descriptor.attrs);

    let ffi_fn_doc = gen_doc(fn_descriptor, trait_name);
    let receiver = fn_descriptor.receiver.as_ref().map(|arg| {
        let arg_name = arg.name();
        let arg_type = declared_input_src_type(arg, fn_descriptor, trait_path);
        quote!(#arg_name: #arg_type)
    });
    let fn_args = fn_descriptor.input_args.iter().map(|arg| {
        let arg_name = arg.name();
        let arg_type = declared_input_src_type(arg, fn_descriptor, trait_path);
        quote!(#arg_name: #arg_type)
    });
    let mut args: Vec<_> = receiver.into_iter().chain(fn_args).collect();
    inject_handle_id_decl_args(handle_id_specs, &mut args);
    let output = fn_descriptor
        .output_arg
        .as_ref()
        .filter(|arg| !arg.src_type_is_empty_tuple())
        .map(|arg| {
            let ty = resolved_src_type(arg, fn_descriptor, trait_path);
            quote!(-> #ty)
        })
        .unwrap_or_default();
    let link_name = gen_link_name_attr(
        fn_descriptor,
        trait_symbol_name.as_deref(),
        import_crate_name,
        import_fn_name,
    )
    .unwrap_or_else(|| {
        if fn_descriptor.self_ty.is_none() && trait_name.is_none() && !has_explicit_link_name {
            let fn_name = &fn_descriptor.sig.ident;
            quote! { #[link_name = stringify!(#fn_name)] }
        } else {
            quote! {}
        }
    });

    let generics = &fn_descriptor.sig.generics;
    let where_clause = &generics.where_clause;

    quote! {
        unsafe #import_abi {
            #(#extern_block_attrs)*
            #[doc = #ffi_fn_doc]
            #link_name
            fn #ffi_fn_name #generics (
                #(#args),*
            ) #output #where_clause;
        }
    }
}

pub fn gen_definition(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    impl_generics: &syn::Generics,
    export_abi: Option<&syn::Abi>,
) -> TokenStream {
    let trait_name = trait_path.and_then(|path| path.segments.last().map(|seg| &seg.ident));
    let trait_symbol_name = trait_path.map(path_symbol_name);
    let ffi_fn_attrs = prune_fn_definition_attributes(&fn_descriptor.attrs);

    let ffi_fn_name = gen_definition_fn_name(fn_descriptor);
    let ffi_fn_doc = gen_doc(fn_descriptor, trait_name);

    let export_name = gen_export_name_attr(fn_descriptor, trait_symbol_name.as_deref());
    let ffi_abi = export_abi
        .cloned()
        .or_else(|| fn_descriptor.sig.abi.clone())
        .unwrap_or_else(|| syn::parse_quote!(extern "Rust"));
    let use_passthrough_shim = export_abi
        .zip(fn_descriptor.sig.abi.as_ref())
        .is_some_and(|(override_abi, outer_abi)| override_abi != outer_abi);

    if use_passthrough_shim {
        let fn_signature = gen_passthrough_signature(
            &ffi_fn_name,
            fn_descriptor,
            impl_generics,
            trait_path,
            &ffi_abi,
        );
        let fn_body = gen_passthrough_body(fn_descriptor, trait_path);
        return quote! {
            #(#ffi_fn_attrs)*
            #[doc = #ffi_fn_doc]
            #export_name
            #fn_signature {
                #fn_body
            }
        };
    }

    let ffi_fn_body = gen_body(fn_descriptor, trait_path);
    let fn_signature =
        gen_fn_signature(&ffi_fn_name, fn_descriptor, impl_generics, trait_path, &[]);
    let is_exported_drop = trait_path.is_some_and(|path| path_symbol_name(path) == "Drop")
        && fn_descriptor.sig.ident == "drop";
    if is_exported_drop {
        let Some(receiver) = fn_descriptor.receiver.as_ref() else {
            return syn::Error::new_spanned(
                &fn_descriptor.sig,
                "Drop export requires a receiver argument",
            )
            .to_compile_error();
        };
        let Some(self_ty) = fn_descriptor.self_ty.as_ref() else {
            return syn::Error::new_spanned(
                &fn_descriptor.sig,
                "Drop export requires a concrete self type",
            )
            .to_compile_error();
        };
        let receiver_name = receiver.name();
        return quote! {
            #(#ffi_fn_attrs)*
            #[doc = #ffi_fn_doc]
            #export_name
            unsafe #ffi_abi #fn_signature {
                let fn_ = || {
                    let fn_body = || -> Result<(), co3::FfiReturn> {
                        let __self: &mut #self_ty = unsafe {
                            co3::Decode::decode(#receiver_name, &mut ())
                        }.ok_or(co3::FfiReturn::TrapRepresentation)?;

                        let __self_ptr: *mut #self_ty = __self as *mut #self_ty;
                        unsafe { core::mem::drop(Box::from_raw(__self_ptr)); }

                        Ok(())
                    };

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
        };
    }

    quote! {
        #(#ffi_fn_attrs)*
        #[doc = #ffi_fn_doc]
        #export_name
        unsafe #ffi_abi #fn_signature {
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

fn gen_passthrough_signature(
    ffi_fn_name: &Ident,
    fn_descriptor: &FnDescriptor,
    impl_generics: &syn::Generics,
    trait_path: Option<&Path>,
    export_abi: &syn::Abi,
) -> TokenStream {
    let asyncness = &fn_descriptor.sig.asyncness;
    let where_clause = &impl_generics.where_clause;

    let receiver = fn_descriptor.receiver.as_ref().map(|arg| {
        let arg_name = arg.name();
        let arg_type = resolved_src_type(arg, fn_descriptor, trait_path);
        quote! { #arg_name: #arg_type }
    });
    let fn_args = fn_descriptor.input_args.iter().map(|arg| {
        let arg_name = arg.name();
        let arg_type = resolved_src_type(arg, fn_descriptor, trait_path);
        quote! { #arg_name: #arg_type }
    });
    let output = fn_descriptor
        .output_arg
        .as_ref()
        .map(|arg| {
            let ty = resolved_src_type(arg, fn_descriptor, trait_path);
            quote!(-> #ty)
        })
        .unwrap_or_default();

    quote! {
        #asyncness unsafe #export_abi fn #ffi_fn_name #impl_generics (
            #receiver #(#fn_args),*
        ) #output #where_clause
    }
}

fn gen_passthrough_body(fn_descriptor: &FnDescriptor, trait_path: Option<&Path>) -> TokenStream {
    let fn_name = &fn_descriptor.sig.ident;
    let mut args: Vec<Ident> = Vec::new();
    if let Some(receiver) = &fn_descriptor.receiver {
        args.push(receiver.name().clone());
    }
    args.extend(
        fn_descriptor
            .input_args
            .iter()
            .map(|arg| arg.name().clone()),
    );

    let call = match (fn_descriptor.self_ty.as_ref(), trait_path) {
        (Some(self_ty), Some(trait_path)) => {
            quote!(<#self_ty as #trait_path>::#fn_name(#(#args),*))
        }
        (Some(self_ty), None) => quote!(<#self_ty>::#fn_name(#(#args),*)),
        (None, None) => quote!(#fn_name(#(#args),*)),
        (None, Some(_)) => unreachable!("trait path without self type"),
    };

    if fn_descriptor.output_arg.is_some() {
        quote!(#call)
    } else {
        quote! {
            #call;
        }
    }
}

pub fn gen_default_export_name_attr(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> Option<syn::Attribute> {
    let trait_symbol_name = trait_path.map(path_symbol_name);
    gen_non_shared_symbol_name(
        fn_descriptor,
        trait_symbol_name.as_deref(),
        &quote!(concat!(env!("CARGO_CRATE_NAME"), "_")),
        None,
    )
    .map(|symbol| syn::parse_quote!(#[unsafe(export_name = #symbol)]))
}

fn gen_definition_fn_name(fn_descriptor: &FnDescriptor) -> Ident {
    Ident::new(
        &format!("_{}", fn_descriptor.sig.ident),
        proc_macro2::Span::call_site(),
    )
}

#[cfg(feature = "getset")]
fn gen_fn_name(fn_descriptor: &FnDescriptor, _trait_symbol_name: Option<&str>) -> Ident {
    gen_definition_fn_name(fn_descriptor)
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
    if let Some(name) = fn_descriptor
        .attrs
        .iter()
        .find_map(|attr| parse_unsafe_export_name_attr(attr))
    {
        return Some(quote!(#[unsafe(export_name = #name)]));
    }
    if fn_descriptor
        .attrs
        .iter()
        .any(|attr| is_unsafe_no_mangle_attr(attr))
    {
        let name = LitStr::new(
            &fn_descriptor.sig.ident.to_string(),
            fn_descriptor.sig.ident.span(),
        );
        return Some(quote!(#[unsafe(export_name = #name)]));
    }
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
    trait_path: Option<&Path>,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let self_arg = fn_descriptor
        .receiver
        .as_ref()
        .map(|arg| gen_input_arg(arg, fn_descriptor, trait_path))
        .map_or_else(Vec::new, |self_arg| vec![self_arg]);
    let fn_args: Vec<_> = fn_descriptor
        .input_args
        .iter()
        .map(|arg| gen_input_arg(arg, fn_descriptor, trait_path))
        .collect();
    let mut input_args = self_arg;
    input_args.extend(fn_args);
    inject_handle_id_decl_args(handle_id_specs, &mut input_args);
    let output_arg =
        ffi_output_arg(fn_descriptor).map(|arg| gen_out_ptr_arg(arg, fn_descriptor, trait_path));

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
        fn #ffi_fn_name #impl_generics (#(#input_args,)* #output_arg) -> co3::FfiReturn #where_clause
    }
}

fn gen_input_arg(
    arg: &Arg,
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> TokenStream {
    let arg_name = arg.name();
    let src_type = declared_input_src_type(arg, fn_descriptor, trait_path);
    let arg_type: Type = if arg.is_handle() {
        if let Type::Reference(reference) = arg.src_type() {
            if reference.mutability.is_some() {
                syn::parse_quote!(*mut co3::external::Extern)
            } else {
                syn::parse_quote!(*const co3::external::Extern)
            }
        } else {
            syn::parse_quote!(*mut co3::external::Extern)
        }
    } else {
        syn::parse_quote!(<#src_type as co3::ExternC>::CType)
    };

    quote! { #arg_name: #arg_type }
}

fn declared_input_src_type(
    arg: &Arg,
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> Type {
    resolved_src_type(arg, fn_descriptor, trait_path)
}

fn inject_handle_id_decl_args(handle_id_specs: &[HandleIdSpec], args: &mut Vec<TokenStream>) {
    fn selector_base_name(selector: &Type) -> String {
        if let Type::Path(type_path) = selector {
            if type_path.qself.is_none() && type_path.path.is_ident("Self") {
                return "self".to_string();
            }
            if let Some(seg) = type_path.path.segments.last() {
                return seg.ident.to_string().to_lowercase();
            }
        }
        "handle".to_string()
    }

    let mut inserts: Vec<(usize, usize, TokenStream)> = handle_id_specs
        .iter()
        .enumerate()
        .map(|(order, spec)| {
            let base = selector_base_name(&spec.selector);
            let arg_name = Ident::new(
                &format!("__{base}_handle_id"),
                proc_macro2::Span::call_site(),
            );
            (
                spec.at,
                order,
                quote!(#arg_name: <co3::handle::Id as co3::ExternC>::CType),
            )
        })
        .collect();
    inserts.sort_by(|(a_at, a_order, _), (b_at, b_order, _)| {
        a_at.cmp(b_at).then(a_order.cmp(b_order))
    });

    if inserts.is_empty() {
        return;
    }

    let real_args = core::mem::take(args);
    let final_len = real_args.len() + inserts.len();
    let mut slots: Vec<Option<TokenStream>> = vec![None; final_len];

    for (at, _order, decl) in inserts {
        let desired = core::cmp::min(at, final_len.saturating_sub(1));
        let mut pos = desired;
        while pos < final_len && slots[pos].is_some() {
            pos += 1;
        }
        if pos == final_len {
            pos = 0;
            while pos < desired && slots[pos].is_some() {
                pos += 1;
            }
        }
        if pos < final_len {
            slots[pos] = Some(decl);
        }
    }

    let mut real_iter = real_args.into_iter();
    for slot in &mut slots {
        if slot.is_none() {
            *slot = real_iter.next();
        }
    }
    *args = slots.into_iter().flatten().collect();
}

fn gen_out_ptr_arg(
    arg: &Arg,
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> TokenStream {
    let arg_name = arg.name();
    let arg_type = resolved_src_type(arg, fn_descriptor, trait_path);
    quote! { #arg_name: *mut <#arg_type as co3::out_ptr::OutPtr>::OutPtr }
}

fn gen_body(fn_descriptor: &FnDescriptor, trait_path: Option<&Path>) -> TokenStream {
    let input_conversions = gen_input_conversion_stmts(fn_descriptor, trait_path);
    let method_call_stmt = gen_method_call_stmt(fn_descriptor, trait_path);
    let output_assignment = gen_output_assignment_stmts(fn_descriptor, trait_path);
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

fn gen_input_conversion_stmts(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> TokenStream {
    let mut stmts = quote! {};

    if let Some(arg) = &fn_descriptor.receiver {
        stmts = gen_arg_ffi_to_src(arg, fn_descriptor, trait_path);
    }

    for arg in &fn_descriptor.input_args {
        stmts.extend(gen_arg_ffi_to_src(arg, fn_descriptor, trait_path));
    }

    stmts
}

pub fn gen_arg_ffi_to_src(
    arg: &Arg,
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> TokenStream {
    let arg_name = arg.name();
    let src_type = resolved_src_type(arg, fn_descriptor, trait_path);
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

fn gen_output_assignment_stmts(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
) -> TokenStream {
    fn_descriptor.output_arg.as_ref().map_or_else(
        || quote! {},
        |out_arg| {
            let arg_name = out_arg.name();
            let arg_type = resolved_src_type(out_arg, fn_descriptor, trait_path);
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

fn resolved_src_type(arg: &Arg, fn_descriptor: &FnDescriptor, trait_path: Option<&Path>) -> Type {
    let mut ty = arg.src_type_resolved();
    qualify_trait_associated_types(&mut ty, fn_descriptor.self_ty.as_ref(), trait_path);
    ty
}

fn qualify_trait_associated_types(
    ty: &mut Type,
    self_ty: Option<&Path>,
    trait_path: Option<&Path>,
) {
    struct Qualifier<'a> {
        self_ty: Option<&'a Path>,
        trait_path: Option<&'a Path>,
    }

    impl VisitMut for Qualifier<'_> {
        fn visit_type_path_mut(&mut self, i: &mut syn::TypePath) {
            syn::visit_mut::visit_type_path_mut(self, i);
            let (Some(self_ty), Some(trait_path)) = (self.self_ty, self.trait_path) else {
                return;
            };
            if i.qself.is_some() {
                return;
            }
            let self_len = self_ty.segments.len();
            if i.path.segments.len() <= self_len {
                return;
            }
            let mut prefix = syn::Path {
                leading_colon: None,
                segments: Default::default(),
            };
            for seg in i.path.segments.iter().take(self_len) {
                prefix.segments.push(seg.clone());
            }
            if prefix.to_token_stream().to_string() != self_ty.to_token_stream().to_string() {
                return;
            }

            let mut rest = syn::Path {
                leading_colon: None,
                segments: Default::default(),
            };
            for seg in i.path.segments.iter().skip(self_len) {
                rest.segments.push(seg.clone());
            }
            let qualified: Type = syn::parse_quote!(<#self_ty as #trait_path>::#rest);
            if let Type::Path(tp) = qualified {
                *i = tp;
            }
        }
    }

    Qualifier {
        self_ty,
        trait_path,
    }
    .visit_type_mut(ty);
}
