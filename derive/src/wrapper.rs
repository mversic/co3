use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    dispatch::{HandleId, gen_handle_erase_stmts, handle_id, is_handle_id_arg},
    ffi_fn::{self, boxed_array_ty, item_fn_input_ident, ownership_mode_for_arg},
    generate::OwnershipMode,
    is_link_name_attr,
    utils::{gen_normalization_stmts, gen_store_name, is_type_erased, unwrap_result_type},
};

fn strip_internal_arg_attrs(signature: &mut syn::Signature) {
    struct InternalAttrStripper;

    impl VisitMut for InternalAttrStripper {
        fn visit_receiver_mut(&mut self, node: &mut syn::Receiver) {
            node.attrs.retain(|attr| !attr.path().is_ident("by_val"));
        }

        fn visit_pat_type_mut(&mut self, node: &mut syn::PatType) {
            node.attrs.retain(|attr| !attr.path().is_ident("by_val"));
        }
    }

    InternalAttrStripper.visit_signature_mut(signature);
}

pub fn wrap_fn_definition(
    abi: &syn::Abi,
    block_attrs: &[syn::Attribute],
    mut item: syn::ItemFn,
) -> TokenStream {
    let vis = &item.vis;

    let wrapper_attrs = item
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("link_name"));

    let mut wrapper_sig = item.sig.clone();
    strip_internal_arg_attrs(&mut wrapper_sig);

    let wrapper_body = gen_wrapper_body(None, None, None, &item.sig);
    ffi_fn::normalize_fn_signature(&mut item.sig, None);
    let decl = ffi_fn::gen_extern_fn_signature(item.sig);
    let extern_fn_decl = gen_extern_decl(abi, block_attrs, &item.attrs, decl);

    quote! {
        #(#wrapper_attrs)*
        #vis #wrapper_sig {
            #extern_fn_decl
            #wrapper_body
        }
    }
}

pub fn wrap_impl_definition(impl_: &ItemImpl, self_id: Option<&syn::Type>) -> ItemImpl {
    let ItemImpl {
        attrs: impl_attrs,
        defaultness,
        unsafety,
        generics,
        trait_,
        self_ty,
        items,
        ..
    } = impl_;

    let trait_ = trait_.as_ref().map(|(_, path, _)| path);
    let methods = items.iter().map(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return quote!(#item);
        };

        let mut sig = item.sig.clone();
        let vis = &item.vis;

        let wrapper_attrs = item
            .attrs
            .iter()
            .filter(|attr| !attr.path().is_ident("link_name"));

        let self_binding = sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)))
            .then(|| quote! { let __co3_self = self; });

        let id_assignments = sig
            .inputs
            .iter()
            .filter_map(|input| {
                let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                    return None;
                };

                let handle_ty = match handle_id(ty)? {
                    HandleId::DynType(ty_param) => quote!(#ty_param),
                    HandleId::DynSelf => quote!(#self_ty),
                };

                Some(quote! { let #pat = <#handle_ty as co3::handle::Handle>::ID; })
            })
            .collect::<Vec<_>>();

        let wrapper_body = gen_wrapper_body(Some(generics), self_id, Some(self_ty), &sig);

        sig.inputs = sig
            .inputs
            .into_iter()
            .filter_map(|input| (!is_handle_id_arg(&input)).then_some(input))
            .collect();

        strip_internal_arg_attrs(&mut sig);

        quote! {
            #(#wrapper_attrs)*
            #vis #sig {
                #(#id_assignments)*
                #self_binding
                #wrapper_body
            }
        }
    });

    let mut generics = generics.clone();
    strip_internal_generic_attrs(&mut generics);
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let impl_head = if let Some(trait_) = &trait_ {
        quote!(impl #impl_generics #trait_ for #self_ty #where_clause)
    } else {
        quote!(impl #impl_generics #self_ty #where_clause)
    };

    syn::parse_quote! {
        #(#impl_attrs)*
        #defaultness #unsafety #impl_head {
            #(#methods)*
        }
    }
}

