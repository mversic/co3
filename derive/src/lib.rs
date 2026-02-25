//! Crate containing FFI related macro functionality
use darling::FromDeriveInput;
use impl_visitor::{FnDescriptor, ImplDescriptor};
use manyhow::{emit, manyhow};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use wrapper::{ExternTypeLinkMode, ExternTypeSymbolOverride};

#[cfg(feature = "getset")]
use crate::attr_parse::derive::Derive;
use crate::{
    emitter::Emitter,
    extern_c::{FfiTypeData, FfiTypeInput, FfiTypeKindAttribute, derive_extern_c},
    impl_visitor::Arg,
};

mod attr_parse;
mod emitter;
mod extern_c;
mod ffi_fn;
#[cfg(feature = "getset")]
mod getset_gen;
mod impl_visitor;
mod utils;
mod wrapper;

const NO_EXPORT_ABI_MSG: &str = "specify ABI with `export(\"...\")`";

struct FfiItems(Vec<FfiTypeInput>);

impl syn::parse::Parse for FfiItems {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut items = Vec::new();

        while !input.is_empty() {
            let input = input.parse::<syn::DeriveInput>()?;
            let input = FfiTypeInput::from_derive_input(&input)?;

            items.push(input);
        }

        Ok(Self(items))
    }
}

/// A test utility function that parses multiple attributes
#[cfg(test)]
fn parse_attributes(ts: TokenStream) -> Vec<syn::Attribute> {
    struct Attributes(Vec<syn::Attribute>);
    impl syn::parse::Parse for Attributes {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            syn::Attribute::parse_outer(input).map(Attributes)
        }
    }

    syn::parse2::<Attributes>(ts).unwrap().0
}

