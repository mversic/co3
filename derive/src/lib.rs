//! Crate containing FFI related macro functionality
use darling::FromDeriveInput;
use handle::{
    ExportPolySpec, gen_poly_export, infer_default_handle_id_specs, infer_poly_handle_id_specs,
    parse_entry_handle_map_attr, parse_handle_id_attr, validate_handle_id_positions_for_sig,
};
use impl_visitor::{FnDescriptor, ImplDescriptor, path_symbol_name};
use manyhow::{emit, manyhow};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};
use wrapper::{ExternTypeLinkMode, HandleIdSpec};

use crate::{
    emitter::Emitter,
    extern_c::{FfiTypeInput, derive_extern_c},
    impl_visitor::Arg,
};

mod attr_parse;
mod emitter;
mod extern_c;
mod ffi_fn;
mod handle;
mod impl_visitor;
mod utils;
mod wrapper;

const NO_EXPORT_ABI_MSG: &str = "specify ABI with `export(\"...\")`";

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

#[derive(Clone, Default)]
struct CarbonateArgs {
    export_abi: Option<syn::Abi>,
    export_methods: Option<Vec<ExportEntrySpec>>,
}

#[derive(Clone)]
struct ExportMethodSpec {
    self_ty: syn::Type,
    trait_path: Option<syn::Path>,
    method: syn::Ident,
    decl_sig: Option<syn::Signature>,
    unsafe_name: Option<syn::LitStr>,
    unsafe_no_mangle: bool,
}

#[derive(Clone)]
struct ExportFunctionSpec {
    fn_path: syn::Path,
    decl_sig: Option<syn::Signature>,
    unsafe_name: Option<syn::LitStr>,
    unsafe_no_mangle: bool,
}

#[derive(Clone)]
enum ExportEntrySpec {
    Method(ExportMethodSpec),
    Function(ExportFunctionSpec),
    Poly(ExportPolySpec),
}

impl syn::parse::Parse for ExportMethodSpec {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        #[derive(Default)]
        struct UnsafeExportOpts {
            name: Option<syn::LitStr>,
            no_mangle: bool,
        }