pub(crate) fn gen_extern_decl(
    abi: &syn::Abi,
    block_attrs: &[syn::Attribute],
    attrs: &[syn::Attribute],
    decl: TokenStream,
) -> TokenStream {
    let decl_attrs = attrs.iter().filter(|attr| is_link_name_attr(attr));

    quote! {
        unsafe #abi {
            #(#block_attrs)*
            #(#decl_attrs)*
            #decl;
        }
    }
}

fn gen_wrapper_body(
    generics: Option<&syn::Generics>,
    self_id: Option<&syn::Type>,
    self_ty: Option<&syn::Type>,
    sig: &syn::Signature,
) -> TokenStream {
    let handle_erase_stmts = generics
        .zip(self_ty)
        .map(|(generics, self_ty)| gen_handle_erase_stmts(generics, self_id, self_ty, sig))
        .unwrap_or_default();

    let input_convert = gen_input_conversion_stmts(&sig.inputs);
    let output_init = gen_output_init_stmt(&sig.output);
    let ffi_fn_call_stmt = gen_ffi_fn_call_stmt(sig);
    let store_sync_stmts = gen_store_sync_stmts(&sig.inputs);

    let sync_success_args: Vec<_> = (0..sig.inputs.len())
        .map(|idx| {
            let idx = syn::Index::from(idx);
            quote! { u8::from(!__co3_sync_errors[#idx]) }
        })
        .collect();

    let sync_success_fmt = if sig.inputs.is_empty() {
        "\n".to_owned()
    } else {
        let placeholders = core::iter::repeat_n("{}", sig.inputs.len())
            .collect::<Vec<_>>()
            .join(", ");

        format!("\nArg Sync: ({placeholders})\n")
    };

    let body = if let syn::ReturnType::Type(_, output_ty) = &sig.output {
        let fmt = format!("{sync_success_fmt}Out Read: {{}}\n");
        let return_ = if unwrap_result_type(output_ty).is_some() {
            quote!(Ok(__co3_out))
        } else {
            quote!(__co3_out)
        };

        let out_res = quote!(u8::from(core::option::Option::is_some(&__co3_out)));
        let panic_sync_error = if sync_success_args.is_empty() {
            quote! { panic!(#fmt, #out_res); }
        } else {
            quote! { panic!(#fmt, #(#sync_success_args),*, #out_res); }
        };

        quote! {
            let __co3_sync_errors = #store_sync_stmts;

            let __co3_out = unsafe { core::mem::MaybeUninit::assume_init(__co3_out) };
            let __co3_out = unsafe { co3::out_ptr::OutPtrRead::try_read_out(__co3_out) };

            let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
            if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity)
                || core::option::Option::is_none(&__co3_out)
            {
                #panic_sync_error
            }

            let __co3_out = unsafe { core::option::Option::unwrap_unchecked(__co3_out) };

            #return_
        }
    } else {
        let panic_sync_error = if sync_success_args.is_empty() {
            quote! { panic!(#sync_success_fmt); }
        } else {
            quote! { panic!(#sync_success_fmt, #(#sync_success_args),*); }
        };

        quote! {
            let __co3_sync_errors = #store_sync_stmts;

            let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
            if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity) {
                #panic_sync_error
            }
        }
    };

    quote! {
        #input_convert
        #output_init

        {
            #(#handle_erase_stmts)*
            #ffi_fn_call_stmt
        }

        #body
    }
}

fn gen_store_sync_stmts(inputs: &Punctuated<FnArg, syn::Token![,]>) -> TokenStream {
    let input_len = inputs.len();
    let mut stmts = quote! {};

    for (idx, input) in inputs.iter().enumerate() {
        let arg_name = match input {
            FnArg::Typed(arg) => item_fn_input_ident(&arg.pat).clone(),
            FnArg::Receiver(_) => format_ident!("__co3_self"),
        };

        let store_name = gen_store_name(&arg_name);
        stmts.extend(quote! {
            if co3::Store::sync(#store_name).is_none() {
                __co3_sync_errors[#idx] = true;
            }
        });
    }

    quote! {{
        let mut __co3_sync_errors = [false; #input_len];
        #stmts
        __co3_sync_errors
    }}
}

fn gen_input_conversion_stmts(inputs: &Punctuated<FnArg, syn::Token![,]>) -> TokenStream {
    let mut stmts = quote! {};

    for input in inputs {
        let (attrs, arg_name, arg_ty) = match input {
            FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => {
                (attrs, item_fn_input_ident(pat).clone(), (**ty).clone())
            }
            FnArg::Receiver(receiver) => (
                &receiver.attrs,
                format_ident!("__co3_self"),
                (*receiver.ty).clone(),
            ),
        };

        let resolve_ty = gen_normalization_stmts(&arg_name, &arg_ty);
        let store_name = gen_store_name(&arg_name);

        stmts.extend(match ownership_mode_for_arg(attrs, &arg_ty) {
            OwnershipMode::Borrow if matches!(arg_ty, syn::Type::Array(_)) => {
                quote! { #resolve_ty let #arg_name = &#arg_name; }
            }
            OwnershipMode::Borrow => {
                let borrow_store_name = format_ident!("__co3_{arg_name}_borrow_store");

                quote! {
                    #resolve_ty
                    let mut #borrow_store_name = Default::default();
                    let #arg_name = co3::borrow::Borrow::borrow(#arg_name, &mut #borrow_store_name);
                }
            }
            OwnershipMode::ByValue if let Some(boxed_ty) = boxed_array_ty(&arg_ty) => {
                quote! { #resolve_ty let #arg_name: #boxed_ty = Box::new(#arg_name); }
            }
            OwnershipMode::ByValue => quote! { #resolve_ty },
        });

        stmts.extend(quote! {
            let mut #store_name = Default::default();
            let #arg_name = co3::EncodeWithStore::encode(#arg_name, &mut #store_name);
        });
    }

    stmts
}

fn gen_output_init_stmt(output: &syn::ReturnType) -> TokenStream {
    let syn::ReturnType::Type(_, output) = output else {
        return quote! {};
    };

    let output = unwrap_result_type(output).map_or(&**output, |(ok, _)| ok);
    let output_ty = quote! {
        core::mem::MaybeUninit<<#output as co3::ExternC>::CType>
    };

    quote! {
        let mut __co3_out: #output_ty = core::mem::MaybeUninit::uninit();
        let __co3_out_ptr = core::mem::MaybeUninit::as_mut_ptr(&mut __co3_out);
    }
}

fn gen_ffi_fn_call_stmt(sig: &syn::Signature) -> TokenStream {
    let mut arg_names: Vec<TokenStream> = Vec::new();

    let fn_name = &sig.ident;
    for input in &sig.inputs {
        arg_names.push(if let FnArg::Typed(syn::PatType { pat, .. }) = input {
            let arg_name = item_fn_input_ident(pat);
            quote!(#arg_name)
        } else {
            quote!(__co3_self)
        });
    }

    if matches!(sig.output, syn::ReturnType::Type(_, _)) {
        arg_names.push(quote!(__co3_out_ptr));
    }

    let execution_fail_arm = if let syn::ReturnType::Type(_, output_ty) = &sig.output {
        if unwrap_result_type(output_ty).is_some() {
            quote! {
                co3::FfiReturn::ExecutionFail => {
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    unimplemented!("Error handling is not properly implemented yet");
                }
            }
        } else {
            quote! {}
        }
    } else {
        quote! {}
    };

    quote! {
        let __co3_return: co3::FfiReturn = unsafe { #fn_name(#(#arg_names),*) };

        match __co3_return {
            co3::FfiReturn::Ok => {},
            #execution_fail_arm
            _ => panic!(concat!(stringify!(#fn_name), " returned {}"), __co3_return)
        }
    }
}

pub(crate) fn strip_internal_generic_attrs(generics: &mut syn::Generics) {
    for param in &mut generics.params {
        if let syn::GenericParam::Type(param) = param {
            param.attrs.retain(|attr| !is_type_erased(attr));
        }
    }
}