/// Declare a shared trait contract that can be used by `co3::def_fns!` and
/// `#[extern_type]` derived wrappers.
///
/// Supported signatures:
/// - Receiver: `self`, `&self`, `&mut self`, or no receiver
/// - Inputs: any number of parameters
/// - Output: optional return type
#[manyhow]
#[proc_macro_attribute]
pub fn shared(args: TokenStream, input: TokenStream) -> TokenStream {
    #[derive(Clone)]
    enum SharedTy {
        SelfValue,
        SelfRef,
        SelfRefMut,
        Other(syn::Type),
    }

    impl SharedTy {
        fn from_type(ty: &syn::Type) -> Option<Self> {
            match ty {
                syn::Type::Path(path)
                    if path.qself.is_none()
                        && path.path.segments.len() == 1
                        && path.path.segments[0].ident == "Self" =>
                {
                    Some(Self::SelfValue)
                }
                syn::Type::Reference(reference) => {
                    let syn::Type::Path(path) = reference.elem.as_ref() else {
                        return None;
                    };
                    if path.qself.is_none()
                        && path.path.segments.len() == 1
                        && path.path.segments[0].ident == "Self"
                    {
                        if reference.mutability.is_some() {
                            Some(Self::SelfRefMut)
                        } else {
                            Some(Self::SelfRef)
                        }
                    } else {
                        None
                    }
                }
                _ => Some(Self::Other(ty.clone())),
            }
        }

        fn decl_ffi_ty(&self) -> TokenStream {
            match self {
                SharedTy::SelfValue | SharedTy::SelfRefMut => quote!(*mut co3::external::Extern),
                SharedTy::SelfRef => quote!(*const co3::external::Extern),
                SharedTy::Other(ty) => quote!(<#ty as co3::ExternC>::CType),
            }
        }

        fn concrete_ty(&self, other: &TokenStream) -> TokenStream {
            match self {
                SharedTy::SelfValue => quote!(#other),
                SharedTy::SelfRef => quote!(&#other),
                SharedTy::SelfRefMut => quote!(&mut #other),
                SharedTy::Other(ty) => quote!(#ty),
            }
        }

        fn decode_rhs(&self, ffi_ident: &syn::Ident, other: &TokenStream) -> TokenStream {
            match self {
                SharedTy::SelfValue => quote!(#ffi_ident as <#other as co3::ExternC>::CType),
                SharedTy::SelfRef => quote!(#ffi_ident as <&#other as co3::ExternC>::CType),
                SharedTy::SelfRefMut => {
                    quote!(#ffi_ident as <&mut #other as co3::ExternC>::CType)
                }
                SharedTy::Other(_) => quote!(#ffi_ident),
            }
        }
    }

    let mut emitter = Emitter::new();

    let item = match syn::parse2::<syn::ItemTrait>(input) {
        Ok(item) => item,
        Err(err) => return err.to_compile_error(),
    };

    if !args.is_empty() {
        emit!(emitter, args, "Unknown tokens in the attribute");
    }

    let trait_ident = &item.ident;
    let mut def_items = Vec::new();
    let mut impl_items = Vec::new();
    let mut link_resolve_arms = Vec::new();

    for trait_item in &item.items {
        let syn::TraitItem::Fn(method) = trait_item else {
            continue;
        };

        if !method.sig.generics.params.is_empty() {
            emit!(
                emitter,
                method.sig.generics,
                "Generic shared methods are not supported"
            );
            continue;
        }

        let method_ident = &method.sig.ident;
        let rust_fn_ident = format_ident!(
            "__co3_shared_{}_{}",
            trait_ident.to_string().to_lowercase(),
            method_ident
        );

        let receiver = method.sig.receiver().cloned();
        let receiver_ty = receiver.as_ref().map(|receiver| {
            if receiver.reference.is_none() {
                SharedTy::SelfValue
            } else if receiver.mutability.is_some() {
                SharedTy::SelfRefMut
            } else {
                SharedTy::SelfRef
            }
        });

        let mut arg_specs: Vec<(syn::Ident, SharedTy)> = Vec::new();
        for input in &method.sig.inputs {
            let syn::FnArg::Typed(arg) = input else {
                continue;
            };

            let syn::Pat::Ident(pat_ident) = arg.pat.as_ref() else {
                emit!(
                    emitter,
                    arg.pat,
                    "Shared method arguments must use identifier patterns"
                );
                continue;
            };

            let Some(arg_ty) = SharedTy::from_type(arg.ty.as_ref()) else {
                emit!(
                    emitter,
                    arg.ty,
                    "Unsupported `Self` usage in shared method argument type"
                );
                continue;
            };
            arg_specs.push((pat_ident.ident.clone(), arg_ty));
        }

        let output_ty = match &method.sig.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => {
                let Some(kind) = SharedTy::from_type(ty.as_ref()) else {
                    emit!(
                        emitter,
                        ty,
                        "Unsupported `Self` usage in shared method return type"
                    );
                    continue;
                };
                Some(kind)
            }
        };
        let method_sig = &method.sig;

        let decl_receiver = receiver_ty.as_ref().map(SharedTy::decl_ffi_ty);
        let decl_args: Vec<_> = arg_specs
            .iter()
            .map(|(name, ty)| {
                let ty = ty.decl_ffi_ty();
                quote!(#name: #ty)
            })
            .collect();
        let decl_out_ptr = output_ty.as_ref().map(|out| match out {
            SharedTy::SelfValue | SharedTy::SelfRefMut => {
                quote!(out_ptr: *mut *mut co3::external::Extern)
            }
            SharedTy::SelfRef => quote!(out_ptr: *mut *const co3::external::Extern),
            SharedTy::Other(ty) => quote!(out_ptr: *mut <#ty as co3::out_ptr::OutPtr>::OutPtr),
        });
        let mut decl_params = vec![quote!(handle_id: <co3::handle::Id as co3::ExternC>::CType)];
        if let Some(receiver_ty) = decl_receiver {
            decl_params.push(quote!(receiver: #receiver_ty));
        }
        decl_params.extend(decl_args);
        if let Some(out_ptr) = decl_out_ptr {
            decl_params.push(out_ptr);
        }

        let mut def_decl_inputs: Vec<TokenStream> = Vec::new();
        if let Some(receiver_ffi_ty) = receiver_ty.as_ref().map(SharedTy::decl_ffi_ty) {
            def_decl_inputs.push(quote!(receiver: #receiver_ffi_ty));
        }
        for (name, ty) in &arg_specs {
            let ty = ty.decl_ffi_ty();
            def_decl_inputs.push(quote!(#name: #ty));
        }
        if output_ty.is_some() {
            def_decl_inputs.push(quote!(out_ptr: *mut core::ffi::c_void));
        }

        let mut decode_stmts: Vec<TokenStream> = Vec::new();
        let mut sync_stmts: Vec<TokenStream> = Vec::new();
        let mut call_args: Vec<TokenStream> = Vec::new();

        if let Some(receiver_ty) = &receiver_ty {
            let receiver_store = format_ident!("receiver_store");
            let receiver_rust_ty = receiver_ty.concrete_ty(&quote!($other));
            let receiver_decode =
                receiver_ty.decode_rhs(&format_ident!("receiver"), &quote!($other));
            decode_stmts.push(quote! {
                let mut #receiver_store = Default::default();
                let receiver: #receiver_rust_ty = co3::Decode::decode(#receiver_decode, &mut #receiver_store)
                    .ok_or(co3::FfiReturn::TrapRepresentation)?;
            });
            call_args.push(quote!(receiver));
            sync_stmts.push(quote! {
                co3::Store::sync(#receiver_store).ok_or(co3::FfiReturn::TrapRepresentation)?;
            });
        }

        for (name, ty) in &arg_specs {
            let store = format_ident!("{name}_store");
            let rust_ty = ty.concrete_ty(&quote!($other));
            let decode_rhs = ty.decode_rhs(name, &quote!($other));
            decode_stmts.push(quote! {
                let mut #store = Default::default();
                let #name: #rust_ty = co3::Decode::decode(#decode_rhs, &mut #store)
                    .ok_or(co3::FfiReturn::TrapRepresentation)?;
            });
            call_args.push(quote!(#name));
            sync_stmts.push(quote! {
                co3::Store::sync(#store).ok_or(co3::FfiReturn::TrapRepresentation)?;
            });
        }

        let method_call = quote!(<$other as #trait_ident>::#method_ident(#(#call_args),*));
        let output_write = output_ty
            .as_ref()
            .map(|out_ty| {
                let out_rust_ty = out_ty.concrete_ty(&quote!($other));
                quote! {
                    let output: #out_rust_ty = #method_call;
                    <#out_rust_ty as co3::out_ptr::OutPtrWrite>::write_out(
                        output,
                        out_ptr.cast::<<#out_rust_ty as co3::out_ptr::OutPtr>::OutPtr>(),
                    );
                }
            })
            .unwrap_or_else(|| quote! { #method_call; });

        let mut impl_stmts: Vec<TokenStream> = Vec::new();
        let mut call_args: Vec<TokenStream> = Vec::new();
        impl_stmts.push(quote! {
            let handle_id = <Self as co3::handle::Handle>::ID;
        });
        call_args.push(quote!(co3::Encode::encode(handle_id, &mut ())));

        if let Some(receiver_ty) = &receiver_ty {
            match receiver_ty {
                SharedTy::SelfValue => {
                    impl_stmts.push(quote! {
                        let mut receiver = core::mem::ManuallyDrop::new(self);
                        let receiver = co3::external::External::as_mut_ptr(&mut *receiver);
                    });
                    call_args.push(quote!(receiver));
                }
                SharedTy::SelfRef => {
                    impl_stmts.push(quote! {
                        let receiver = co3::external::External::as_ptr(self);
                    });
                    call_args.push(quote!(receiver));
                }
                SharedTy::SelfRefMut => {
                    impl_stmts.push(quote! {
                        let receiver = co3::external::External::as_mut_ptr(self);
                    });
                    call_args.push(quote!(receiver));
                }
                SharedTy::Other(_) => unreachable!(),
            }
        }

        for (name, ty) in &arg_specs {
            let store = format_ident!("{name}_store");
            impl_stmts.push(quote! { let mut #store = Default::default(); });
            match ty {
                SharedTy::SelfRef => {
                    impl_stmts.push(quote! {
                        let #name = co3::external::External::as_ptr(#name);
                    });
                }
                SharedTy::SelfRefMut => {
                    impl_stmts.push(quote! {
                        let #name = co3::external::External::as_mut_ptr(#name);
                    });
                }
                SharedTy::SelfValue | SharedTy::Other(_) => {
                    impl_stmts.push(quote! {
                        let #name = co3::Encode::encode(#name, &mut #store);
                    });
                }
            }
            call_args.push(quote!(#name));
            impl_stmts.push(quote! {
                if co3::Store::sync(#store).is_none() {
                    panic!("failed to sync store for {}", stringify!(#name));
                }
            });
        }

        let return_stmt = if output_ty.is_some() {
            call_args.push(quote!(output.as_mut_ptr()));
            quote! {
                unsafe {
                    co3::out_ptr::OutPtrRead::try_read_out(output.assume_init())
                        .expect("Invalid output")
                }
            }
        } else {
            quote! { () }
        };
        let output_buffer = if let syn::ReturnType::Type(_, ty) = &method.sig.output {
            quote! { let mut output = core::mem::MaybeUninit::<<#ty as co3::out_ptr::OutPtr>::OutPtr>::uninit(); }
        } else {
            quote! {}
        };
        impl_items.push(quote! {
            #method_sig {
                #(#impl_stmts)*
                #output_buffer

                unsafe extern "C" {
                    #[link_name = #trait_ident!(@resolve_link [ $($prefix)+ ] #method_ident [ $($links)* ])]
                    fn #rust_fn_ident(#(#decl_params),*) -> co3::FfiReturn;
                }

                let ffi_return = unsafe { #rust_fn_ident(#(#call_args),*) };
                match ffi_return {
                    co3::FfiReturn::Ok => {},
                    _ => panic!(concat!(stringify!(#method_ident), " returned {}"), ffi_return),
                }

                #return_stmt
            }
        });

        link_resolve_arms.push(quote! {
            ( @resolve_link [ $($prefix:tt)+ ] #method_ident [ #trait_ident :: #method_ident = $link_name:literal $(, $($rest:tt)*)? ] ) => {
                $link_name
            };
            ( @resolve_link [ $($prefix:tt)+ ] #method_ident [ $other_trait:ident :: $other_method:ident = $other_link:literal $(, $($rest:tt)*)? ] ) => {
                #trait_ident!(@resolve_link [ $($prefix)+ ] #method_ident [ $($($rest)*)? ])
            };
            ( @resolve_link [ $($prefix:tt)+ ] #method_ident [ ] ) => {
                concat!($($prefix)+, stringify!(#trait_ident), "_", stringify!(#method_ident))
            };
        });

        def_items.push(quote! {
            #[unsafe(export_name = concat!(env!("CARGO_CRATE_NAME"), "_", stringify!(#trait_ident), "_", stringify!(#method_ident)))]
            unsafe extern "C" fn #rust_fn_ident(
                handle_id: <co3::handle::Id as co3::ExternC>::CType,
                #(#def_decl_inputs),*
            ) -> co3::FfiReturn {
                co3::def_fns!(@catch_unwind {
                    match co3::Decode::decode(handle_id, &mut ()).ok_or(co3::FfiReturn::TrapRepresentation)? {
                        $( <$other as co3::handle::Handle>::ID => {
                            #(#decode_stmts)*
                            #output_write
                            #(#sync_stmts)*
                        } )+
                        _ => return Err(co3::FfiReturn::UnknownHandle),
                    }

                    Ok(())
                })
            }
        });
    }

    let output = quote! {
        #item

        #[doc(hidden)]
        macro_rules! #trait_ident {
            ( @def: $( $other:ty ),+ $(,)? ) => {
                #(#def_items)*
            };
            ( @impl [ $($prefix:tt)+ ] ) => {
                #trait_ident!(@impl [ $($prefix)+ ] [ ])
            };
            ( @impl [ $($prefix:tt)+ ] [ $($links:tt)* ] ) => {
                #(#impl_items)*
            };
            #(#link_resolve_arms)*
        }
    };

    emitter.finish_token_stream_with(output)
}

#[derive(Clone, Default)]
struct CarbonateArgs {
    export_abi: Option<syn::Abi>,
}

fn merge_export_args(
    emitter: &mut Emitter,
    into: &mut CarbonateArgs,
    attr: &syn::Attribute,
    parsed: CarbonateArgs,
) {
    if let Some(export_abi) = parsed.export_abi {
        if into.export_abi.is_some() {
            emit!(
                emitter,
                attr,
                "`\"...\"` ABI can only be provided once across export attributes on the same item"
            );
        } else {
            into.export_abi = Some(export_abi);
        }
    }
}

fn is_export_attr(attr: &syn::Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "export")
}

fn is_export_skip_attr(attr: &syn::Attribute) -> bool {
    if !is_export_attr(attr) {
        return false;
    }

    attr.parse_args::<syn::Ident>()
        .is_ok_and(|arg| arg == "skip")
}

fn is_export_name_attr(attr: &syn::Attribute) -> bool {
    if !is_export_attr(attr) {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let Ok(metas) = meta_list.parse_args_with(
        syn::punctuated::Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };

    if metas.len() != 1 {
        return false;
    }

    let nv = &metas[0];
    if !nv.path.is_ident("name") {
        return false;
    }

    matches!(
        nv.value,
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(_),
            ..
        })
    )
}

fn validate_method_export_attrs(emitter: &mut Emitter, attrs: &[&syn::Attribute]) {
    for attr in attrs {
        if is_export_attr(attr) && !is_export_skip_attr(attr) && !is_export_name_attr(attr) {
            emit!(
                emitter,
                attr,
                "method-level `#[export(...)]` is not supported, except `#[export(skip)]`; specify ABI on the impl block"
            );
        }
    }
}

fn has_valid_export_skip(emitter: &mut Emitter, attrs: &[&syn::Attribute]) -> bool {
    let has_skip = attrs.iter().any(|attr| is_export_skip_attr(attr));
    if !has_skip {
        return false;
    }

    if let Some(conflicting_attr) = attrs.iter().find(|attr| is_unsafe_no_mangle_attr(attr)) {
        emit!(
            emitter,
            conflicting_attr,
            "`#[no_mangle]` attribute may not be used in combination with `#[export(skip)]`"
        );
    }
    if let Some(conflicting_attr) = attrs.iter().find(|attr| is_unsafe_export_name_attr(attr)) {
        emit!(
            emitter,
            conflicting_attr,
            "`#[export_name]` attribute may not be used in combination with `#[export(skip)]`"
        );
    }

    true
}

fn is_rust_abi(abi: Option<&syn::Abi>) -> bool {
    match abi {
        None => true,
        Some(abi) => abi.name.as_ref().is_some_and(|name| name.value() == "Rust"),
    }
}

impl syn::parse::Parse for CarbonateArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error(NO_EXPORT_ABI_MSG));
        }

        let abi_name: syn::LitStr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error(NO_EXPORT_ABI_MSG));
        }

        let export_abi = syn::parse2::<syn::Abi>(quote!(extern #abi_name))
            .map_err(|_| syn::Error::new_spanned(abi_name, "invalid ABI"))?;

        Ok(Self {
            export_abi: Some(export_abi),
        })
    }
}

#[derive(Default)]
struct ParsedLinkAttr {
    link_prefix: Option<syn::LitStr>,
    link_name: Option<syn::LitStr>,
}

fn parse_link_attr(attr: &syn::Attribute) -> Result<Option<ParsedLinkAttr>, syn::Error> {
    if !attr.path().is_ident("link") {
        return Ok(None);
    }

    let syn::Meta::List(list) = &attr.meta else {
        return Err(syn::Error::new_spanned(
            attr,
            "expected `#[link(name = \"...\")]` or `#[link(crate = \"...\")]`",
        ));
    };

    let metas = list.parse_args_with(
        syn::punctuated::Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated,
    )?;

    let mut out = ParsedLinkAttr::default();
    for nv in metas {
        let Some(ident) = nv.path.get_ident() else {
            return Err(syn::Error::new_spanned(
                nv.path,
                "expected `name = \"...\"` or `crate = \"...\"`",
            ));
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = nv.value
        else {
            return Err(syn::Error::new_spanned(
                nv,
                "expected string literal in `#[link(...)]`",
            ));
        };

        if ident == "name" {
            if out.link_name.replace(value).is_some() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`name` can only be provided once in `#[link(...)]`",
                ));
            }
        } else if ident == "crate" {
            if out.link_prefix.replace(value).is_some() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`crate` can only be provided once in `#[link(...)]`",
                ));
            }
        } else {
            return Err(syn::Error::new_spanned(
                ident,
                "expected `name = \"...\"` or `crate = \"...\"`",
            ));
        }
    }

    Ok(Some(out))
}

fn parse_link_name_attr(attr: &syn::Attribute) -> Result<Option<syn::LitStr>, syn::Error> {
    if !attr.path().is_ident("link_name") {
        return Ok(None);
    }

    let syn::Meta::NameValue(nv) = &attr.meta else {
        return Err(syn::Error::new_spanned(
            attr,
            "expected `#[link_name = \"...\"]`",
        ));
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(link),
        ..
    }) = &nv.value
    else {
        return Err(syn::Error::new_spanned(
            &nv.value,
            "expected string literal in `#[link_name = \"...\"]`",
        ));
    };

    Ok(Some(link.clone()))
}

fn parse_unsafe_export_name_attr(attr: &syn::Attribute) -> Option<syn::LitStr> {
    if attr.path().is_ident("export") {
        let syn::Meta::List(meta_list) = &attr.meta else {
            return None;
        };

        let metas = meta_list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .ok()?;

        for meta in metas {
            let syn::Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident("name") {
                continue;
            }
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(name),
                ..
            }) = nv.value
            else {
                continue;
            };
            return Some(name);
        }

        return None;
    }

    if !attr.path().is_ident("unsafe") {
        return None;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return None;
    };

    let metas = meta_list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .ok()?;

    for meta in metas {
        let syn::Meta::NameValue(nv) = meta else {
            continue;
        };
        if !nv.path.is_ident("export_name") {
            continue;
        }
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(name),
            ..
        }) = nv.value
        else {
            continue;
        };
        return Some(name);
    }

    None
}

fn is_unsafe_export_name_attr(attr: &syn::Attribute) -> bool {
    parse_unsafe_export_name_attr(attr).is_some()
}

fn is_unsafe_no_mangle_attr(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("unsafe") {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let Ok(metas) = meta_list.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };

    metas.into_iter().any(|meta| match meta {
        syn::Meta::Path(path) => path.is_ident("no_mangle"),
        _ => false,
    })
}

fn is_consumed_export_attr(attr: &syn::Attribute) -> bool {
    is_unsafe_export_name_attr(attr) || is_unsafe_no_mangle_attr(attr)
}

fn strip_consumed_export_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !is_consumed_export_attr(attr));
}

fn strip_export_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| !is_export_attr(attr));
}

fn attach_export_attr_if_absent(
    attrs: &mut Vec<syn::Attribute>,
    export_attr: Option<syn::Attribute>,
) {
    if attrs.iter().any(is_consumed_export_attr) {
        return;
    }
    if let Some(export_attr) = export_attr {
        attrs.push(export_attr);
    }
}

enum ExternTypeAttrArgs {
    LinkCrate(syn::LitStr),
    Symbols(Vec<ExternTypeSymbolOverride>),
}

impl syn::parse::Parse for ExternTypeAttrArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error("expected at least one `Trait::method = \"symbol\"` entry"));
        }

        let link_prefix: Option<syn::LitStr> = None;
        let mut symbols = Vec::new();

        while !input.is_empty() {
            let key: syn::Path = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let value = input.parse::<syn::LitStr>()?;

            if key.is_ident("link_prefix") {
                return Err(syn::Error::new(
                    key.segments[0].ident.span(),
                    "provide explicit `Trait::method = \"symbol\"` mappings",
                ));
            } else {
                if key.segments.len() != 2 {
                    return Err(syn::Error::new(
                        key.segments[0].ident.span(),
                        "expected `Trait::method = \"symbol\"`",
                    ));
                }

                let trait_name = key.segments[0].ident.clone();
                let method_name = key.segments[1].ident.clone();
                symbols.push(ExternTypeSymbolOverride {
                    trait_name,
                    method_name,
                    symbol: value,
                });
            }

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else if !input.is_empty() {
                return Err(input.error("expected `,` between extern_type arguments"));
            }
        }

        match (link_prefix, symbols.is_empty()) {
            (Some(link_prefix), true) => Ok(Self::LinkCrate(link_prefix)),
            (Some(_), false) => {
                Err(input.error("cannot mix link mode with explicit symbol mappings"))
            }
            (None, false) => Ok(Self::Symbols(symbols)),
            (None, true) => {
                Err(input.error("expected at least one `Trait::method = \"symbol\"` mapping"))
            }
        }
    }
}

fn gen_export_link_prefix(link_prefix: syn::LitStr) -> TokenStream {
    let mut pref = link_prefix.value();
    if !pref.ends_with('_') {
        pref.push('_');
    }
    let link_prefix = syn::LitStr::new(&pref, link_prefix.span());
    quote!(#link_prefix)
}

/// Replace struct/enum/union definition with opaque pointer. This applies to types that
/// are converted to an opaque pointer when sent across FFI but does not affect any other
/// item wrapped with this macro (e.g. fieldless enums). This is so that most of the time
/// users can safely wrap all of their structs with this macro and not be concerned with the
/// cognitive load of figuring out which structs are converted to opaque pointers.
///
/// ## A note on `#[derive(...)]` limitations
///
/// This proc-macro crate parses the `#[derive(...)]` attributes.
/// Due to technical limitations of proc macros, it does not have access to the resolved path of the macro, only to what is written in the derive.
/// As such, it cannot support derives that are used through aliases, such as
///
/// ```ignore
/// use getset::Getters as GettersAlias;
/// #[derive(GettersAlias)]
/// pub struct Hello {
///     // ...
/// }
/// ```
///
/// It assumes that the derive is imported and referred to by its original name.
///
fn extern_type_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = match syn::parse2::<ExternTypeAttrArgs>(args) {
        Err(err) => return err.to_compile_error(),
        Ok(args) => args,
    };

    let link_mode = match args {
        ExternTypeAttrArgs::LinkCrate(link_prefix) => {
            ExternTypeLinkMode::LinkCrate(gen_export_link_prefix(link_prefix.clone()))
        }
        ExternTypeAttrArgs::Symbols(symbols) => ExternTypeLinkMode::ExplicitSymbols(symbols),
    };
    let items = match syn::parse2::<FfiItems>(input) {
        Err(err) => return err.to_compile_error(),
        Ok(items) => items.0,
    };

    let mut emitter = Emitter::new();
    let items = items
        .into_iter()
        .map(|item| {
            let kind = item.ffi_type_attr.kind.as_ref();
            let is_opaque = kind == Some(&FfiTypeKindAttribute::Opaque)
                || matches!((&item.data, kind), (FfiTypeData::Struct(_), None));

            if !is_opaque {
                let item = item.ast;

                return quote! {
                    #[derive(co3::ReprC)]
                    #item
                };
            }

            #[cfg(feature = "getset")]
            if let FfiTypeData::Struct(fields) = &item.data
                && item
                    .derive_attr
                    .derives
                    .iter()
                    .any(|d| matches!(d, Derive::GetSet(_)))
            {
                let ExternTypeLinkMode::LinkCrate(link_prefix) = &link_mode else {
                    unimplemented!();
                };
                let derived_methods: Vec<_> = getset_gen::gen_derived_methods(
                    &mut emitter,
                    &item.ident,
                    &item.derive_attr,
                    &item.getset_attr,
                    fields,
                )
                .collect();

                let ffi_fns: Vec<_> = derived_methods
                    .iter()
                    .map(|fn_| {
                        ffi_fn::gen_declaration(
                            &Default::default(),
                            fn_,
                            None,
                            Some(link_prefix),
                            None,
                        )
                    })
                    .collect();

                let impl_block = wrapper::wrap_impl_items(
                    &ImplDescriptor {
                        attrs: Vec::new(),
                        trait_name: None,
                        associated_types: Vec::new(),
                        associated_consts: Vec::new(),
                        generics: &Default::default(),
                        fns: derived_methods,
                    },
                    Some(link_prefix),
                );
                let opaque = wrapper::wrap_as_opaque(&mut emitter, item, &link_mode);

                return quote! {
                    #opaque

                    #impl_block
                    #(#ffi_fns)*
                };
            }

            wrapper::wrap_as_opaque(&mut emitter, item, &link_mode)
        })
        .collect::<Vec<_>>();

    emitter.finish_token_stream_with(quote! { #(#items)* })
}

/// Supported arguments:
/// - `#[extern_type(link(crate = "other"))]` sets shared import/export symbols to `"other_{Trait}_{method}"`.
/// - `#[extern_type(Trait::method = "symbol", ...)]` sets explicit symbols per shared method.
#[manyhow]
#[proc_macro_attribute]
pub fn extern_type(args: TokenStream, input: TokenStream) -> TokenStream {
    extern_type_impl(args, input)
}

// TODO: reprC(`local`) is a workaround for https://github.com/rust-lang/rust/issues/48214
// because some derived types cannot derive `NonLocal` othwerise. Should be removed in future
/// Derive implementations of traits required to convert to and from an FFI-compatible type
///
/// # Attributes
///
/// * `#[reprC(opaque)]`
/// serialize the type as opaque. If automatically derived type doesn't work just
/// attach this attribute and force the type to be serialized as opaque across FFI
///
/// * `#[reprC(NICHE_VALUE = <expr>, unsafe(is_valid = |target| ...))]`
/// customize [`co3::niche::Niche`] value and validation function for `#[repr(transparent)]` types.
/// `NICHE_VALUE` can be ommitted in which case the implementation delegates to the wrapped type.
///
/// # Safety
///
/// `is_valid` must not return false positives
///
/// Check [`co3::transmute::CheckedTransmute`] or [`co3::reprC`] for more details
///
/// * `#[reprC(local)]`
/// marks the type as local, meaning it contains references to the local frame. If a type
/// contains references to the local frame you won't be able to return it from an FFI function
/// because the frame is destroyed on function return which would invalidate your type's references.
///
/// Only applicable to data-carrying enums.
///
/// NOTE: This attribute is likely to be removed in future versions
///
/// * `#[reprC(unsafe(non_owning))]`
/// when a type contains a raw pointer (e.g. `*const T`/*mut T`) it's not possible to figure out
/// whether it carries ownership of the data pointed to. Place this attribute on the field to
/// indicate pointer doesn't own the data and is robust in the type. Alternatively, if the type
/// is carrying ownership mark entire type as opaque with `#[reprC(opaque)]`. If the type
/// is not carrying ownership, but is not robust convert it into an equivalent [`co3::ReprC`]
/// type that is validated when crossing the FFI boundary. It is also ok to mark non-owning,
/// non-robust type as opaque
///
/// # Safety
///
/// * wrapping type must allow for all possible values of the pointer including `null` (it's robust)
/// * the wrapping types's field of the pointer type must not carry ownership (it's non owning)
///
/// ```
/// use getset::Getters as GettersAlias;
///
/// #[derive(GettersAlias)]
/// pub struct Hello {}
/// ```
///
/// It assumes that the derive is imported and referred to by its original name.
#[manyhow]
#[proc_macro_derive(ReprC, attributes(reprC))]
pub fn extern_c_derive(input: TokenStream) -> TokenStream {
    let mut emitter = Emitter::new();

    let Some(item) = emitter.handle(syn::parse2::<syn::DeriveInput>(input)) else {
        return emitter.finish_token_stream();
    };

    let result = derive_extern_c(&mut emitter, &item);
    emitter.finish_token_stream_with(result)
}

/// Generate FFI functions
///
/// When placed on a structure, it integrates with [`getset`] to export derived getter/setter methods.
/// To be visible this attribute must be placed before/on top of any [`getset`] derive macro attributes
///
/// It also works on impl blocks (by visiting all methods in the impl block) and on enums and unions (as a no-op)
///
/// # Example:
/// ```rust
/// use std::alloc::alloc;
///
/// use co3::{ReprC, export};
/// use getset::Getters;
///
/// // For a struct such as:
/// #[export("C")]
/// #[derive(ReprC, Clone, Getters)]
/// #[getset(get = "pub")]
/// pub struct Foo {
///     /// Id of the struct
///     id: u8,
///     #[getset(skip)]
///     bar: Vec<u8>,
/// }
///
/// #[export("C")]
/// impl Foo {
///     /// Construct new type
///     pub extern "C" fn new(id: u8) -> Self {
///         Self {
///             id,
///             bar: Vec::new(),
///         }
///     }
///     /// Return bar
///     pub fn bar(&self) -> &[u8] {
///         &self.bar
///     }
/// }
///
/// /* The following functions will be derived:
/// unsafe extern "C" fn Foo__new(id: u8, output: *mut Foo) -> FfiReturn {
///     /* function implementation */
///     FfiReturn::Ok
/// }
/// unsafe extern "C" fn Foo__bar(handle: *const Foo, output: *mut CSlice<u8>) -> FfiReturn {
///     /* function implementation */
///     FfiReturn::Ok
/// }
/// unsafe extern "C" fn Foo__id(handle: *const Foo, output: *mut u8) -> FfiReturn {
///     /* function implementation */
///     FfiReturn::Ok
/// } */
/// ```
///
/// ## A note on `#[derive(...)]` limitations
///
/// This proc-macro crate parses the `#[derive(...)]` attributes.
/// Due to technical limitations of proc macros, it does not have access to the resolved path of the macro, only to what is written in the derive.
/// As such, it cannot support derives that are used through aliases, such as
///
/// ```ignore
/// use getset::Getters as GettersAlias;
/// #[derive(GettersAlias)]
/// pub struct Hello {
///     // ...
/// }
/// ```
///
/// It assumes that the derive is imported and referred to by its original name.
#[manyhow]
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut emitter = Emitter::new();
    let mut export_args = match syn::parse2::<CarbonateArgs>(attr) {
        Ok(args) => args,
        Err(err) => {
            let msg = err.to_string();
            emit!(emitter, err.span(), "{}", msg);
            CarbonateArgs::default()
        }
    };

    let item = match syn::parse2::<syn::Item>(item) {
        Err(err) => return err.to_compile_error(),
        Ok(item) => item,
    };

    match &item {
        syn::Item::Impl(item_impl) => {
            for attr in &item_impl.attrs {
                if is_export_skip_attr(attr) {
                    emit!(
                        emitter,
                        attr,
                        "`#[export(skip)]` is only supported on impl methods"
                    );
                    continue;
                }
                if !is_export_attr(attr) {
                    continue;
                }
                match attr.parse_args::<CarbonateArgs>() {
                    Ok(parsed) => merge_export_args(&mut emitter, &mut export_args, attr, parsed),
                    Err(err) => emit!(emitter, err.span(), "{}", err),
                }
            }
        }
        syn::Item::Fn(item_fn) => {
            for attr in &item_fn.attrs {
                if is_export_skip_attr(attr) {
                    emit!(
                        emitter,
                        attr,
                        "`#[export(skip)]` is only supported on impl methods"
                    );
                    continue;
                }
                if !is_export_attr(attr) {
                    continue;
                }
                match attr.parse_args::<CarbonateArgs>() {
                    Ok(parsed) => merge_export_args(&mut emitter, &mut export_args, attr, parsed),
                    Err(err) => emit!(emitter, err.span(), "{}", err),
                }
            }
        }
        _ => {}
    }

    use syn::Item::*;
    let result = match item {
        Impl(mut item) => {
            strip_export_attrs(&mut item.attrs);
            let Some(impl_descriptor) = ImplDescriptor::from_impl(&mut emitter, &item) else {
                return emitter.finish_token_stream();
            };

            enum MethodAction {
                Skip,
                ExternShim(TokenStream),
                NonExternAttach(Option<syn::Attribute>),
            }

            let method_actions: Vec<_> = impl_descriptor
                .fns
                .iter()
                .map(|fn_descriptor| {
                    validate_method_export_attrs(&mut emitter, &fn_descriptor.attrs);
                    let export_abi = export_args.export_abi.as_ref();

                    let force_shim = export_abi.is_some_and(|abi| !is_rust_abi(Some(abi)));
                    if has_valid_export_skip(&mut emitter, &fn_descriptor.attrs) {
                        MethodAction::Skip
                    } else if force_shim || !is_rust_abi(fn_descriptor.sig.abi.as_ref()) {
                        MethodAction::ExternShim(ffi_fn::gen_definition(
                            fn_descriptor,
                            impl_descriptor.trait_name,
                            impl_descriptor.generics,
                            export_abi,
                        ))
                    } else {
                        let export_attr = fn_descriptor
                            .attrs
                            .iter()
                            .find_map(|attr| parse_unsafe_export_name_attr(attr))
                            .map(|name| syn::parse_quote!(#[unsafe(export_name = #name)]))
                            .or_else(|| {
                                ffi_fn::gen_default_export_name_attr(
                                    fn_descriptor,
                                    impl_descriptor.trait_name,
                                )
                            });
                        MethodAction::NonExternAttach(export_attr)
                    }
                })
                .collect();

            for (method, action) in item
                .items
                .iter_mut()
                .filter_map(|it| match it {
                    syn::ImplItem::Fn(method) => Some(method),
                    _ => None,
                })
                .zip(method_actions.into_iter())
            {
                match action {
                    MethodAction::Skip => {
                        strip_export_attrs(&mut method.attrs);
                    }
                    MethodAction::ExternShim(ffi_fn) => {
                        strip_export_attrs(&mut method.attrs);
                        strip_consumed_export_attrs(&mut method.attrs);
                        method.block.stmts.insert(0, syn::parse_quote! { #ffi_fn });
                    }
                    MethodAction::NonExternAttach(export_attr) => {
                        strip_export_attrs(&mut method.attrs);
                        attach_export_attr_if_absent(&mut method.attrs, export_attr);
                    }
                }
            }

            quote! { #item }
        }
        Fn(mut item) => {
            strip_export_attrs(&mut item.attrs);
            enum FnAction {
                ExternShim(TokenStream),
                NonExternAttach(Option<syn::Attribute>),
            }

            let action = {
                let Some(fn_descriptor) = FnDescriptor::from_fn(&mut emitter, &item) else {
                    return emitter.finish_token_stream();
                };
                let force_shim = export_args
                    .export_abi
                    .as_ref()
                    .is_some_and(|abi| !is_rust_abi(Some(abi)));
                let use_extern_shim = force_shim || !is_rust_abi(fn_descriptor.sig.abi.as_ref());
                if use_extern_shim {
                    FnAction::ExternShim(ffi_fn::gen_definition(
                        &fn_descriptor,
                        None,
                        &Default::default(),
                        export_args.export_abi.as_ref(),
                    ))
                } else {
                    FnAction::NonExternAttach(ffi_fn::gen_default_export_name_attr(
                        &fn_descriptor,
                        None,
                    ))
                }
            };

            match action {
                FnAction::NonExternAttach(export_attr) => {
                    attach_export_attr_if_absent(&mut item.attrs, export_attr);
                    return emitter.finish_token_stream_with(quote! { #item });
                }
                FnAction::ExternShim(ffi_fn) => {
                    strip_consumed_export_attrs(&mut item.attrs);
                    item.block.stmts.insert(0, syn::parse_quote! { #ffi_fn });
                }
            }

            quote! { #item }
        }
        Struct(item) => {
            let input = syn::parse2(quote!(#item)).unwrap();
            let Some(input) = emitter.handle(FfiTypeInput::from_derive_input(&input)) else {
                return emitter.finish_token_stream();
            };

            #[cfg(feature = "getset")]
            let has_getset_derive = input
                .derive_attr
                .derives
                .iter()
                .any(|d| matches!(d, Derive::GetSet(_)));

            #[cfg(feature = "getset")]
            if has_getset_derive {
                if !input.generics.params.is_empty() {
                    emit!(
                        emitter,
                        input.generics,
                        "Generics on derived methods not supported"
                    );

                    return emitter.finish_token_stream();
                }
            }

            if input.ffi_type_attr.kind != Some(FfiTypeKindAttribute::Opaque) {
                let input = input.ast;
                return emitter.finish_token_stream_with(quote! { #input });
            }

            #[cfg(feature = "getset")]
            if has_getset_derive {
                let darling::ast::Data::Struct(fields) = &input.data else {
                    unreachable!();
                };

                let derived_ffi_fns = getset_gen::gen_derived_methods(
                    &mut emitter,
                    &input.ident,
                    &input.derive_attr,
                    &input.getset_attr,
                    fields,
                )
                .map(|fn_| ffi_fn::gen_definition(&fn_, None, &Default::default(), None));

                quote! {
                    #item
                    #(#derived_ffi_fns)*
                }
            } else {
                let input = input.ast;
                quote! { #input }
            }

            #[cfg(not(feature = "getset"))]
            {
                let input = input.ast;
                quote! { #input }
            }
        }
        Enum(item) => quote! { #item },
        Union(item) => quote! { #item },
        item => {
            emit!(emitter, item, "Item not supported");
            quote!()
        }
    };

    emitter.finish_token_stream_with(result)
}

struct ExternCMethodDecl {
    attrs: Vec<syn::Attribute>,
    vis: syn::Visibility,
    sig: syn::Signature,
}

impl syn::parse::Parse for ExternCMethodDecl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let vis = input.parse::<syn::Visibility>()?;
        let sig = input.parse::<syn::Signature>()?;
        input.parse::<syn::Token![;]>()?;

        Ok(Self { attrs, vis, sig })
    }
}

enum ExternCImplItemDecl {
    Method(ExternCMethodDecl),
    AssocType(syn::ImplItemType),
    AssocConst(syn::ImplItemConst),
}

impl syn::parse::Parse for ExternCImplItemDecl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ahead = input.fork();
        let _ = ahead.call(syn::Attribute::parse_outer)?;
        let _ = ahead.parse::<syn::Visibility>()?;

        if ahead.peek(syn::Token![type]) {
            return Ok(Self::AssocType(input.parse::<syn::ImplItemType>()?));
        }
        if ahead.peek(syn::Token![const]) {
            return Ok(Self::AssocConst(input.parse::<syn::ImplItemConst>()?));
        }

        Ok(Self::Method(input.parse::<ExternCMethodDecl>()?))
    }
}

enum ExternCImplTarget {
    Inherent(syn::Type),
    Trait(syn::Path, syn::Type),
}

struct ExternCImplDecl {
    attrs: Vec<syn::Attribute>,
    target: ExternCImplTarget,
    items: Vec<ExternCImplItemDecl>,
}

impl syn::parse::Parse for ExternCImplDecl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        input.parse::<syn::Token![impl]>()?;

        let ahead = input.fork();
        let first_ty = ahead.parse::<syn::Type>()?;
        if ahead.peek(syn::Token![for]) {
            let trait_path = if let syn::Type::Path(path) = first_ty {
                path.path
            } else {
                return Err(syn::Error::new_spanned(first_ty, "expected trait path"));
            };
            input.parse::<syn::Type>()?;
            input.parse::<syn::Token![for]>()?;
            let self_ty = input.parse::<syn::Type>()?;
            let content;
            syn::braced!(content in input);
            let mut items = Vec::new();
            while !content.is_empty() {
                items.push(content.parse::<ExternCImplItemDecl>()?);
            }
            Ok(Self {
                attrs,
                target: ExternCImplTarget::Trait(trait_path, self_ty),
                items,
            })
        } else {
            let self_ty = input.parse::<syn::Type>()?;
            let content;
            syn::braced!(content in input);
            let mut items = Vec::new();
            while !content.is_empty() {
                items.push(content.parse::<ExternCImplItemDecl>()?);
            }
            Ok(Self {
                attrs,
                target: ExternCImplTarget::Inherent(self_ty),
                items,
            })
        }
    }
}

struct ExternCFnDecl {
    attrs: Vec<syn::Attribute>,
    vis: syn::Visibility,
    sig: syn::Signature,
}

impl syn::parse::Parse for ExternCFnDecl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let vis = input.parse::<syn::Visibility>()?;
        let sig = input.parse::<syn::Signature>()?;
        input.parse::<syn::Token![;]>()?;

        Ok(Self { attrs, vis, sig })
    }
}

enum ExternCDecl {
    Impl(ExternCImplDecl),
    Fn(ExternCFnDecl),
}

struct ExternCDecls(Vec<ExternCDecl>);

impl syn::parse::Parse for ExternCDecls {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut decls = Vec::new();
        while !input.is_empty() {
            let ahead = input.fork();
            let _ = ahead.call(syn::Attribute::parse_outer)?;
            if ahead.peek(syn::Token![impl]) {
                decls.push(ExternCDecl::Impl(input.parse::<ExternCImplDecl>()?));
            } else {
                decls.push(ExternCDecl::Fn(input.parse::<ExternCFnDecl>()?));
            }
        }
        Ok(Self(decls))
    }
}

fn parse_inner_abi(attrs: &[syn::Attribute]) -> Result<Option<syn::Abi>, syn::Error> {
    let mut abi = None;
    for attr in attrs {
        let syn::Meta::NameValue(nv) = &attr.meta else {
            continue;
        };
        if !nv.path.is_ident("abi") {
            continue;
        }
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(abi_lit),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(
                &nv.value,
                "expected string literal in `#![abi = \"...\"]`",
            ));
        };
        if abi.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "`abi` can only be provided once",
            ));
        }
        let parsed = syn::parse2::<syn::Abi>(quote!(extern #abi_lit))
            .map_err(|_| syn::Error::new_spanned(abi_lit, "invalid ABI"))?;
        abi = Some(parsed);
    }

    Ok(abi)
}

fn with_default_abi(sig: &syn::Signature, default_abi: Option<&syn::Abi>) -> syn::Signature {
    let mut sig = sig.clone();
    if sig.abi.is_none()
        && let Some(default_abi) = default_abi
    {
        sig.abi = Some(default_abi.clone());
    }
    sig
}

fn parse_inner_link_prefix(attrs: &[syn::Attribute]) -> Result<Option<syn::LitStr>, syn::Error> {
    let mut link_prefix = None;
    for attr in attrs {
        let link_prefix_lit = if let Some(link_meta) = parse_link_attr(attr)? {
            let Some(link_prefix_lit) = link_meta.link_prefix else {
                continue;
            };
            link_prefix_lit
        } else {
            continue;
        };
        if link_prefix.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "`link_prefix` can only be provided once",
            ));
        }
        link_prefix = Some(link_prefix_lit);
    }

    Ok(link_prefix)
}

fn validate_inner_attrs(attrs: &[syn::Attribute]) -> Result<(), syn::Error> {
    for attr in attrs {
        if attr.path().is_ident("link_name") {
            return Err(syn::Error::new_spanned(
                attr,
                "`#![link_name = \"...\"]` is not supported",
            ));
        }
    }
    Ok(())
}

fn validate_extern_decl_attrs(decls: &[ExternCDecl]) -> Result<(), syn::Error> {
    for decl in decls {
        match decl {
            ExternCDecl::Fn(decl) => {
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "declaration-level `#[link(...)]` attributes are not supported; use `#![link(name = \"...\")]` or `#![link(crate = \"...\")]` on the macro block",
                        ));
                    }
                }
            }
            ExternCDecl::Impl(decl) => {
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() || attr.path().is_ident("link_name") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "impl-level link attributes are not supported; use method-level `#[link_name = \"...\"]` or macro-level `#![link(...)]`",
                        ));
                    }
                }
                for item in &decl.items {
                    match item {
                        ExternCImplItemDecl::Method(method) => {
                            for attr in &method.attrs {
                                if parse_link_attr(attr)?.is_some() {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        "declaration-level `#[link(...)]` attributes are not supported; use `#![link(name = \"...\")]` or `#![link(crate = \"...\")]` on the macro block",
                                    ));
                                }
                            }
                        }
                        ExternCImplItemDecl::AssocType(assoc) => {
                            for attr in &assoc.attrs {
                                if parse_link_attr(attr)?.is_some()
                                    || attr.path().is_ident("link_name")
                                {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        "link attributes are only supported on imported function declarations",
                                    ));
                                }
                            }
                        }
                        ExternCImplItemDecl::AssocConst(assoc) => {
                            for attr in &assoc.attrs {
                                if parse_link_attr(attr)?.is_some()
                                    || attr.path().is_ident("link_name")
                                {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        "link attributes are only supported on imported function declarations",
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_inner_special_attr(attr: &syn::Attribute) -> bool {
    if attr.path().is_ident("abi") {
        return true;
    }

    parse_link_attr(attr)
        .ok()
        .flatten()
        .is_some_and(|parsed| parsed.link_prefix.is_some())
}

fn inner_attr_to_outer(attr: &syn::Attribute) -> syn::Attribute {
    let meta = &attr.meta;
    syn::parse_quote!(#[#meta])
}

fn expand_extern_import_decls(
    decls: Vec<ExternCDecl>,
    import_abi: &syn::Abi,
    default_link_prefix: Option<&syn::LitStr>,
    module_attrs: &[syn::Attribute],
) -> TokenStream {
    let mut emitter = Emitter::new();
    let import_prefix = default_link_prefix.map(|link_prefix| quote!(concat!(#link_prefix, "_")));
    let mut out = Vec::new();

    for decl in decls {
        match decl {
            ExternCDecl::Impl(decl) => {
                let is_trait_target = matches!(decl.target, ExternCImplTarget::Trait(_, _));
                let items = decl.items.iter().map(|item| match item {
                    ExternCImplItemDecl::Method(m) => {
                        let attrs: Vec<_> = m
                            .attrs
                            .iter()
                            .filter(|attr| parse_link_attr(attr).ok().flatten().is_none())
                            .collect();
                        let vis = &m.vis;
                        let sig = if is_trait_target {
                            m.sig.clone()
                        } else {
                            with_default_abi(&m.sig, Some(import_abi))
                        };
                        quote! {
                            #(#module_attrs)*
                            #(#attrs)*
                            #vis #sig {
                                unreachable!("replaced by extern_")
                            }
                        }
                    }
                    ExternCImplItemDecl::AssocType(assoc) => quote! {
                        #(#module_attrs)*
                        #assoc
                    },
                    ExternCImplItemDecl::AssocConst(assoc) => quote! {
                        #(#module_attrs)*
                        #assoc
                    },
                });

                let impl_item_tokens = match decl.target {
                    ExternCImplTarget::Inherent(self_ty) => quote! {
                        impl #self_ty {
                            #(#items)*
                        }
                    },
                    ExternCImplTarget::Trait(trait_path, self_ty) => quote! {
                        impl #trait_path for #self_ty {
                            #(#items)*
                        }
                    },
                };
                let Some(item_impl) =
                    emitter.handle(syn::parse2::<syn::ItemImpl>(impl_item_tokens))
                else {
                    continue;
                };
                let Some(impl_desc) = ImplDescriptor::from_foreign_impl(&mut emitter, &item_impl)
                else {
                    continue;
                };

                let wrapped_methods = impl_desc
                    .fns
                    .iter()
                    .map(|fn_| {
                        let mut method_link_name: Option<syn::LitStr> = None;
                        for attr in &fn_.attrs {
                            match parse_link_name_attr(attr) {
                                Ok(Some(link_name)) => {
                                    if method_link_name.replace(link_name).is_some() {
                                        emit!(
                                            emitter,
                                            attr,
                                            "`link_name` can only be provided once per method"
                                        );
                                    }
                                }
                                Ok(None) => {}
                                Err(err) => emit!(emitter, attr, "{}", err),
                            }
                        }

                        let method_import_name = method_link_name.as_ref();
                        let method_prefix = if method_import_name.is_some() {
                            None
                        } else {
                            import_prefix.as_ref()
                        };
                        wrapper::wrap_method_with_import(
                            fn_,
                            impl_desc.trait_name,
                            method_prefix,
                            method_import_name,
                            Some(import_abi),
                        )
                    })
                    .collect::<Vec<_>>();

                let self_ty = &impl_desc.fns[0].self_ty;
                let impl_trait_for = impl_desc
                    .trait_name
                    .map(|trait_name| quote! { #trait_name for });
                let (associated_names, associated_types) = impl_desc.associated_types.iter().fold(
                    (Vec::new(), Vec::new()),
                    |(mut names, mut types), (name, ty)| {
                        names.push(name);
                        types.push(ty);
                        (names, types)
                    },
                );
                let mut associated_const_names = Vec::new();
                let mut associated_const_types = Vec::new();
                let mut associated_const_values = Vec::new();
                for (name, ty, value) in &impl_desc.associated_consts {
                    associated_const_names.push(name);
                    associated_const_types.push(ty);
                    associated_const_values.push(value);
                }

                out.push(quote! {
                    impl #impl_trait_for #self_ty {
                        #(type #associated_names = #associated_types;)*
                        #(const #associated_const_names: #associated_const_types = #associated_const_values;)*
                        #(#wrapped_methods)*
                    }
                });
            }
            ExternCDecl::Fn(decl) => {
                let fn_attrs: Vec<_> = decl
                    .attrs
                    .iter()
                    .filter(|attr| parse_link_attr(attr).ok().flatten().is_none())
                    .collect();
                let vis = &decl.vis;
                let sig = with_default_abi(&decl.sig, Some(import_abi));
                let Some(item_fn) = emitter.handle(syn::parse2::<syn::ItemFn>(quote! {
                    #(#module_attrs)*
                    #(#fn_attrs)*
                    #vis #sig {
                        unreachable!("replaced by extern_")
                    }
                })) else {
                    continue;
                };

                let mut item_link_name: Option<syn::LitStr> = None;
                for attr in &item_fn.attrs {
                    match parse_link_name_attr(attr) {
                        Ok(Some(link_name)) => {
                            if item_link_name.replace(link_name).is_some() {
                                emit!(
                                    emitter,
                                    attr,
                                    "`link_name` can only be provided once per function"
                                );
                            }
                        }
                        Ok(None) => {}
                        Err(err) => emit!(emitter, attr, "{}", err),
                    }
                }

                let Some(fn_descriptor) = FnDescriptor::from_fn(&mut emitter, &item_fn) else {
                    continue;
                };
                let import_name = item_link_name.as_ref();
                let method_prefix = if import_name.is_some() {
                    None
                } else {
                    import_prefix.as_ref()
                };
                out.push(wrapper::wrap_method_with_import(
                    &fn_descriptor,
                    None,
                    method_prefix,
                    import_name,
                    Some(import_abi),
                ));
            }
        }
    }

    emitter.finish_token_stream_with(quote!(#(#out)*))
}

/// See `[extern_]`
#[manyhow]
#[proc_macro]
#[allow(non_snake_case)]
pub fn extern_C(input: TokenStream) -> TokenStream {
    struct ExternCInput {
        attrs: Vec<syn::Attribute>,
        decls: ExternCDecls,
    }

    impl syn::parse::Parse for ExternCInput {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let attrs = input.call(syn::Attribute::parse_inner)?;
            let decls = input.parse::<ExternCDecls>()?;
            Ok(Self { attrs, decls })
        }
    }

    let original_input = input.clone();
    let input = match syn::parse2::<ExternCInput>(input) {
        Err(err) => return err.to_compile_error(),
        Ok(input) => input,
    };
    if let Err(err) = validate_inner_attrs(&input.attrs) {
        return err.to_compile_error();
    }
    if let Err(err) = validate_extern_decl_attrs(&input.decls.0) {
        return err.to_compile_error();
    }
    if let Ok(Some(_)) = parse_inner_abi(&input.attrs) {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "extern_C! does not support `#![abi = \"...\"]`",
        )
        .to_compile_error();
    }

    extern_(
        (quote! {
            #![abi = "C"]
            #original_input
        })
        .into(),
    )
    .into()
}

#[manyhow]
#[proc_macro]
pub fn extern_(input: TokenStream) -> TokenStream {
    struct ExternInput {
        attrs: Vec<syn::Attribute>,
        decls: ExternCDecls,
    }

    impl syn::parse::Parse for ExternInput {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let attrs = input.call(syn::Attribute::parse_inner)?;
            let decls = input.parse::<ExternCDecls>()?;
            Ok(Self { attrs, decls })
        }
    }

    let input = match syn::parse2::<ExternInput>(input) {
        Err(err) => return err.to_compile_error(),
        Ok(input) => input,
    };
    let abi = match parse_inner_abi(&input.attrs) {
        Err(err) => return err.to_compile_error(),
        Ok(Some(abi)) => abi,
        Ok(None) => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "extern_! requires `#![abi = \"...\"]`",
            )
            .to_compile_error();
        }
    };
    if let Err(err) = validate_inner_attrs(&input.attrs) {
        return err.to_compile_error();
    };
    if let Err(err) = validate_extern_decl_attrs(&input.decls.0) {
        return err.to_compile_error();
    }
    let default_link_prefix = match parse_inner_link_prefix(&input.attrs) {
        Err(err) => return err.to_compile_error(),
        Ok(link_prefix) => link_prefix,
    };
    let module_attrs: Vec<_> = input
        .attrs
        .iter()
        .filter(|attr| !is_inner_special_attr(attr))
        .map(inner_attr_to_outer)
        .collect();

    expand_extern_import_decls(
        input.decls.0,
        &abi,
        default_link_prefix.as_ref(),
        &module_attrs,
    )
}