        fn parse_unsafe_export_opts(
            input: syn::parse::ParseStream,
        ) -> syn::Result<UnsafeExportOpts> {
            if !input.peek(syn::Token![unsafe]) {
                return Ok(UnsafeExportOpts::default());
            }

            input.parse::<syn::Token![unsafe]>()?;
            let content;
            syn::parenthesized!(content in input);

            if content.peek(syn::LitStr) {
                let lit: syn::LitStr = content.parse()?;
                if !content.is_empty() {
                    return Err(content.error("expected a single string literal"));
                }
                return Ok(UnsafeExportOpts {
                    name: Some(lit),
                    no_mangle: false,
                });
            }

            let meta: syn::Meta = content.parse()?;
            if !content.is_empty() {
                return Err(content.error(
                    "expected one of: `export_name = \"...\"`, `name = \"...\"`, `no_mangle`",
                ));
            }

            match meta {
                syn::Meta::Path(path) if path.is_ident("no_mangle") => Ok(UnsafeExportOpts {
                    name: None,
                    no_mangle: true,
                }),
                syn::Meta::NameValue(nv)
                    if nv.path.is_ident("export_name") || nv.path.is_ident("name") =>
                {
                    let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(lit),
                        ..
                    }) = nv.value
                    else {
                        return Err(syn::Error::new_spanned(
                            nv,
                            "expected string literal in `unsafe(export_name = \"...\")`",
                        ));
                    };
                    Ok(UnsafeExportOpts {
                        name: Some(lit),
                        no_mangle: false,
                    })
                }
                other => Err(syn::Error::new_spanned(
                    other,
                    "expected one of: `export_name = \"...\"`, `name = \"...\"`, `no_mangle`",
                )),
            }
        }

        let prefix_unsafe = parse_unsafe_export_opts(input)?;

        let method_path: syn::ExprPath = input.parse()?;
        let path = method_path.path;
        let method = path
            .segments
            .last()
            .map(|seg| seg.ident.clone())
            .ok_or_else(|| syn::Error::new_spanned(&path, "expected method path after selector"))?;

        let (self_ty, trait_path) = if let Some(qself) = method_path.qself {
            let self_ty = *qself.ty;
            let trait_path = if qself.position == 0 {
                None
            } else {
                let mut trait_segments = syn::punctuated::Punctuated::new();
                for (idx, seg) in path.segments.iter().enumerate() {
                    if idx >= qself.position {
                        break;
                    }
                    trait_segments.push(seg.clone());
                }
                Some(syn::Path {
                    leading_colon: path.leading_colon,
                    segments: trait_segments,
                })
            };
            (self_ty, trait_path)
        } else {
            if path.segments.len() < 2 {
                return Err(syn::Error::new_spanned(
                    path,
                    "expected `<Type as Trait>::method` or `Type::method`",
                ));
            }
            let mut self_segments = syn::punctuated::Punctuated::new();
            for (idx, seg) in path.segments.iter().enumerate() {
                if idx + 1 == path.segments.len() {
                    break;
                }
                self_segments.push(seg.clone());
            }
            let self_ty: syn::Type = syn::Type::Path(syn::TypePath {
                qself: None,
                path: syn::Path {
                    leading_colon: path.leading_colon,
                    segments: self_segments,
                },
            });
            (self_ty, None)
        };

        let suffix_unsafe = if input.peek(syn::Token![as]) {
            input.parse::<syn::Token![as]>()?;
            parse_unsafe_export_opts(input)?
        } else {
            UnsafeExportOpts::default()
        };

        if prefix_unsafe.name.is_some() && suffix_unsafe.name.is_some() {
            return Err(input.error(
                "`unsafe(export_name = \"...\")` can only be provided once per selected method",
            ));
        }
        if prefix_unsafe.no_mangle && suffix_unsafe.no_mangle {
            return Err(
                input.error("`unsafe(no_mangle)` can only be provided once per selected method")
            );
        }

        let unsafe_name = prefix_unsafe.name.or(suffix_unsafe.name);
        let unsafe_no_mangle = prefix_unsafe.no_mangle || suffix_unsafe.no_mangle;
        if unsafe_name.is_some() && unsafe_no_mangle {
            return Err(input.error(
                "`unsafe(export_name = \"...\")` and `unsafe(no_mangle)` are mutually exclusive",
            ));
        }

        Ok(Self {
            self_ty,
            trait_path,
            method,
            decl_sig: None,
            unsafe_name,
            unsafe_no_mangle,
        })
    }
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

    if let Some(export_methods) = parsed.export_methods {
        if into.export_methods.is_some() {
            emit!(
                emitter,
                attr,
                "`[...]` method list can only be provided once across export attributes on the same item"
            );
        } else {
            into.export_methods = Some(export_methods);
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

    emit_export_skip_conflicts(emitter, attrs);

    true
}

fn emit_export_skip_conflicts(emitter: &mut Emitter, attrs: &[&syn::Attribute]) {
    if let Some(conflicting_attr) = attrs.iter().find(|attr| is_unsafe_no_mangle_attr(attr)) {
        emit!(
            emitter,
            conflicting_attr,
            "`#[unsafe(no_mangle)]` attribute may not be used in combination with `#[export(skip)]`"
        );
    }
    if let Some(conflicting_attr) = attrs.iter().find(|attr| is_unsafe_export_name_attr(attr)) {
        emit!(
            emitter,
            conflicting_attr,
            "`#[unsafe(export_name = ...)]` attribute may not be used in combination with `#[export(skip)]`"
        );
    }
}

struct ResolvedMethodExport {
    self_symbol: String,
    trait_symbol: Option<String>,
}

fn resolve_method_export_spec(spec: &ExportMethodSpec) -> Result<ResolvedMethodExport, syn::Error> {
    let syn::Type::Path(self_ty_path) = &spec.self_ty else {
        return Err(syn::Error::new_spanned(
            &spec.self_ty,
            "expected path type in selector self type",
        ));
    };
    Ok(ResolvedMethodExport {
        self_symbol: path_symbol_name(&self_ty_path.path),
        trait_symbol: spec.trait_path.as_ref().map(path_symbol_name),
    })
}

fn gen_method_export_name(spec: &ExportMethodSpec, resolved: &ResolvedMethodExport) -> TokenStream {
    if let Some(name) = &spec.unsafe_name {
        return quote!(#name);
    }
    if spec.unsafe_no_mangle {
        let method_name = syn::LitStr::new(&spec.method.to_string(), spec.method.span());
        return quote!(#method_name);
    }
    let self_name = syn::LitStr::new(&resolved.self_symbol, proc_macro2::Span::call_site());
    let method_name = syn::LitStr::new(&spec.method.to_string(), proc_macro2::Span::call_site());
    if let Some(trait_symbol) = &resolved.trait_symbol {
        let trait_name = syn::LitStr::new(trait_symbol, proc_macro2::Span::call_site());
        quote!(concat!(
            env!("CARGO_CRATE_NAME"),
            "_",
            #trait_name,
            "_",
            #self_name,
            "_",
            #method_name
        ))
    } else {
        quote!(concat!(
            env!("CARGO_CRATE_NAME"),
            "_",
            #self_name,
            "_",
            #method_name
        ))
    }
}

fn gen_selected_export_fn_name(
    spec: &ExportMethodSpec,
    _resolved: &ResolvedMethodExport,
) -> syn::Ident {
    let name = spec.method.to_string().to_lowercase();
    format_ident!("{name}")
}

fn rewrite_self_type(ty: &syn::Type, self_ty: &syn::Type) -> syn::Type {
    struct SelfTypeRewriter {
        self_ty: syn::Type,
    }

    impl syn::visit_mut::VisitMut for SelfTypeRewriter {
        fn visit_type_mut(&mut self, node: &mut syn::Type) {
            if let syn::Type::Path(path) = node
                && path.qself.is_none()
                && path.path.segments.len() == 1
                && path.path.segments[0].ident == "Self"
            {
                *node = self.self_ty.clone();
                return;
            }
            syn::visit_mut::visit_type_mut(self, node);
        }
    }

    let mut out = ty.clone();
    let mut rewriter = SelfTypeRewriter {
        self_ty: self_ty.clone(),
    };
    syn::visit_mut::VisitMut::visit_type_mut(&mut rewriter, &mut out);
    out
}

fn gen_selected_signature_method_export(
    spec: &ExportMethodSpec,
    resolved: &ResolvedMethodExport,
    export_abi: Option<&syn::Abi>,
    sig: &syn::Signature,
) -> TokenStream {
    let self_ty = &spec.self_ty;
    let method = &spec.method;
    let fn_name = gen_selected_export_fn_name(spec, resolved);
    let export_name = gen_method_export_name(spec, resolved);
    let abi = export_abi
        .cloned()
        .unwrap_or_else(|| syn::parse_quote!(extern "Rust"));
    let is_drop_export = spec
        .trait_path
        .as_ref()
        .is_some_and(|trait_path| path_symbol_name(trait_path) == "Drop")
        && *method == "drop";
    if is_drop_export {
        let Some(receiver) = sig.receiver() else {
            return syn::Error::new_spanned(sig, "Drop export entries must declare a receiver")
                .to_compile_error();
        };
        if receiver.reference.is_none() || receiver.mutability.is_none() {
            return syn::Error::new_spanned(receiver, "Drop export entries must use `&mut self`")
                .to_compile_error();
        }
        let mut has_typed_args = false;
        for input in &sig.inputs {
            if matches!(input, syn::FnArg::Typed(_)) {
                has_typed_args = true;
                break;
            }
        }
        if has_typed_args || !matches!(sig.output, syn::ReturnType::Default) {
            return syn::Error::new_spanned(
                sig,
                "Drop export entries must be `fn drop(&mut self);`",
            )
            .to_compile_error();
        }
        let receiver_ffi_ty = quote!(<&mut #self_ty as co3::ExternC>::CType);
        return quote! {
            #[unsafe(export_name = #export_name)]
            unsafe #abi fn #fn_name(
                receiver: #receiver_ffi_ty
            ) -> co3::FfiReturn {
                let fn_ = || {
                    let fn_body = || -> Result<(), co3::FfiReturn> {
                        let __self_ptr = receiver as *mut #self_ty;
                        unsafe {
                            let __owned: Box<#self_ty> = Box::from_raw(__self_ptr);
                            core::mem::drop(__owned);
                        }
                        Ok(())
                    };

                    if let Err(err) = fn_body() {
                        return err;
                    }

                    co3::FfiReturn::Ok
                };

                match std::panic::catch_unwind(fn_) {
                    Ok(res) => res,
                    Err(_) => co3::FfiReturn::UnrecoverableError,
                }
            }
        };
    }

    let mut decl_params: Vec<TokenStream> = Vec::new();
    let mut decode_stmts: Vec<TokenStream> = Vec::new();
    let mut sync_stmts: Vec<TokenStream> = Vec::new();
    let mut call_args: Vec<TokenStream> = Vec::new();
    let mut target_arg_tys: Vec<TokenStream> = Vec::new();

    if let Some(receiver) = sig.receiver() {
        let receiver_rust_ty: syn::Type = if receiver.reference.is_none() {
            self_ty.clone()
        } else if receiver.mutability.is_some() {
            syn::parse_quote!(&mut #self_ty)
        } else {
            syn::parse_quote!(&#self_ty)
        };
        let receiver_ffi_ty = quote!(<#receiver_rust_ty as co3::ExternC>::CType);
        decl_params.push(quote!(receiver: #receiver_ffi_ty));
        decode_stmts.push(quote! {
            let mut receiver_store = Default::default();
            let receiver: #receiver_rust_ty = unsafe { co3::Decode::decode(receiver, &mut receiver_store) }
                .ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
        target_arg_tys.push(quote!(#receiver_rust_ty));
        sync_stmts.push(quote! {
            co3::Store::sync(receiver_store).ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
        call_args.push(quote!(receiver));
    }

    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        let syn::Pat::Ident(pat_ident) = arg.pat.as_ref() else {
            return syn::Error::new_spanned(
                &arg.pat,
                "method arguments in export declarations must use identifier patterns",
            )
            .to_compile_error();
        };
        let name = pat_ident.ident.clone();
        let rust_ty = rewrite_self_type(arg.ty.as_ref(), self_ty);
        let ffi_ty = quote!(<#rust_ty as co3::ExternC>::CType);
        let store = format_ident!("{name}_store");
        decl_params.push(quote!(#name: #ffi_ty));
        decode_stmts.push(quote! {
            let mut #store = Default::default();
            let #name: #rust_ty = unsafe { co3::Decode::decode(#name, &mut #store) }
                .ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
        target_arg_tys.push(quote!(#rust_ty));
        sync_stmts.push(quote! {
            co3::Store::sync(#store).ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
        call_args.push(quote!(#name));
    }

    let target_path = if let Some(trait_path) = &spec.trait_path {
        quote!(<#self_ty as #trait_path>::#method)
    } else {
        quote!(<#self_ty>::#method)
    };
    let target_ret_ty = match &sig.output {
        syn::ReturnType::Default => quote!(()),
        syn::ReturnType::Type(_, ty) => {
            let output_ty = rewrite_self_type(ty, self_ty);
            quote!(#output_ty)
        }
    };
    let target_abi = sig.abi.as_ref().map(|abi| quote!(#abi)).unwrap_or_default();
    let target_unsafety = if sig.unsafety.is_some() {
        quote!(unsafe)
    } else {
        quote!()
    };
    let target_ptr_ty =
        quote!(#target_unsafety #target_abi fn(#(#target_arg_tys),*) -> #target_ret_ty);
    let target_call = if sig.unsafety.is_some() {
        quote!(unsafe { target(#(#call_args),*) })
    } else {
        quote!(target(#(#call_args),*))
    };

    let output_write = match &sig.output {
        syn::ReturnType::Default => quote! {
            let target: #target_ptr_ty = #target_path;
            #target_call;
        },
        syn::ReturnType::Type(_, ty) => {
            let output_ty = rewrite_self_type(ty, self_ty);
            decl_params.push(quote!(out_ptr: *mut <#output_ty as co3::out_ptr::OutPtr>::OutPtr));
            quote! {
                let target: #target_ptr_ty = #target_path;
                let output: #output_ty = #target_call;
                unsafe { <#output_ty as co3::out_ptr::OutPtrWrite>::write_out(output, out_ptr) };
            }
        }
    };

    quote! {
        #[unsafe(export_name = #export_name)]
        unsafe #abi fn #fn_name(
            #(#decl_params),*
        ) -> co3::FfiReturn {
            let fn_ = || {
                let fn_body = || -> Result<(), co3::FfiReturn> {
                    #(#decode_stmts)*
                    #output_write
                    #(#sync_stmts)*
                    Ok(())
                };

                if let Err(err) = fn_body() {
                    return err;
                }

                co3::FfiReturn::Ok
            };

            match std::panic::catch_unwind(fn_) {
                Ok(res) => res,
                Err(_) => co3::FfiReturn::UnrecoverableError,
            }
        }
    }
}

fn gen_selected_function_export(
    spec: &ExportFunctionSpec,
    export_abi: Option<&syn::Abi>,
) -> TokenStream {
    let Some(sig) = &spec.decl_sig else {
        return syn::Error::new_spanned(
            &spec.fn_path,
            "`export_!` requires free-function declarations with full signatures",
        )
        .to_compile_error();
    };

    let fn_name_ident = sig.ident.clone();
    let wrapper_name = format_ident!("export_fn_{}", fn_name_ident.to_string().to_lowercase());
    let export_name = if let Some(name) = &spec.unsafe_name {
        quote!(#name)
    } else if spec.unsafe_no_mangle {
        let name = syn::LitStr::new(&fn_name_ident.to_string(), proc_macro2::Span::call_site());
        quote!(#name)
    } else {
        let name = syn::LitStr::new(&fn_name_ident.to_string(), proc_macro2::Span::call_site());
        quote!(concat!(env!("CARGO_CRATE_NAME"), "_", #name))
    };
    let abi = export_abi
        .cloned()
        .unwrap_or_else(|| syn::parse_quote!(extern "Rust"));
    let mut decl_params = Vec::new();
    let mut decode_stmts = Vec::new();
    let mut sync_stmts = Vec::new();
    let mut call_args = Vec::new();
    let mut target_arg_tys = Vec::new();
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            return syn::Error::new_spanned(
                input,
                "methods with receivers are not supported in free-function export declarations",
            )
            .to_compile_error();
        };
        let syn::Pat::Ident(pat_ident) = arg.pat.as_ref() else {
            return syn::Error::new_spanned(
                &arg.pat,
                "function arguments in export declarations must use identifier patterns",
            )
            .to_compile_error();
        };
        let name = pat_ident.ident.clone();
        let ty = arg.ty.as_ref().clone();
        let store = format_ident!("{name}_store");
        decl_params.push(quote!(#name: <#ty as co3::ExternC>::CType));
        target_arg_tys.push(quote!(#ty));
        decode_stmts.push(quote! {
            let mut #store = Default::default();
            let #name: #ty = unsafe { co3::Decode::decode(#name, &mut #store) }
                .ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
        sync_stmts.push(quote! {
            co3::Store::sync(#store).ok_or(co3::FfiReturn::TrapRepresentation)?;
        });
        call_args.push(name);
    }
    let target_ret_ty = match &sig.output {
        syn::ReturnType::Default => quote!(()),
        syn::ReturnType::Type(_, ty) => {
            let out_ty = ty.as_ref();
            quote!(#out_ty)
        }
    };
    let target_abi = sig.abi.as_ref().map(|abi| quote!(#abi)).unwrap_or_default();
    let target_unsafety = if sig.unsafety.is_some() {
        quote!(unsafe)
    } else {
        quote!()
    };
    let target_ptr_ty =
        quote!(#target_unsafety #target_abi fn(#(#target_arg_tys),*) -> #target_ret_ty);
    let target_call = if sig.unsafety.is_some() {
        quote!(unsafe { target(#(#call_args),*) })
    } else {
        quote!(target(#(#call_args),*))
    };

    let output_write = match &sig.output {
        syn::ReturnType::Default => {
            quote! {
                let target: #target_ptr_ty = #fn_name_ident;
                #target_call;
            }
        }
        syn::ReturnType::Type(_, ty) => {
            let out_ty = ty.as_ref();
            decl_params.push(quote!(out_ptr: *mut <#out_ty as co3::out_ptr::OutPtr>::OutPtr));
            quote! {
                let target: #target_ptr_ty = #fn_name_ident;
                let output: #out_ty = #target_call;
                unsafe { <#out_ty as co3::out_ptr::OutPtrWrite>::write_out(output, out_ptr) };
            }
        }
    };

    quote! {
        #[unsafe(export_name = #export_name)]
        unsafe #abi fn #wrapper_name(
            #(#decl_params),*
        ) -> co3::FfiReturn {
            let fn_ = || {
                let fn_body = || -> Result<(), co3::FfiReturn> {
                    #(#decode_stmts)*
                    #output_write
                    #(#sync_stmts)*
                    Ok(())
                };

                if let Err(err) = fn_body() {
                    return err;
                }

                co3::FfiReturn::Ok
            };

            match std::panic::catch_unwind(fn_) {
                Ok(res) => res,
                Err(_) => co3::FfiReturn::UnrecoverableError,
            }
        }
    }
}

fn gen_selected_exports_block(emitter: &mut Emitter, export_args: &CarbonateArgs) -> TokenStream {
    let mut selected_method_exports = Vec::new();
    if let Some(methods) = &export_args.export_methods {
        for entry in methods.iter() {
            match entry {
                ExportEntrySpec::Function(function_spec) => {
                    selected_method_exports.push(gen_selected_function_export(
                        function_spec,
                        export_args.export_abi.as_ref(),
                    ));
                    continue;
                }
                ExportEntrySpec::Poly(poly_spec) => {
                    selected_method_exports
                        .push(gen_poly_export(poly_spec, export_args.export_abi.as_ref()));
                    continue;
                }
                ExportEntrySpec::Method(method_spec) => {
                    let resolved = match resolve_method_export_spec(method_spec) {
                        Ok(resolved) => resolved,
                        Err(err) => {
                            emit!(emitter, method_spec.self_ty, "{}", err);
                            continue;
                        }
                    };

                    let Some(sig) = &method_spec.decl_sig else {
                        emit!(
                            emitter,
                            method_spec.method,
                            "`export_!` method entries require full signatures",
                        );
                        continue;
                    };
                    selected_method_exports.push(gen_selected_signature_method_export(
                        method_spec,
                        &resolved,
                        export_args.export_abi.as_ref(),
                        sig,
                    ));
                }
            }
        }
    }

    if selected_method_exports.is_empty() {
        quote! {}
    } else {
        quote! {
            #(
                const _: () = {
                    #selected_method_exports
                };
            )*
        }
    }
}

impl syn::parse::Parse for CarbonateArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(input.error(NO_EXPORT_ABI_MSG));
        }

        let abi_name: syn::LitStr = input.parse()?;
        let export_abi = syn::parse2::<syn::Abi>(quote!(extern #abi_name))
            .map_err(|_| syn::Error::new_spanned(abi_name, "invalid ABI"))?;

        let mut export_methods: Option<Vec<ExportEntrySpec>> = None;
        let mut saw_separator = false;
        let mut saw_method = false;
        while !input.is_empty() {
            if input.peek(syn::Token![,]) {
                saw_separator = true;
                input.parse::<syn::Token![,]>()?;
                continue;
            }
            if input.peek(syn::token::Bracket) {
                if export_methods.is_some() {
                    return Err(input.error("`[...]` method list can only be provided once"));
                }
                let content;
                syn::bracketed!(content in input);
                let methods = content.parse_terminated(ExportMethodSpec::parse, syn::Token![,])?;
                if methods.is_empty() {
                    return Err(content.error("`[...]` method list may not be empty"));
                }
                export_methods = Some(methods.into_iter().map(ExportEntrySpec::Method).collect());
                saw_method = true;
                continue;
            }
            let method = input.parse::<ExportMethodSpec>()?;
            saw_method = true;
            match export_methods.as_mut() {
                Some(methods) => methods.push(ExportEntrySpec::Method(method)),
                None => export_methods = Some(vec![ExportEntrySpec::Method(method)]),
            }
        }
        if saw_separator && !saw_method {
            return Err(syn::Error::new_spanned(
                export_abi,
                "expected selected method after ABI",
            ));
        }

        Ok(Self {
            export_abi: Some(export_abi),
            export_methods,
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

fn gen_export_link_prefix(link_prefix: syn::LitStr) -> TokenStream {
    let mut pref = link_prefix.value();
    if !pref.ends_with('_') {
        pref.push('_');
    }
    let link_prefix = syn::LitStr::new(&pref, link_prefix.span());
    quote!(#link_prefix)
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
/// use co3::ReprC as ReprCAlias;
///
/// #[derive(ReprCAlias)]
/// pub struct Hello(u32);
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
/// Works on `impl` blocks and free `fn` items.
/// Type items are not supported by this attribute; use `export_!`/`export_C!` for selecting individual exports.
///
/// # Example:
/// ```rust
/// use co3::{ReprC, export, export_C};
///
/// trait MyTrait {
///     fn foo();
/// }
///
/// #[derive(ReprC, Clone)]
/// #[repr(transparent)]
/// pub struct Foo(u8);
///
/// #[export("C")]
/// impl MyTrait for Foo {
///     fn foo() {}
/// }
///
/// #[export("C")]
/// impl Foo {
///     pub fn new(id: u8) -> Self {
///         Self(id)
///     }
///
///     #[export(skip)]
///     pub fn new2(id: u8) -> Self {
///         Self(id)
///     }
/// }
///
/// #[export("C")]
/// fn selected_only() -> Foo {
///     Foo::new(7)
/// }
///
/// fn selected_only2() -> Foo {
///     Foo::new(7)
/// }
///
/// export_C! {
///     impl Foo {
///         pub fn new2(id: u8) -> Self;
///     }
///
///     fn selected_only2() -> Foo;
/// }
/// ```
#[manyhow]
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut emitter = Emitter::new();
    let item = match syn::parse2::<syn::Item>(item) {
        Err(err) => return err.to_compile_error(),
        Ok(item) => item,
    };
    let mut export_args = match syn::parse2::<CarbonateArgs>(attr) {
        Ok(args) => args,
        Err(err) => {
            let msg = err.to_string();
            emit!(emitter, err.span(), "{}", msg);
            CarbonateArgs::default()
        }
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
            let allow_non_lifetime_impl_generics_for_drop = item
                .trait_
                .as_ref()
                .is_some_and(|(_, trait_path, _)| path_symbol_name(trait_path) == "Drop");
            let impl_descriptor = if allow_non_lifetime_impl_generics_for_drop {
                ImplDescriptor::from_impl_allow_generics(&mut emitter, &item)
            } else {
                ImplDescriptor::from_impl(&mut emitter, &item)
            };
            let Some(impl_descriptor) = impl_descriptor else {
                return emitter.finish_token_stream();
            };

            enum MethodAction {
                Skip,
                ExternShim(TokenStream),
            }

            let method_actions: Vec<_> = impl_descriptor
                .fns
                .iter()
                .map(|fn_descriptor| {
                    validate_method_export_attrs(&mut emitter, &fn_descriptor.attrs);
                    let export_abi = export_args.export_abi.as_ref();

                    if has_valid_export_skip(&mut emitter, &fn_descriptor.attrs) {
                        MethodAction::Skip
                    } else {
                        MethodAction::ExternShim(ffi_fn::gen_definition(
                            fn_descriptor,
                            impl_descriptor.trait_name,
                            impl_descriptor.generics,
                            export_abi,
                        ))
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
                }
            }

            quote! { #item }
        }
        Fn(mut item) => {
            let ffi_shim = {
                let Some(fn_descriptor) = FnDescriptor::from_fn(&mut emitter, &item) else {
                    return emitter.finish_token_stream();
                };
                ffi_fn::gen_definition(
                    &fn_descriptor,
                    None,
                    &Default::default(),
                    export_args.export_abi.as_ref(),
                )
            };

            strip_export_attrs(&mut item.attrs);
            strip_consumed_export_attrs(&mut item.attrs);
            item.block.stmts.insert(0, syn::parse_quote! { #ffi_shim });

            quote! { #item }
        }
        Struct(item) => {
            emit!(
                emitter,
                item,
                "`#[export]` is not supported on type items; use `export_!`/`export_C!`"
            );
            quote! { #item }
        }
        Enum(item) => {
            emit!(
                emitter,
                item,
                "`#[export]` is not supported on type items; use `export_!`/`export_C!`"
            );
            quote! { #item }
        }
        Union(item) => {
            emit!(
                emitter,
                item,
                "`#[export]` is not supported on type items; use `export_!`/`export_C!`"
            );
            quote! { #item }
        }
        Trait(mut item) => {
            strip_export_attrs(&mut item.attrs);
            emit!(
                emitter,
                item,
                "`#[export]` is not supported on trait items; use `export_!`/`export_C!` with explicit declarations"
            );
            quote! { #item }
        }
        item => {
            emit!(emitter, item, "Item not supported");
            quote!()
        }
    };

    emitter.finish_token_stream_with(result)
}

fn apply_exports_entry_attrs(
    entry: &mut ExportEntrySpec,
    attrs: &[syn::Attribute],
) -> syn::Result<()> {
    for attr in attrs {
        if let Some(name) = parse_unsafe_export_name_attr(attr) {
            match entry {
                ExportEntrySpec::Method(method) => {
                    if method.unsafe_name.is_some() {
                        return Err(syn::Error::new_spanned(
                            &method.self_ty,
                            "export name can only be specified once per entry",
                        ));
                    }
                    method.unsafe_name = Some(name);
                }
                ExportEntrySpec::Function(function) => {
                    if function.unsafe_name.is_some() {
                        return Err(syn::Error::new_spanned(
                            &function.fn_path,
                            "export name can only be specified once per entry",
                        ));
                    }
                    function.unsafe_name = Some(name);
                }
                ExportEntrySpec::Poly(poly) => {
                    if poly.unsafe_name.is_some() {
                        return Err(syn::Error::new_spanned(
                            &poly.method,
                            "export name can only be specified once per entry",
                        ));
                    }
                    poly.unsafe_name = Some(name);
                }
            }
        } else if is_unsafe_no_mangle_attr(attr) {
            match entry {
                ExportEntrySpec::Method(method) => {
                    if method.unsafe_no_mangle {
                        return Err(syn::Error::new_spanned(
                            &method.self_ty,
                            "no_mangle can only be specified once per entry",
                        ));
                    }
                    method.unsafe_no_mangle = true;
                }
                ExportEntrySpec::Function(function) => {
                    if function.unsafe_no_mangle {
                        return Err(syn::Error::new_spanned(
                            &function.fn_path,
                            "no_mangle can only be specified once per entry",
                        ));
                    }
                    function.unsafe_no_mangle = true;
                }
                ExportEntrySpec::Poly(poly) => {
                    if poly.unsafe_no_mangle {
                        return Err(syn::Error::new_spanned(
                            &poly.method,
                            "no_mangle can only be specified once per entry",
                        ));
                    }
                    poly.unsafe_no_mangle = true;
                }
            }
        } else if attr.path().is_ident("dispatch") || attr.path().is_ident("id_pos") {
            match entry {
                ExportEntrySpec::Poly(_) => {
                    // consumed by poly selector parser
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`dispatch`/`id_pos` attributes are only supported on polymorphic selector entries",
                    ));
                }
            }
        } else {
            return Err(syn::Error::new_spanned(
                attr,
                "unsupported attribute in export entry; only `#[unsafe(export_name = \"...\")]` and `#[unsafe(no_mangle)]` are allowed",
            ));
        }
    }

    match entry {
        ExportEntrySpec::Method(method) => {
            if method.unsafe_name.is_some() && method.unsafe_no_mangle {
                return Err(syn::Error::new_spanned(
                    &method.self_ty,
                    "`export_name` and `no_mangle` are mutually exclusive",
                ));
            }
        }
        ExportEntrySpec::Function(function) => {
            if function.unsafe_name.is_some() && function.unsafe_no_mangle {
                return Err(syn::Error::new_spanned(
                    &function.fn_path,
                    "`export_name` and `no_mangle` are mutually exclusive",
                ));
            }
        }
        ExportEntrySpec::Poly(poly) => {
            if poly.unsafe_name.is_some() && poly.unsafe_no_mangle {
                return Err(syn::Error::new_spanned(
                    &poly.method,
                    "`export_name` and `no_mangle` are mutually exclusive",
                ));
            }
        }
    }

    Ok(())
}

fn parse_decl_target_header(header: TokenStream) -> syn::Result<(Option<syn::Path>, syn::Type)> {
    let tokens: Vec<proc_macro2::TokenTree> = header.into_iter().collect();
    let start_idx = if matches!(
        tokens.first(),
        Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '<'
    ) {
        let mut depth = 0usize;
        let mut idx = 0usize;
        while idx < tokens.len() {
            if let proc_macro2::TokenTree::Punct(p) = &tokens[idx] {
                match p.as_char() {
                    '<' => depth += 1,
                    '>' if depth > 0 => {
                        depth -= 1;
                        if depth == 0 {
                            idx += 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            idx += 1;
        }
        idx
    } else {
        0
    };

    let mut trait_tokens = TokenStream::new();
    let mut self_tokens = TokenStream::new();
    let mut saw_for = false;
    let mut angle_depth = 0usize;

    for tt in tokens.into_iter().skip(start_idx) {
        if !saw_for {
            let is_top_level_for = angle_depth == 0
                && matches!(
                    &tt,
                    proc_macro2::TokenTree::Ident(ident) if ident == "for"
                );
            if is_top_level_for {
                saw_for = true;
                continue;
            }
            if let proc_macro2::TokenTree::Punct(punct) = &tt {
                match punct.as_char() {
                    '<' => angle_depth += 1,
                    '>' if angle_depth > 0 => angle_depth -= 1,
                    _ => {}
                }
            }
            trait_tokens.extend(std::iter::once(tt));
        } else {
            self_tokens.extend(std::iter::once(tt));
        }
    }

    if saw_for {
        let trait_path: syn::Path = syn::parse2(trait_tokens)?;
        let self_ty: syn::Type = syn::parse2(self_tokens)?;
        Ok((Some(trait_path), self_ty))
    } else {
        let self_ty: syn::Type = syn::parse2(trait_tokens)?;
        Ok((None, self_ty))
    }
}

struct DeclMethod {
    attrs: Vec<syn::Attribute>,
    _vis: syn::Visibility,
    sig: syn::Signature,
}

impl syn::parse::Parse for DeclMethod {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let vis = input.parse::<syn::Visibility>()?;
        let sig = input.parse::<syn::Signature>()?;
        input.parse::<syn::Token![;]>()?;
        Ok(Self {
            attrs,
            _vis: vis,
            sig,
        })
    }
}

struct DeclFunction {
    _vis: syn::Visibility,
    sig: syn::Signature,
}

impl syn::parse::Parse for DeclFunction {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Use ForeignItemFn shape so visibility + semicolon declarations parse naturally.
        let item = input.parse::<syn::ForeignItemFn>()?;
        Ok(Self {
            _vis: item.vis,
            sig: item.sig,
        })
    }
}

fn ensure_no_handle_arg_attrs(sig: &syn::Signature) -> syn::Result<()> {
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                for attr in &receiver.attrs {
                    if attr.path().is_ident("dispatch") || attr.path().is_ident("id_pos") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`#[dispatch]`/`#[id_pos]` are only supported on `trait` export entries",
                        ));
                    }
                }
            }
            syn::FnArg::Typed(arg) => {
                for attr in &arg.attrs {
                    if attr.path().is_ident("dispatch") || attr.path().is_ident("id_pos") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`#[dispatch]`/`#[id_pos]` are only supported on `trait` export entries",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn parse_export_entries(input: syn::parse::ParseStream) -> syn::Result<Vec<ExportEntrySpec>> {
    fn synthesize_self_selector_types(
        self_ty: &syn::Type,
        key_types: &BTreeMap<String, Vec<syn::Type>>,
    ) -> Vec<syn::Type> {
        struct TypeRewriter<'a> {
            map: &'a BTreeMap<String, syn::Type>,
        }
        impl syn::visit_mut::VisitMut for TypeRewriter<'_> {
            fn visit_type_mut(&mut self, node: &mut syn::Type) {
                if let syn::Type::Path(path_ty) = node
                    && path_ty.qself.is_none()
                    && path_ty.path.segments.len() == 1
                    && matches!(path_ty.path.segments[0].arguments, syn::PathArguments::None)
                {
                    let key = path_ty.path.segments[0].ident.to_string();
                    if let Some(replacement) = self.map.get(&key) {
                        *node = replacement.clone();
                        return;
                    }
                }
                syn::visit_mut::visit_type_mut(self, node);
            }
        }

        let max_arms = key_types.values().map(Vec::len).max().unwrap_or(0);
        if max_arms == 0 {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(max_arms);
        for arm_idx in 0..max_arms {
            let mut selector_map = BTreeMap::<String, syn::Type>::new();
            for (selector, types) in key_types {
                if types.is_empty() {
                    continue;
                }
                let selected = if types.len() == 1 {
                    types[0].clone()
                } else if arm_idx < types.len() {
                    types[arm_idx].clone()
                } else {
                    continue;
                };
                selector_map.insert(selector.clone(), selected);
            }

            let mut rewritten = self_ty.clone();
            let mut rewriter = TypeRewriter { map: &selector_map };
            syn::visit_mut::VisitMut::visit_type_mut(&mut rewriter, &mut rewritten);
            out.push(rewritten);
        }
        out
    }

    fn has_dispatch_arg_attr(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| attr.path().is_ident("dispatch"))
    }
    fn base_handle_selector_type(ty: &syn::Type) -> &syn::Type {
        if let syn::Type::Reference(reference) = ty {
            return &reference.elem;
        }
        ty
    }
    fn is_parametric_handle_selector(
        ty: &syn::Type,
        allowed: &std::collections::BTreeSet<String>,
    ) -> bool {
        let ty = base_handle_selector_type(ty);
        let syn::Type::Path(type_path) = ty else {
            return false;
        };
        if type_path.qself.is_some() || type_path.path.segments.len() != 1 {
            return false;
        }
        let seg = &type_path.path.segments[0];
        if !matches!(seg.arguments, syn::PathArguments::None) {
            return false;
        }
        allowed.contains(&seg.ident.to_string())
    }

    let mut methods = Vec::new();

    while !input.is_empty() {
        while input.peek(syn::Token![;]) {
            input.parse::<syn::Token![;]>()?;
        }
        if input.is_empty() {
            break;
        }

        let entry_attrs = input.call(syn::Attribute::parse_outer)?;
        if input.peek(syn::Token![impl]) {
            let mut entry_handle_map: Option<BTreeMap<String, Vec<syn::Type>>> = None;
            for attr in &entry_attrs {
                if parse_unsafe_export_name_attr(attr).is_some() || is_unsafe_no_mangle_attr(attr) {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`#[unsafe(export_name = \"...\")]` and `#[unsafe(no_mangle)]` are not supported on `impl` export entries; put them on methods",
                    ));
                }
                if let Some(map) = parse_entry_handle_map_attr(attr)?
                    && entry_handle_map.replace(map).is_some()
                {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`dispatch` mapping can only be provided once per entry",
                    ));
                }
                if attr.path().is_ident("id_pos") {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`#[id_pos(...)]` is not supported on impl export entries; put it on methods",
                    ));
                }
            }
            input.parse::<syn::Token![impl]>()?;
            let mut header = TokenStream::new();
            while !input.peek(syn::token::Brace) {
                let tt: proc_macro2::TokenTree = input.parse()?;
                header.extend(std::iter::once(tt));
            }
            let (trait_path, self_ty) = parse_decl_target_header(header)?;

            let content;
            syn::braced!(content in input);
            while !content.is_empty() {
                let decl_method = content.parse::<DeclMethod>()?;
                ensure_no_handle_arg_attrs(&decl_method.sig)?;
                let method = decl_method.sig.ident.clone();
                let mut entry = if let Some(key_types) = entry_handle_map.clone() {
                    let mut key_types = key_types;
                    if !key_types.contains_key("Self") {
                        let self_types = synthesize_self_selector_types(&self_ty, &key_types);
                        if !self_types.is_empty() {
                            key_types.insert("Self".to_owned(), self_types);
                        }
                    }
                    let mut explicit_handle_id_specs: Vec<HandleIdSpec> = Vec::new();
                    for attr in &decl_method.attrs {
                        if let Some(mut specs) = parse_handle_id_attr(attr)? {
                            explicit_handle_id_specs.append(&mut specs);
                        }
                    }
                    let method_handle_id_specs = infer_poly_handle_id_specs(
                        &decl_method.sig,
                        &key_types,
                        &explicit_handle_id_specs,
                    );
                    validate_handle_id_positions_for_sig(
                        &decl_method.sig,
                        &method_handle_id_specs,
                    )?;
                    ExportEntrySpec::Poly(ExportPolySpec {
                        trait_path: trait_path.clone(),
                        method,
                        decl_sig: decl_method.sig,
                        key_types,
                        handle_id_specs: method_handle_id_specs,
                        unsafe_name: None,
                        unsafe_no_mangle: false,
                    })
                } else {
                    ExportEntrySpec::Method(ExportMethodSpec {
                        self_ty: self_ty.clone(),
                        trait_path: trait_path.clone(),
                        method,
                        decl_sig: Some(decl_method.sig),
                        unsafe_name: None,
                        unsafe_no_mangle: false,
                    })
                };
                let mut combined_attrs = entry_attrs.clone();
                combined_attrs.extend(decl_method.attrs);
                apply_exports_entry_attrs(&mut entry, &combined_attrs)?;
                methods.push(entry);
            }
            continue;
        }

        if input.peek(syn::Token![trait]) {
            let trait_item: syn::ItemTrait = input.parse()?;
            let trait_ident = &trait_item.ident;
            let trait_generics = &trait_item.generics;
            let (_, trait_ty_generics, _) = trait_generics.split_for_impl();
            let trait_path: syn::Path = syn::parse_quote!(#trait_ident #trait_ty_generics);
            let mut allowed_handle_selectors = std::collections::BTreeSet::<String>::new();
            allowed_handle_selectors.insert("Self".to_string());
            for generic in &trait_item.generics.params {
                if let syn::GenericParam::Type(param) = generic {
                    allowed_handle_selectors.insert(param.ident.to_string());
                }
            }
            let mut entry_handle_map: Option<BTreeMap<String, Vec<syn::Type>>> = None;
            for attr in &entry_attrs {
                if attr.path().is_ident("id_pos") {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`#[id_pos(...)]` is not supported on trait export entries; put it on trait methods",
                    ));
                }
                if let Some(map) = parse_entry_handle_map_attr(attr)?
                    && entry_handle_map.replace(map).is_some()
                {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`handle` mapping can only be provided once per entry",
                    ));
                }
            }
            for item in trait_item.items {
                match item {
                    syn::TraitItem::Fn(method_item) => {
                        if let Some(receiver) = method_item.sig.receiver()
                            && has_dispatch_arg_attr(&receiver.attrs)
                        {
                            // receiver-side #[dispatch] is always parametric Self
                        }
                        for input in &method_item.sig.inputs {
                            let syn::FnArg::Typed(arg) = input else {
                                continue;
                            };
                            if !has_dispatch_arg_attr(&arg.attrs) {
                                continue;
                            }
                            if !is_parametric_handle_selector(&arg.ty, &allowed_handle_selectors) {
                                return Err(syn::Error::new_spanned(
                                    &arg.ty,
                                    "in trait export entries, `#[dispatch]` arguments must use `Self` or a trait type parameter (for example `T`), not a concrete type",
                                ));
                            }
                        }
                        let mut explicit_handle_id_specs: Vec<HandleIdSpec> = Vec::new();
                        for attr in &method_item.attrs {
                            if attr.path().is_ident("dispatch") {
                                return Err(syn::Error::new_spanned(
                                    attr,
                                    "method-level `#[dispatch(...)]` is not supported in trait export entries; use `#[dispatch(...)]` on the trait entry and `#[dispatch]` on method arguments",
                                ));
                            }
                            if let Some(mut specs) = parse_handle_id_attr(attr)? {
                                explicit_handle_id_specs.append(&mut specs);
                            }
                        }
                        let Some(key_types) = entry_handle_map.clone() else {
                            return Err(syn::Error::new_spanned(
                                method_item.sig,
                                "trait export entries require `#[dispatch(...)]` mapping",
                            ));
                        };
                        let method_handle_id_specs = infer_poly_handle_id_specs(
                            &method_item.sig,
                            &key_types,
                            &explicit_handle_id_specs,
                        );
                        validate_handle_id_positions_for_sig(
                            &method_item.sig,
                            &method_handle_id_specs,
                        )?;
                        let method = method_item.sig.ident.clone();
                        let mut combined_attrs = entry_attrs.clone();
                        combined_attrs.extend(method_item.attrs.clone());

                        let mut entry = ExportEntrySpec::Poly(ExportPolySpec {
                            trait_path: Some(trait_path.clone()),
                            method,
                            decl_sig: method_item.sig.clone(),
                            key_types,
                            handle_id_specs: method_handle_id_specs,
                            unsafe_name: None,
                            unsafe_no_mangle: false,
                        });
                        apply_exports_entry_attrs(&mut entry, &combined_attrs)?;
                        methods.push(entry);
                    }
                    other => {
                        return Err(syn::Error::new_spanned(
                            other,
                            "trait export entries only support method signatures; associated types, consts, and macros are not supported",
                        ));
                    }
                }
            }
            continue;
        }

        let is_fn_decl = {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Visibility>()?;
            ahead.peek(syn::Token![fn]) || ahead.peek(syn::Token![unsafe])
        };
        if is_fn_decl {
            let decl_fn = input.parse::<DeclFunction>()?;
            ensure_no_handle_arg_attrs(&decl_fn.sig)?;
            let sig = decl_fn.sig;
            if sig.receiver().is_some() {
                return Err(syn::Error::new_spanned(
                    &sig,
                    "free function export entries cannot declare a receiver",
                ));
            }
            let ident = sig.ident.clone();
            let fn_path: syn::Path = syn::parse_quote!(#ident);
            let mut entry = ExportEntrySpec::Function(ExportFunctionSpec {
                fn_path,
                decl_sig: Some(sig),
                unsafe_name: None,
                unsafe_no_mangle: false,
            });
            apply_exports_entry_attrs(&mut entry, &entry_attrs)?;
            methods.push(entry);
            continue;
        }

        return Err(syn::Error::new(
            input.span(),
            "export entries must be `fn ...;`, `trait ... { ... }`, or `impl ... { ... }`",
        ));
    }

    Ok(methods)
}

fn emit_selected_exports(
    abi: syn::Abi,
    methods: Vec<ExportEntrySpec>,
    empty_err: &str,
) -> TokenStream {
    let mut emitter = Emitter::new();

    if methods.is_empty() {
        emit!(emitter, proc_macro2::Span::call_site(), "{}", empty_err);
        return emitter.finish_token_stream();
    }

    let export_args = CarbonateArgs {
        export_abi: Some(abi),
        export_methods: Some(methods),
    };
    let generated = gen_selected_exports_block(&mut emitter, &export_args);
    emitter.finish_token_stream_with(generated)
}

#[manyhow]
#[proc_macro]
pub fn export_(input: TokenStream) -> TokenStream {
    struct ExportInput {
        attrs: Vec<syn::Attribute>,
        methods: Vec<ExportEntrySpec>,
    }

    impl syn::parse::Parse for ExportInput {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let attrs = input.call(syn::Attribute::parse_inner)?;
            let methods = parse_export_entries(input)?;
            Ok(Self { attrs, methods })
        }
    }

    let input = match syn::parse2::<ExportInput>(input) {
        Ok(input) => input,
        Err(err) => return err.to_compile_error(),
    };
    let abi = match parse_inner_abi(&input.attrs) {
        Ok(Some(abi)) => abi,
        Ok(None) => {
            let mut emitter = Emitter::new();
            emit!(
                emitter,
                proc_macro2::Span::call_site(),
                "export_! requires `#![abi = \"...\"]`"
            );
            return emitter.finish_token_stream();
        }
        Err(err) => return err.to_compile_error(),
    };
    emit_selected_exports(
        abi,
        input.methods,
        "export_! requires at least one selected method",
    )
}

#[manyhow]
#[proc_macro]
#[allow(non_snake_case)]
pub fn export_C(input: TokenStream) -> TokenStream {
    let delegated = quote! {
        #![abi = "C"]
        #input
    };
    export_(delegated.into()).into()
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
    Inherent(TokenStream),
    Trait(TokenStream, TokenStream),
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

        let mut header = TokenStream::new();
        while !input.peek(syn::token::Brace) {
            let tt: proc_macro2::TokenTree = input.parse()?;
            header.extend(std::iter::once(tt));
        }

        let target = parse_impl_target_header(header)?;

        let content;
        syn::braced!(content in input);
        let mut items = Vec::new();
        while !content.is_empty() {
            items.push(content.parse::<ExternCImplItemDecl>()?);
        }
        Ok(Self {
            attrs,
            target,
            items,
        })
    }
}

fn parse_impl_target_header(header: TokenStream) -> syn::Result<ExternCImplTarget> {
    let mut trait_tokens = TokenStream::new();
    let mut self_tokens = TokenStream::new();
    let mut saw_for = false;
    let mut angle_depth = 0usize;

    for tt in header {
        if !saw_for {
            let is_top_level_for = angle_depth == 0
                && matches!(
                    &tt,
                    proc_macro2::TokenTree::Ident(ident) if ident == "for"
                );
            if is_top_level_for {
                saw_for = true;
                continue;
            }
            if let proc_macro2::TokenTree::Punct(punct) = &tt {
                match punct.as_char() {
                    '<' => angle_depth += 1,
                    '>' if angle_depth > 0 => angle_depth -= 1,
                    _ => {}
                }
            }
            trait_tokens.extend(std::iter::once(tt));
        } else {
            self_tokens.extend(std::iter::once(tt));
        }
    }

    if saw_for {
        if trait_tokens.is_empty() || self_tokens.is_empty() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "expected `impl Trait for Type { ... }`",
            ));
        }
        Ok(ExternCImplTarget::Trait(trait_tokens, self_tokens))
    } else {
        if trait_tokens.is_empty() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "expected `impl Type { ... }`",
            ));
        }
        Ok(ExternCImplTarget::Inherent(trait_tokens))
    }
}

struct ExternCFnDecl {
    attrs: Vec<syn::Attribute>,
    vis: syn::Visibility,
    sig: syn::Signature,
}

impl syn::parse::Parse for ExternCFnDecl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let item = input.parse::<syn::ForeignItemFn>()?;
        Ok(Self {
            attrs: item.attrs,
            vis: item.vis,
            sig: item.sig,
        })
    }
}

struct ExternCTypeDecl {
    attrs: Vec<syn::Attribute>,
    vis: syn::Visibility,
    ident: syn::Ident,
    generics: syn::Generics,
}

impl syn::parse::Parse for ExternCTypeDecl {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let vis = input.parse::<syn::Visibility>()?;
        input.parse::<syn::Token![type]>()?;
        let ident = input.parse::<syn::Ident>()?;
        let generics = input.parse::<syn::Generics>()?;
        input.parse::<syn::Token![;]>()?;

        Ok(Self {
            attrs,
            vis,
            ident,
            generics,
        })
    }
}

enum ExternCDecl {
    Impl(ExternCImplDecl),
    Type(ExternCTypeDecl),
    Fn(ExternCFnDecl),
}

struct ExternCDecls(Vec<ExternCDecl>);

impl syn::parse::Parse for ExternCDecls {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut decls = Vec::new();
        while !input.is_empty() {
            let ahead = input.fork();
            let _ = ahead.call(syn::Attribute::parse_outer)?;
            let _ = ahead.parse::<syn::Visibility>()?;
            if ahead.peek(syn::Token![impl]) {
                decls.push(ExternCDecl::Impl(input.parse::<ExternCImplDecl>()?));
            } else if ahead.peek(syn::Token![type]) {
                decls.push(ExternCDecl::Type(input.parse::<ExternCTypeDecl>()?));
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
    fn is_bare_dispatch_attr(attr: &syn::Attribute) -> bool {
        attr.path().is_ident("dispatch") && matches!(attr.meta, syn::Meta::Path(_))
    }
    fn impl_trait_is_drop(trait_tokens: &TokenStream) -> bool {
        let tokens: Vec<_> = trait_tokens.clone().into_iter().collect();
        tokens
            .last()
            .is_some_and(|tt| matches!(tt, proc_macro2::TokenTree::Ident(ident) if ident == "Drop"))
    }

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
            ExternCDecl::Type(decl) => {
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "link attributes are only supported on imported function declarations",
                        ));
                    }

                    if attr.path().is_ident("link_name") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "type-level `#[link_name]` is not supported; use method-level `#[link_name = \"...\"]` or macro-level `#![link(crate = \"...\")]`",
                        ));
                    }
                }
            }
            ExternCDecl::Impl(decl) => {
                let drop_trait_impl = matches!(
                    &decl.target,
                    ExternCImplTarget::Trait(trait_tokens, _) if impl_trait_is_drop(trait_tokens)
                );
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() || attr.path().is_ident("link_name") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "impl-level link attributes are not supported; use method-level `#[link_name = \"...\"]` or macro-level `#![link(...)]`",
                        ));
                    }
                    if drop_trait_impl
                        && attr.path().is_ident("dispatch")
                        && !is_bare_dispatch_attr(attr)
                    {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`impl Drop` only supports bare `#[dispatch]` (without type mappings)",
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

fn validate_extern_type_drop_requirements(decls: &[ExternCDecl]) -> Result<(), syn::Error> {
    fn type_ctor_ident(ty: &syn::Type) -> Option<&syn::Ident> {
        let syn::Type::Path(type_path) = ty else {
            return None;
        };
        type_path.path.segments.last().map(|seg| &seg.ident)
    }

    let mut declared_types: Vec<&ExternCTypeDecl> = Vec::new();
    for decl in decls {
        if let ExternCDecl::Type(ty_decl) = decl {
            declared_types.push(ty_decl);
        }
    }
    if declared_types.is_empty() {
        return Ok(());
    }

    let mut drop_ctors = std::collections::BTreeSet::<String>::new();
    for decl in decls {
        let ExternCDecl::Impl(impl_decl) = decl else {
            continue;
        };
        let ExternCImplTarget::Trait(trait_tokens, self_tokens) = &impl_decl.target else {
            continue;
        };
        let Ok(item_impl) = syn::parse2::<syn::ItemImpl>(quote! {
            impl #trait_tokens for #self_tokens {}
        }) else {
            continue;
        };
        let Some((_, trait_path, _)) = &item_impl.trait_ else {
            continue;
        };
        if path_symbol_name(trait_path) != "Drop" {
            continue;
        }
        let self_ty = item_impl.self_ty.as_ref().clone();
        let Some(ident) = type_ctor_ident(&self_ty) else {
            continue;
        };
        drop_ctors.insert(ident.to_string());
    }

    for ty_decl in declared_types {
        if !drop_ctors.contains(&ty_decl.ident.to_string()) {
            return Err(syn::Error::new_spanned(
                &ty_decl.ident,
                "extern_C! type declarations must include a corresponding `impl Drop for Type { ... }` declaration",
            ));
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
    fn has_bare_dispatch_marker(attrs: &[syn::Attribute]) -> bool {
        attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch") && matches!(attr.meta, syn::Meta::Path(_)))
    }
    fn add_dispatch_attr_to_receiver(receiver: &mut syn::Receiver) {
        if receiver
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch"))
        {
            return;
        }
        receiver.attrs.push(syn::parse_quote!(#[dispatch]));
    }
    fn add_dispatch_attr_to_arg(arg: &mut syn::PatType) {
        if arg
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch"))
        {
            return;
        }
        arg.attrs.push(syn::parse_quote!(#[dispatch]));
    }
    fn base_type(ty: &syn::Type) -> &syn::Type {
        if let syn::Type::Reference(reference) = ty {
            return &reference.elem;
        }
        ty
    }
    fn is_self_type(ty: &syn::Type) -> bool {
        matches!(
            ty,
            syn::Type::Path(syn::TypePath { qself: None, path }) if path.is_ident("Self")
        )
    }
    fn dispatch_type_keys_from_map(
        handle_map: &BTreeMap<String, Vec<syn::Type>>,
    ) -> BTreeSet<String> {
        handle_map
            .values()
            .flat_map(|types| types.iter())
            .map(|ty| quote!(#ty).to_string())
            .collect()
    }
    fn annotate_dispatch_args(
        item_impl: &mut syn::ItemImpl,
        handle_map: Option<&BTreeMap<String, Vec<syn::Type>>>,
    ) {
        let trait_is_drop = item_impl
            .trait_
            .as_ref()
            .is_some_and(|(_, path, _)| path_symbol_name(path) == "Drop");
        let dispatch_type_keys = handle_map.map(dispatch_type_keys_from_map);

        for item in &mut item_impl.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };

            for input in &mut method.sig.inputs {
                match input {
                    syn::FnArg::Receiver(receiver) => {
                        let is_drop_receiver = trait_is_drop
                            && method.sig.ident == "drop"
                            && receiver.reference.is_some()
                            && receiver.mutability.is_some();
                        if handle_map.is_some() || is_drop_receiver {
                            add_dispatch_attr_to_receiver(receiver);
                        }
                    }
                    syn::FnArg::Typed(arg) => {
                        let ty = base_type(arg.ty.as_ref());
                        let should_dispatch = if let Some(keys) = &dispatch_type_keys {
                            is_self_type(ty) || keys.contains(&quote!(#ty).to_string())
                        } else {
                            false
                        };
                        if should_dispatch {
                            add_dispatch_attr_to_arg(arg);
                        }
                    }
                }
            }
        }
    }
    fn type_ctor_ident(ty: &syn::Type) -> Option<&syn::Ident> {
        let syn::Type::Path(type_path) = ty else {
            return None;
        };
        type_path.path.segments.last().map(|seg| &seg.ident)
    }
    fn has_non_lifetime_generics(generics: &syn::Generics) -> bool {
        generics
            .params
            .iter()
            .any(|p| !matches!(p, syn::GenericParam::Lifetime(_)))
    }
    fn impl_header_has_generics(trait_tokens: &TokenStream) -> bool {
        trait_tokens.clone().into_iter().any(|tt| match tt {
            proc_macro2::TokenTree::Punct(p) => p.as_char() == '<',
            _ => false,
        })
    }
    fn impl_trait_is_drop(trait_tokens: &TokenStream) -> bool {
        let tokens: Vec<_> = trait_tokens.clone().into_iter().collect();
        tokens
            .last()
            .is_some_and(|tt| matches!(tt, proc_macro2::TokenTree::Ident(ident) if ident == "Drop"))
    }
    fn add_handle_bound_for_named_type(name: &syn::Ident, generics: &mut syn::Generics) {
        let cloned_generics = generics.clone();
        let (_, ty_generics, _) = cloned_generics.split_for_impl();
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#name #ty_generics: co3::handle::Handle));
    }
    fn impl_desc_needs_drop_self_handle_bound(
        impl_desc: &ImplDescriptor<'_>,
        impl_has_bare_dispatch: bool,
    ) -> bool {
        if !impl_has_bare_dispatch {
            return false;
        }
        if impl_desc
            .trait_name
            .is_none_or(|trait_name| path_symbol_name(trait_name) != "Drop")
        {
            return false;
        }
        has_non_lifetime_generics(impl_desc.generics)
    }
    fn collect_impl_handle_map(
        attrs: &[syn::Attribute],
        emitter: &mut Emitter,
    ) -> Option<BTreeMap<String, Vec<syn::Type>>> {
        let mut out = None;
        for attr in attrs {
            match parse_entry_handle_map_attr(attr) {
                Ok(Some(map)) => {
                    if out.replace(map).is_some() {
                        emit!(
                            emitter,
                            attr,
                            "`handle` mapping can only be provided once per impl entry"
                        );
                    }
                }
                Ok(None) => {}
                Err(err) => emit!(emitter, attr, "{}", err),
            }
        }
        out
    }
    fn type_is_ident(ty: &syn::Type, ident: &syn::Ident) -> bool {
        let syn::Type::Path(type_path) = ty else {
            return false;
        };
        type_path.qself.is_none()
            && type_path.path.segments.len() == 1
            && type_path.path.segments[0].ident == *ident
    }
    fn monomorphize_foreign_impl(
        item_impl: &syn::ItemImpl,
        handle_map: &BTreeMap<String, Vec<syn::Type>>,
        emitter: &mut Emitter,
    ) -> Option<Vec<syn::ItemImpl>> {
        fn infer_param_variants_from_self_map(
            self_ty: &syn::Type,
            param_ident: &syn::Ident,
            self_variants: &[syn::Type],
        ) -> Option<Vec<syn::Type>> {
            let syn::Type::Path(self_path) = self_ty else {
                return None;
            };
            let self_seg = self_path.path.segments.last()?;
            let syn::PathArguments::AngleBracketed(self_args) = &self_seg.arguments else {
                return None;
            };
            let param_pos = self_args.args.iter().position(|arg| {
                matches!(
                    arg,
                    syn::GenericArgument::Type(syn::Type::Path(type_path))
                        if type_path.qself.is_none()
                            && type_path.path.segments.len() == 1
                            && type_path.path.segments[0].ident == *param_ident
                )
            })?;

            let mut out = Vec::with_capacity(self_variants.len());
            for variant in self_variants {
                let syn::Type::Path(variant_path) = variant else {
                    return None;
                };
                let variant_seg = variant_path.path.segments.last()?;
                if variant_seg.ident != self_seg.ident {
                    return None;
                }
                let syn::PathArguments::AngleBracketed(variant_args) = &variant_seg.arguments
                else {
                    return None;
                };
                let arg = variant_args.args.iter().nth(param_pos)?;
                let syn::GenericArgument::Type(ty) = arg else {
                    return None;
                };
                out.push(ty.clone());
            }
            Some(out)
        }

        let mut type_params = Vec::<syn::Ident>::new();
        for param in &item_impl.generics.params {
            match param {
                syn::GenericParam::Type(type_param) => type_params.push(type_param.ident.clone()),
                syn::GenericParam::Lifetime(_) => {}
                syn::GenericParam::Const(const_param) => {
                    emit!(
                        emitter,
                        const_param,
                        "Const generics on extern impl blocks are not supported"
                    );
                    return None;
                }
            }
        }
        if type_params.is_empty() {
            return Some(vec![item_impl.clone()]);
        }

        let mut param_variants = BTreeMap::<String, Vec<syn::Type>>::new();
        for ident in &type_params {
            let key = ident.to_string();
            if let Some(types) = handle_map.get(&key) {
                param_variants.insert(key, types.clone());
                continue;
            }
            if let Some(types) = handle_map.get("Self")
                && type_is_ident(item_impl.self_ty.as_ref(), ident)
            {
                param_variants.insert(key, types.clone());
                continue;
            }
            if let Some(self_types) = handle_map.get("Self")
                && let Some(inferred) = infer_param_variants_from_self_map(
                    item_impl.self_ty.as_ref(),
                    ident,
                    self_types,
                )
            {
                param_variants.insert(key, inferred);
                continue;
            }
            emit!(
                emitter,
                ident,
                "generic extern impl parameter `{}` requires `#[dispatch({} = [Type, ...])]` mapping or `Self` mapping when used as the self type",
                ident,
                ident
            );
            return None;
        }

        let mut arms = 1usize;
        for types in param_variants.values() {
            if types.is_empty() {
                emit!(
                    emitter,
                    item_impl,
                    "handle mapping lists for generic extern impls may not be empty"
                );
                return None;
            }
            arms = arms.max(types.len());
        }
        for (param, types) in &param_variants {
            if types.len() != 1 && types.len() != arms {
                emit!(
                    emitter,
                    item_impl,
                    "generic extern impl mapping for `{}` must have either 1 entry or {} entries",
                    param,
                    arms
                );
                return None;
            }
        }

        let mut mono_impls = Vec::<syn::ItemImpl>::new();
        for idx in 0..arms {
            let mut subst = BTreeMap::<String, syn::Type>::new();
            for (param, types) in &param_variants {
                let selected = if types.len() == 1 {
                    types[0].clone()
                } else {
                    types[idx].clone()
                };
                subst.insert(param.clone(), selected);
            }

            struct Rewriter<'a> {
                subst: &'a BTreeMap<String, syn::Type>,
            }
            impl syn::visit_mut::VisitMut for Rewriter<'_> {
                fn visit_type_mut(&mut self, node: &mut syn::Type) {
                    if let syn::Type::Path(type_path) = node
                        && type_path.qself.is_none()
                        && type_path.path.segments.len() == 1
                        && matches!(
                            type_path.path.segments[0].arguments,
                            syn::PathArguments::None
                        )
                    {
                        let key = type_path.path.segments[0].ident.to_string();
                        if let Some(replacement) = self.subst.get(&key) {
                            *node = replacement.clone();
                            return;
                        }
                    }
                    syn::visit_mut::visit_type_mut(self, node);
                }
            }

            let mut monomorphized = item_impl.clone();
            let mut rewriter = Rewriter { subst: &subst };
            syn::visit_mut::VisitMut::visit_item_impl_mut(&mut rewriter, &mut monomorphized);
            monomorphized.generics.params = monomorphized
                .generics
                .params
                .into_iter()
                .filter(|param| matches!(param, syn::GenericParam::Lifetime(_)))
                .collect();
            if monomorphized.generics.params.is_empty() {
                monomorphized.generics.lt_token = None;
                monomorphized.generics.gt_token = None;
            }
            mono_impls.push(monomorphized);
        }
        Some(mono_impls)
    }

    let mut emitter = Emitter::new();
    let import_prefix = default_link_prefix.map(|link_prefix| quote!(concat!(#link_prefix, "_")));
    let mut out = Vec::new();
    let mut type_names_with_drop_handle_bound = BTreeSet::<String>::new();
    for decl in &decls {
        let ExternCDecl::Impl(impl_decl) = decl else {
            continue;
        };
        let ExternCImplTarget::Trait(trait_tokens, self_tokens) = &impl_decl.target else {
            continue;
        };
        if !impl_trait_is_drop(trait_tokens) {
            continue;
        }
        if !impl_header_has_generics(trait_tokens) {
            continue;
        }
        let Ok(self_ty) = syn::parse2::<syn::Type>(self_tokens.clone()) else {
            continue;
        };
        let Some(type_ident) = type_ctor_ident(&self_ty) else {
            continue;
        };
        if has_bare_dispatch_marker(&impl_decl.attrs) {
            type_names_with_drop_handle_bound.insert(type_ident.to_string());
        }
    }

    for decl in decls {
        match decl {
            ExternCDecl::Impl(decl) => {
                let impl_dispatch_map = collect_impl_handle_map(&decl.attrs, &mut emitter);
                let impl_has_bare_dispatch = has_bare_dispatch_marker(&decl.attrs);
                let items = decl.items.iter().map(|item| match item {
                    ExternCImplItemDecl::Method(m) => {
                        let attrs: Vec<_> = m
                            .attrs
                            .iter()
                            .filter(|attr| parse_link_attr(attr).ok().flatten().is_none())
                            .collect();
                        let vis = &m.vis;
                        let sig = m.sig.clone();
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

                let has_non_lifetime_generics = item_impl
                    .generics
                    .params
                    .iter()
                    .any(|p| !matches!(p, syn::GenericParam::Lifetime(_)));
                let item_impls = if has_non_lifetime_generics {
                    if let Some(handle_map) = impl_dispatch_map.clone() {
                        let Some(monomorphized) =
                            monomorphize_foreign_impl(&item_impl, &handle_map, &mut emitter)
                        else {
                            continue;
                        };
                        monomorphized
                    } else {
                        vec![item_impl]
                    }
                } else {
                    vec![item_impl]
                };

                for mut item_impl in item_impls {
                    annotate_dispatch_args(&mut item_impl, impl_dispatch_map.as_ref());
                    let Some(impl_desc) =
                        ImplDescriptor::from_foreign_impl(&mut emitter, &item_impl)
                    else {
                        continue;
                    };
                    if impl_desc.fns.is_empty() {
                        emit!(
                            emitter,
                            item_impl,
                            "extern impl declarations must include at least one method"
                        );
                        continue;
                    }

                    let wrapped_methods = impl_desc
                        .fns
                        .iter()
                        .map(|fn_| {
                            let mut method_link_name: Option<syn::LitStr> = None;
                            let mut method_handle_id_specs: Vec<HandleIdSpec> = Vec::new();
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
                                match parse_handle_id_attr(attr) {
                                    Ok(Some(mut specs)) => {
                                        method_handle_id_specs.append(&mut specs)
                                    }
                                    Ok(None) => {}
                                    Err(err) => emit!(emitter, attr, "{}", err),
                                }
                            }

                            let method_import_name = method_link_name.as_ref();
                            let method_handle_id_specs = if impl_desc
                                .trait_name
                                .is_some_and(|trait_name| path_symbol_name(trait_name) == "Drop")
                                && fn_.sig.ident == "drop"
                                && !impl_has_bare_dispatch
                            {
                                Vec::new()
                            } else {
                                infer_default_handle_id_specs(fn_, &method_handle_id_specs)
                            };
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
                                &method_handle_id_specs,
                            )
                        })
                        .collect::<Vec<_>>();

                    let self_ty = &impl_desc.fns[0].self_ty;
                    let impl_trait_for = impl_desc
                        .trait_name
                        .map(|trait_name| quote! { #trait_name for });
                    let (associated_names, associated_types) =
                        impl_desc.associated_types.iter().fold(
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
                    let mut generics = impl_desc.generics.clone();
                    if impl_desc_needs_drop_self_handle_bound(&impl_desc, impl_has_bare_dispatch) {
                        let self_ty = impl_desc.fns[0]
                            .self_ty
                            .as_ref()
                            .expect("impl methods must have resolved self type");
                        let self_ty: syn::Type = syn::parse_quote!(#self_ty);
                        generics
                            .make_where_clause()
                            .predicates
                            .push(syn::parse_quote!(#self_ty: co3::handle::Handle));
                    }
                    let (impl_generics, _, where_clause) = generics.split_for_impl();

                    out.push(quote! {
                        impl #impl_generics #impl_trait_for #self_ty #where_clause {
                            #(type #associated_names = #associated_types;)*
                            #(const #associated_const_names: #associated_const_types = #associated_const_values;)*
                            #(#wrapped_methods)*
                        }
                    });
                }
            }
            ExternCDecl::Type(decl) => {
                let type_attrs = decl.attrs.clone();
                let vis = &decl.vis;
                let ident = &decl.ident;
                let mut generics = decl.generics.clone();
                if type_names_with_drop_handle_bound.contains(&ident.to_string()) {
                    add_handle_bound_for_named_type(ident, &mut generics);
                }
                let (decl_generics, _, decl_where_clause) = generics.split_for_impl();

                let Some(item) = emitter.handle(syn::parse2::<syn::DeriveInput>(quote! {
                    #(#module_attrs)*
                    #(#type_attrs)*
                    #vis struct #ident #decl_generics #decl_where_clause;
                })) else {
                    continue;
                };
                let Some(input) = emitter.handle(FfiTypeInput::from_derive_input(&item)) else {
                    continue;
                };

                let link_mode = if let Some(link_prefix) = default_link_prefix {
                    ExternTypeLinkMode::LinkCrate(gen_export_link_prefix(link_prefix.clone()))
                } else {
                    ExternTypeLinkMode::LinkCrate(quote!(concat!(env!("CARGO_CRATE_NAME"), "_")))
                };
                out.push(wrapper::wrap_as_opaque(&mut emitter, input, &link_mode));
            }
            ExternCDecl::Fn(decl) => {
                let fn_attrs: Vec<_> = decl
                    .attrs
                    .iter()
                    .filter(|attr| parse_link_attr(attr).ok().flatten().is_none())
                    .collect();
                let vis = &decl.vis;
                let sig = decl.sig.clone();
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
                    &[],
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
    if let Err(err) = validate_extern_type_drop_requirements(&input.decls.0) {
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
