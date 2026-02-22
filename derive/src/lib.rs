//! Crate containing FFI related macro functionality
use darling::FromDeriveInput;
use impl_visitor::{FnDescriptor, ImplDescriptor};
use manyhow::{emit, manyhow};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use wrapper::{ExternTypeLinkMode, ExternTypeSymbolOverride, wrap_method};

#[cfg(feature = "getset")]
use crate::{attr_parse::derive::Derive, extern_c::FfiTypeData};
use crate::{
    emitter::Emitter,
    extern_c::{FfiTypeInput, FfiTypeKindAttribute, derive_extern_c},
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
/// `#[co3::extern_type]` derived wrappers.
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
struct DecarbonateArgs {
    link_crate: Option<syn::LitStr>,
    link_name: Option<syn::LitStr>,
}

fn is_decarbonate_attr(attr: &syn::Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "decarbonate")
}

fn merge_decarbonate_args(
    emitter: &mut Emitter,
    into: &mut DecarbonateArgs,
    attr: &syn::Attribute,
    parsed: DecarbonateArgs,
) {
    if let Some(link_crate) = parsed.link_crate {
        if into.link_crate.is_some() {
            emit!(
                emitter,
                attr,
                "`link_crate` can only be provided once across decarbonate attributes"
            );
        } else {
            into.link_crate = Some(link_crate);
        }
    }

    if let Some(link_name) = parsed.link_name {
        if into.link_name.is_some() {
            emit!(
                emitter,
                attr,
                "`link_name` can only be provided once across decarbonate attributes"
            );
        } else {
            into.link_name = Some(link_name);
        }
    }
}

impl syn::parse::Parse for DecarbonateArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self::default());
        }

        let mut args = Self::default();

        while !input.is_empty() {
            let key: syn::Path = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let value = input.parse::<syn::LitStr>()?;

            if key.is_ident("link_crate") || key.is_ident("crate") {
                if args.link_crate.is_some() {
                    return Err(syn::Error::new(
                        key.segments[0].ident.span(),
                        "`link_crate` can only be provided once",
                    ));
                }
                args.link_crate = Some(value);
            } else if key.is_ident("link_name") || key.is_ident("link") {
                if args.link_name.is_some() {
                    return Err(syn::Error::new(
                        key.segments[0].ident.span(),
                        "`link_name` can only be provided once",
                    ));
                }
                args.link_name = Some(value);
            } else {
                return Err(syn::Error::new(
                    key.segments[0].ident.span(),
                    "expected `link_crate = \"...\"` or `link_name = \"...\"`",
                ));
            }

            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else if !input.is_empty() {
                return Err(input.error("expected `,` between decarbonate arguments"));
            }
        }

        Ok(args)
    }
}

fn decarbonate_import_prefix(args: &DecarbonateArgs) -> Option<TokenStream> {
    if let Some(link_crate) = &args.link_crate {
        return Some(quote!(concat!(#link_crate, "_")));
    }
    None
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

enum ExternTypeAttrArgs {
    LinkCrate(syn::LitStr),
    Symbols(Vec<ExternTypeSymbolOverride>),
}

impl syn::parse::Parse for ExternTypeAttrArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(
                input
                    .error("expected `link_crate = \"...\"` or `Trait::method = \"symbol\"` entries")
            );
        }

        let mut link_crate: Option<syn::LitStr> = None;
        let mut symbols = Vec::new();

        while !input.is_empty() {
            let key: syn::Path = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let value = input.parse::<syn::LitStr>()?;

            if key.is_ident("link_crate") {
                if link_crate.is_some() {
                    return Err(syn::Error::new(
                        key.segments[0].ident.span(),
                        "`link_crate` can only be provided once",
                    ));
                }
                let mut pref = value.value();
                if !pref.ends_with('_') {
                    pref.push('_');
                }
                link_crate = Some(syn::LitStr::new(&pref, value.span()));
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

        match (link_crate, symbols.is_empty()) {
            (Some(link_crate), true) => Ok(Self::LinkCrate(link_crate)),
            (Some(_), false) => {
                Err(input.error(
                    "`link_crate = \"...\"` cannot be mixed with explicit symbol mappings",
                ))
            }
            (None, false) => Ok(Self::Symbols(symbols)),
            (None, true) => Err(input.error(
                "expected `link_crate = \"...\"` or at least one `Trait::method = \"symbol\"` mapping",
            )),
        }
    }
}

fn gen_export_link_crate(link_crate: syn::LitStr) -> TokenStream {
    let mut pref = link_crate.value();
    if !pref.ends_with('_') {
        pref.push('_');
    }
    let link_crate = syn::LitStr::new(&pref, link_crate.span());
    quote!(#link_crate)
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
        ExternTypeAttrArgs::LinkCrate(link_crate) => {
            ExternTypeLinkMode::LinkCrate(gen_export_link_crate(link_crate.clone()))
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
                    #[derive(co3::ExternC)]
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
                let ExternTypeLinkMode::LinkCrate(link_crate) = &link_mode else {
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
                            Some(link_crate),
                            None,
                        )
                    })
                    .collect();

                let impl_block = wrapper::wrap_impl_items(
                    &ImplDescriptor {
                        attrs: Vec::new(),
                        trait_name: None,
                        associated_types: Vec::new(),
                        generics: &Default::default(),
                        fns: derived_methods,
                    },
                    Some(link_crate),
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
/// - `#[co3::extern_type(link_crate = "other")]` sets shared import/export symbols to `"other_{Trait}_{method}"`.
/// - `#[co3::extern_type(Trait::method = "symbol", ...)]` sets explicit symbols per shared method.
#[manyhow]
#[proc_macro_attribute]
pub fn extern_type(args: TokenStream, input: TokenStream) -> TokenStream {
    extern_type_impl(args, input)
}

// TODO: mineral(`local`) is a workaround for https://github.com/rust-lang/rust/issues/48214
// because some derived types cannot derive `NonLocal` othwerise. Should be removed in future
/// Derive implementations of traits required to convert to and from an FFI-compatible type
///
/// # Attributes
///
/// * `#[mineral(opaque)]`
/// serialize the type as opaque. If automatically derived type doesn't work just
/// attach this attribute and force the type to be serialized as opaque across FFI
///
/// * `#[mineral(NICHE_VALUE = <expr>, unsafe(is_valid = |target| ...))]`
/// customize [`co3::niche::Niche`] value and validation function for `#[repr(transparent)]` types.
/// `NICHE_VALUE` can be ommitted in which case the implementation delegates to the wrapped type.
///
/// # Safety
///
/// `is_valid` must not return false positives
///
/// Check [`co3::transmute::CheckedTransmute`] or [`co3::mineral`] for more details
///
/// * `#[mineral(local)]`
/// marks the type as local, meaning it contains references to the local frame. If a type
/// contains references to the local frame you won't be able to return it from an FFI function
/// because the frame is destroyed on function return which would invalidate your type's references.
///
/// Only applicable to data-carrying enums.
///
/// NOTE: This attribute is likely to be removed in future versions
///
/// * `#[mineral(unsafe(non_owning))]`
/// when a type contains a raw pointer (e.g. `*const T`/*mut T`) it's not possible to figure out
/// whether it carries ownership of the data pointed to. Place this attribute on the field to
/// indicate pointer doesn't own the data and is robust in the type. Alternatively, if the type
/// is carrying ownership mark entire type as opaque with `#[mineral(opaque)]`. If the type
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
#[proc_macro_derive(ExternC, attributes(mineral))]
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
/// use getset::Getters;
///
/// // For a struct such as:
/// #[co3::carbonate]
/// #[derive(co3::ExternC, Clone, Getters)]
/// #[getset(get = "pub")]
/// pub struct Foo {
///     /// Id of the struct
///     id: u8,
///     #[getset(skip)]
///     bar: Vec<u8>,
/// }
///
/// #[co3::carbonate]
/// impl Foo {
///     /// Construct new type
///     pub fn new(id: u8) -> Self {
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
pub fn carbonate(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut emitter = Emitter::new();

    let item = match syn::parse2::<syn::Item>(item) {
        Err(err) => return err.to_compile_error(),
        Ok(item) => item,
    };

    if !attr.is_empty() {
        emit!(emitter, item, "Unknown tokens in the attribute");
    }

    use syn::Item::*;
    let result = match item {
        Impl(mut item) => {
            let Some(impl_descriptor) = ImplDescriptor::from_impl(&mut emitter, &item) else {
                return emitter.finish_token_stream();
            };

            let ffi_fns: Vec<_> = impl_descriptor
                .fns
                .iter()
                .map(|fn_| {
                    ffi_fn::gen_definition(
                        fn_,
                        impl_descriptor.trait_name,
                        impl_descriptor.generics,
                    )
                })
                .collect();

            for (idx, ffi_fn) in ffi_fns.into_iter().enumerate() {
                if let Some(syn::ImplItem::Fn(method)) = item.items.get_mut(idx) {
                    method.block.stmts.insert(0, syn::parse_quote! { #ffi_fn });
                }
            }

            quote! { #item }
        }
        Fn(mut item) => {
            let Some(fn_descriptor) = FnDescriptor::from_fn(&mut emitter, &item) else {
                return emitter.finish_token_stream();
            };
            let ffi_fn = ffi_fn::gen_definition(&fn_descriptor, None, &Default::default());
            item.block.stmts.insert(0, syn::parse_quote! { #ffi_fn });

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
                .map(|fn_| ffi_fn::gen_definition(&fn_, None, &Default::default()));

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

/// Replace the function's body with a call to FFI function. Counterpart of [`carbonate`]
///
/// When placed on a structure, it integrates with [`getset`] to import derived getter/setter methods.
///
/// # Example:
/// ```rust
/// #[co3::decarbonate]
/// pub fn return_first_elem_from_arr(arr: &[u8; 8]) -> &u8 {
///     // The body of this function is replaced with something like the following:
///     // let mut store = Default::default();
///     // let arr = co3::Encode::encode(&arr, &mut store);
///     // let output = MaybeUninit::uninit();
///     //
///     // let call_res = __return_first_elem_from_arr(arr, output.as_mut_ptr());
///     // if co3::FfiReturn::Ok != call_res {
///     //     panic!("Function call failed");
///     // }
///     //
///     // co3::out_ptr::OutPtrRead::try_read_out(output.assume_init()).unwrap()
/// }
///
/// /* The following functions will be declared:
/// unsafe extern "C" {
///     fn __return_first_elem_from_arr(arr: *const [u8; 8]) -> *const u8;
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
///
/// Optional arguments:
/// - `#[co3::decarbonate]` imports by the literal function name in the generated signature.
/// - `#[co3::decarbonate(link_crate = "crate")]` uses `"crate_"` as symbol prefix.
/// - `#[co3::decarbonate(link_name = "symbol")]` sets an exact imported symbol.
/// - Resolution is always `"{link_crate_?}{fn_ident}"` unless `link_name` is set.
#[manyhow]
#[proc_macro_attribute]
pub fn decarbonate(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut args = match syn::parse2::<DecarbonateArgs>(attr) {
        Err(err) => return err.to_compile_error(),
        Ok(args) => args,
    };

    let item = match syn::parse2::<syn::Item>(item) {
        Err(err) => return err.to_compile_error(),
        Ok(item) => item,
    };

    let mut emitter = Emitter::new();

    use syn::Item::*;
    let result = match item {
        Impl(item) => {
            for attr in &item.attrs {
                if !is_decarbonate_attr(attr) {
                    continue;
                }
                match attr.parse_args::<DecarbonateArgs>() {
                    Ok(parsed) => merge_decarbonate_args(&mut emitter, &mut args, attr, parsed),
                    Err(err) => emit!(emitter, attr, "{}", err),
                }
            }

            let import_prefix = decarbonate_import_prefix(&args);
            if args.link_name.is_some() {
                emit!(
                    emitter,
                    item,
                    "Impl-level `decarbonate` does not support `link_name`; use method-level overrides"
                );
            }
            let attrs: Vec<_> = item
                .attrs
                .iter()
                .filter(|attr| !is_decarbonate_attr(attr))
                .collect();

            let Some(impl_desc) = ImplDescriptor::from_foreign_impl(&mut emitter, &item) else {
                return emitter.finish_token_stream();
            };
            let wrapped_items = wrapper::wrap_impl_items(&impl_desc, import_prefix.as_ref());
            let ffi_fns = impl_desc
                .fns
                .iter()
                .map(|fn_| {
                    let mut method_args = DecarbonateArgs::default();
                    let mut method_link_name: Option<syn::LitStr> = None;

                    for attr in &fn_.attrs {
                        if is_decarbonate_attr(attr) {
                            match attr.parse_args::<DecarbonateArgs>() {
                                Ok(parsed) => {
                                    if parsed.link_crate.is_some() {
                                        emit!(
                                            emitter,
                                            attr,
                                            "Method-level `decarbonate` only supports `link_name = \"...\"`"
                                        );
                                    }
                                    merge_decarbonate_args(
                                        &mut emitter,
                                        &mut method_args,
                                        attr,
                                        parsed,
                                    );
                                }
                                Err(err) => emit!(emitter, attr, "{}", err),
                            }
                            continue;
                        }

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

                    let method_exact = method_link_name.or(method_args.link_name.clone());
                    let method_prefix = if method_exact.is_some() {
                        None
                    } else {
                        import_prefix.as_ref()
                    };
                    let method_import_name = method_exact.as_ref();

                    ffi_fn::gen_declaration(
                        impl_desc.generics,
                        fn_,
                        impl_desc.trait_name,
                        method_prefix,
                        method_import_name,
                    )
                })
                .collect::<Vec<_>>();

            quote! {
                #(#attrs)*
                #wrapped_items
                #(#ffi_fns)*
            }
        }
        Fn(item) => {
            for attr in &item.attrs {
                if !is_decarbonate_attr(attr) {
                    continue;
                }
                match attr.parse_args::<DecarbonateArgs>() {
                    Ok(parsed) => merge_decarbonate_args(&mut emitter, &mut args, attr, parsed),
                    Err(err) => emit!(emitter, attr, "{}", err),
                }
            }
            let mut item_link_name: Option<syn::LitStr> = None;
            for attr in &item.attrs {
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

            let import_prefix = decarbonate_import_prefix(&args);
            let import_name = item_link_name.as_ref().or(args.link_name.as_ref());
            let import_prefix = if item_link_name.is_some() || args.link_name.is_some() {
                None
            } else {
                import_prefix
            };

            let Some(fn_descriptor) = FnDescriptor::from_fn(&mut emitter, &item) else {
                return emitter.finish_token_stream();
            };

            let ffi_fn = ffi_fn::gen_declaration(
                &Default::default(),
                &fn_descriptor,
                None,
                import_prefix.as_ref(),
                import_name,
            );
            let wrapped_item = wrap_method(&fn_descriptor, None);

            quote! {
                #wrapped_item
                #ffi_fn
            }
        }
        Struct(item) => quote! { #item },
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

enum ExternCImplTarget {
    Inherent(syn::Type),
    Trait(syn::Path, syn::Type),
}

struct ExternCImplDecl {
    attrs: Vec<syn::Attribute>,
    target: ExternCImplTarget,
    methods: Vec<ExternCMethodDecl>,
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
            let mut methods = Vec::new();
            while !content.is_empty() {
                methods.push(content.parse::<ExternCMethodDecl>()?);
            }
            Ok(Self {
                attrs,
                target: ExternCImplTarget::Trait(trait_path, self_ty),
                methods,
            })
        } else {
            let self_ty = input.parse::<syn::Type>()?;
            let content;
            syn::braced!(content in input);
            let mut methods = Vec::new();
            while !content.is_empty() {
                methods.push(content.parse::<ExternCMethodDecl>()?);
            }
            Ok(Self {
                attrs,
                target: ExternCImplTarget::Inherent(self_ty),
                methods,
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

#[manyhow]
#[proc_macro]
pub fn extern_c(input: TokenStream) -> TokenStream {
    let decls = match syn::parse2::<ExternCDecls>(input) {
        Err(err) => return err.to_compile_error(),
        Ok(decls) => decls.0,
    };

    let mut out = Vec::new();
    for decl in decls {
        match decl {
            ExternCDecl::Impl(decl) => {
                let link_crate = decl.attrs.iter().find_map(|attr| {
                    let syn::Meta::NameValue(nv) = &attr.meta else {
                        return None;
                    };
                    let Some(ident) = nv.path.get_ident() else {
                        return None;
                    };
                    if ident != "link_crate" && ident != "crate" {
                        return None;
                    }
                    let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    else {
                        return None;
                    };
                    Some(s.clone())
                });

                let decarbonate_attr = link_crate
                    .map(|link_crate| quote!(#[co3::decarbonate(link_crate = #link_crate)]))
                    .unwrap_or_else(|| quote!(#[co3::decarbonate]));

                let methods = decl.methods.iter().map(|m| {
                    let attrs = &m.attrs;
                    let vis = &m.vis;
                    let sig = &m.sig;
                    quote! {
                        #(#attrs)*
                        #vis #sig {
                            unreachable!("replaced by co3::decarbonate")
                        }
                    }
                });

                match decl.target {
                    ExternCImplTarget::Inherent(self_ty) => out.push(quote! {
                        #decarbonate_attr
                        impl #self_ty {
                            #(#methods)*
                        }
                    }),
                    ExternCImplTarget::Trait(trait_path, self_ty) => out.push(quote! {
                        #decarbonate_attr
                        impl #trait_path for #self_ty {
                            #(#methods)*
                        }
                    }),
                }
            }
            ExternCDecl::Fn(decl) => {
                let link_crate = decl.attrs.iter().find_map(|attr| {
                    let syn::Meta::NameValue(nv) = &attr.meta else {
                        return None;
                    };
                    let Some(ident) = nv.path.get_ident() else {
                        return None;
                    };
                    if ident != "link_crate" && ident != "crate" {
                        return None;
                    }
                    let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    else {
                        return None;
                    };
                    Some(s.clone())
                });

                let decarbonate_attr = link_crate
                    .map(|link_crate| quote!(#[co3::decarbonate(link_crate = #link_crate)]))
                    .unwrap_or_else(|| quote!(#[co3::decarbonate]));
                let fn_attrs: Vec<_> = decl
                    .attrs
                    .iter()
                    .filter(|attr| {
                        let syn::Meta::NameValue(nv) = &attr.meta else {
                            return true;
                        };
                        let Some(ident) = nv.path.get_ident() else {
                            return true;
                        };
                        ident != "link_crate" && ident != "crate"
                    })
                    .collect();
                let vis = &decl.vis;
                let sig = &decl.sig;

                out.push(quote! {
                    #decarbonate_attr
                    #(#fn_attrs)*
                    #vis #sig {
                        unreachable!("replaced by co3::decarbonate")
                    }
                });
            }
        }
    }

    quote!(#(#out)*)
}
