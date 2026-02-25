use manyhow::emit;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr, Path, Type, visit_mut::VisitMut};

use crate::{
    attr_parse::derive::{Derive, RustcDerive},
    emitter::Emitter,
    extern_c::FfiTypeInput,
    ffi_fn,
    impl_visitor::{Arg, FnDescriptor, TypeImplTraitResolver},
    utils::{gen_resolve_type, gen_store_name, unwrap_result_type},
};

fn add_handle_bound(name: &Ident, generics: &mut syn::Generics) {
    let cloned_generics = generics.clone();
    let (_, ty_generics, _) = cloned_generics.split_for_impl();

    generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote! {
            #name #ty_generics: co3::handle::Handle
        });
}

fn link_name(prefix: &TokenStream, suffix: &str) -> TokenStream {
    let suffix = LitStr::new(suffix, Span::call_site());
    quote!(concat!(#prefix, #suffix))
}

pub enum ExternTypeLinkMode {
    LinkCrate(TokenStream),
}

#[derive(Clone)]
pub struct HandleIdSpec {
    pub selector: syn::Type,
    pub at: usize,
}

fn built_in_symbol(mode: &ExternTypeLinkMode, trait_name: &str, method_name: &str) -> TokenStream {
    match mode {
        ExternTypeLinkMode::LinkCrate(crate_) => {
            link_name(crate_, &format!("{trait_name}_{method_name}"))
        }
    }
}

fn impl_clone_for_opaque(
    name: &Ident,
    generics: &syn::Generics,
    link_name: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Clone for #name #ty_generics #where_clause {
            fn clone(&self) -> Self {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let clone_result = unsafe {
                    unsafe extern "C" {
                        #[link_name = #link_name]
                        fn co3_clone(
                            handle_id: <co3::handle::Id as co3::ExternC>::CType,
                            handle_ptr: *const co3::external::Extern,
                            out_ptr: *mut *mut co3::external::Extern,
                        ) -> co3::FfiReturn;
                    }

                    co3_clone(
                        co3::Encode::encode(handle_id, &mut ()),
                    co3::Encode::encode(self.__co3_as_ref(), &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if clone_result != co3::FfiReturn::Ok  {
                    panic!("Clone returned: {}", clone_result);
                }

                unsafe {co3::Decode::decode(output.assume_init(), &mut ()).expect("Invalid output")}
            }
        }
    }
}

fn impl_default_for_opaque(
    name: &Ident,
    generics: &syn::Generics,
    link_name: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Default for #name #ty_generics #where_clause {
            fn default() -> Self {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let default_result = unsafe {
                    unsafe extern "C" {
                        #[link_name = #link_name]
                        fn co3_default(
                            handle_id: <co3::handle::Id as co3::ExternC>::CType,
                            out_ptr: *mut *mut co3::external::Extern,
                        ) -> co3::FfiReturn;
                    }

                    co3_default(
                        co3::Encode::encode(handle_id, &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if default_result != co3::FfiReturn::Ok  {
                    panic!("Default returned: {}", default_result);
                }

                unsafe {co3::Decode::decode(output.assume_init(), &mut ()).expect("Invalid output")}
            }
        }
    }
}

fn impl_eq_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! { impl #impl_generics Eq for #name #ty_generics #where_clause {} }
}
fn impl_partial_eq_for_opaque(
    name: &Ident,
    generics: &syn::Generics,
    link_name: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics PartialEq for #name #ty_generics #where_clause {
            fn eq(&self, other: &Self) -> bool {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let eq_result = unsafe {
                    unsafe extern "C" {
                        #[link_name = #link_name]
                        fn co3_eq(
                            handle_id: <co3::handle::Id as co3::ExternC>::CType,
                            left_handle_ptr: *const co3::external::Extern,
                            right_handle_ptr: *const co3::external::Extern,
                            out_ptr: *mut u8,
                        ) -> co3::FfiReturn;
                    }

                    co3_eq(
                        co3::Encode::encode(handle_id, &mut ()),
                        co3::Encode::encode(self.__co3_as_ref(), &mut ()),
                        co3::Encode::encode(other.__co3_as_ref(), &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if eq_result != co3::FfiReturn::Ok  {
                    panic!("Eq returned: {}", eq_result);
                }

                unsafe {co3::Decode::decode(output.assume_init(), &mut ()).expect("Invalid output")}
            }
        }
    }
}

fn impl_partial_ord_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics PartialOrd for #name #ty_generics #where_clause {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
    }
}
fn impl_ord_for_opaque(
    name: &Ident,
    generics: &syn::Generics,
    link_name: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Ord for #name #ty_generics #where_clause {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let cmp_result = unsafe {
                    unsafe extern "C" {
                        #[link_name = #link_name]
                        fn co3_ord(
                            handle_id: <co3::handle::Id as co3::ExternC>::CType,
                            left_handle_ptr: *const co3::external::Extern,
                            right_handle_ptr: *const co3::external::Extern,
                            out_ptr: *mut i8,
                        ) -> co3::FfiReturn;
                    }

                    co3_ord(
                        co3::Encode::encode(handle_id, &mut ()),
                        co3::Encode::encode(self.__co3_as_ref(), &mut ()),
                        co3::Encode::encode(other.__co3_as_ref(), &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if cmp_result != co3::FfiReturn::Ok  {
                    panic!("Ord returned: {}", cmp_result);
                }

                unsafe {co3::Decode::decode(output.assume_init(), &mut ()).expect("Invalid output")}
            }
        }
    }
}

fn impl_shared_trait_for_opaque(
    name: &Ident,
    generics: &syn::Generics,
    trait_path: &syn::Path,
    shared_macro_ident: &Ident,
    link_prefix: &TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let prefix = quote!(#link_prefix);

    quote! {
        impl #impl_generics #trait_path for #name #ty_generics #where_clause {
            #shared_macro_ident! { @impl [ #prefix ] [] }
        }
    }
}

fn gen_shared_fns(
    emitter: &mut Emitter,
    input: &FfiTypeInput,
    generics: &syn::Generics,
    link_mode: &ExternTypeLinkMode,
) -> Vec<TokenStream> {
    let name = &input.ident;

    let mut shared_fn_impls = Vec::new();
    for derive in &input.derive_attr.derives {
        match derive {
            Derive::Rustc(derive) => match derive {
                RustcDerive::Copy => {
                    emit!(
                        emitter,
                        name,
                        "Opaque type should not implement `Copy` trait"
                    );
                }
                RustcDerive::Clone => {
                    shared_fn_impls.push(impl_clone_for_opaque(
                        name,
                        generics,
                        built_in_symbol(link_mode, "Clone", "clone"),
                    ));
                }
                RustcDerive::Default => {
                    shared_fn_impls.push(impl_default_for_opaque(
                        name,
                        generics,
                        built_in_symbol(link_mode, "Default", "default"),
                    ));
                }
                RustcDerive::PartialEq => {
                    shared_fn_impls.push(impl_partial_eq_for_opaque(
                        name,
                        generics,
                        built_in_symbol(link_mode, "Eq", "eq"),
                    ));
                }
                RustcDerive::Eq => {
                    shared_fn_impls.push(impl_eq_for_opaque(name, generics));
                }
                RustcDerive::PartialOrd => {
                    shared_fn_impls.push(impl_partial_ord_for_opaque(name, generics));
                }
                RustcDerive::Ord => {
                    shared_fn_impls.push(impl_ord_for_opaque(
                        name,
                        generics,
                        built_in_symbol(link_mode, "Ord", "cmp"),
                    ));
                }
                RustcDerive::Hash | RustcDerive::Debug => {
                    emit!(
                        emitter,
                        name,
                        "Opaque type should not implement `{:?}` trait",
                        derive
                    );
                }
            },
            #[cfg(feature = "getset")]
            Derive::GetSet(_) => {
                // handled by `getset_gen` module
            }
            Derive::Other(derive) => {
                let Some(trait_path) = emitter.handle(syn::parse_str::<syn::Path>(derive)) else {
                    emit!(emitter, name, "Invalid derive path `{}`", derive);
                    continue;
                };
                let Some(shared_macro_ident) = trait_path.segments.last().map(|seg| &seg.ident)
                else {
                    emit!(emitter, name, "Invalid derive path `{}`", derive);
                    continue;
                };
                shared_fn_impls.push(impl_shared_trait_for_opaque(
                    name,
                    generics,
                    &trait_path,
                    shared_macro_ident,
                    match link_mode {
                        ExternTypeLinkMode::LinkCrate(prefix) => prefix,
                    },
                ));
            }
        }
    }

    shared_fn_impls
}

fn has_shared_derives(input: &FfiTypeInput) -> bool {
    input.derive_attr.derives.iter().any(|derive| match derive {
        Derive::Rustc(derive) => matches!(
            derive,
            RustcDerive::Clone
                | RustcDerive::Default
                | RustcDerive::PartialEq
                | RustcDerive::Eq
                | RustcDerive::PartialOrd
                | RustcDerive::Ord
        ),
        #[cfg(feature = "getset")]
        Derive::GetSet(_) => false,
        Derive::Other(_) => true,
    })
}

pub fn wrap_as_opaque(
    emitter: &mut Emitter,
    mut input: FfiTypeInput,
    link_mode: &ExternTypeLinkMode,
) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;

    let needs_handle = has_shared_derives(&input);
    if needs_handle {
        add_handle_bound(name, &mut input.generics);
    }
    let shared_fns = if needs_handle {
        gen_shared_fns(emitter, &input, &input.generics, link_mode)
    } else {
        Vec::new()
    };
    let (_, ty_generics, where_clause) = input.generics.split_for_impl();

    let phantom_data_fields = input
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Lifetime(param) => {
                let lifetime = &param.lifetime;
                Some(quote! { core::marker::PhantomData<&#lifetime ()> })
            }
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                Some(quote! { core::marker::PhantomData<#ident> })
            }
            syn::GenericParam::Const(_) => None,
        });

    let impl_ffi = gen_impl_ffi(name, &input.generics);
    let (impl_generics, _, _) = input.generics.split_for_impl();
    let drop_impl = if needs_handle {
        let drop_link_name = built_in_symbol(link_mode, "Drop", "drop");
        quote! {
            impl #impl_generics Drop for #name #ty_generics #where_clause {
                fn drop(&mut self) {
                    let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;

                    let drop_result = unsafe {
                        unsafe extern "C" {
                            #[link_name = #drop_link_name]
                            fn co3_drop(
                                handle_id: <co3::handle::Id as co3::ExternC>::CType,
                                handle_ptr: *mut co3::external::Extern,
                            ) -> co3::FfiReturn;
                        }

                        co3_drop(
                            co3::Encode::encode(handle_id, &mut ()),
                            co3::external::External::as_mut_ptr(self),
                        )
                    };

                    if drop_result != co3::FfiReturn::Ok {
                        panic!("Drop returned: {}", drop_result);
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #[repr(transparent)]
        #vis struct #name #ty_generics(core::ptr::NonNull<co3::external::Extern> #(, #phantom_data_fields)*) #where_clause;

        #drop_impl
        #(#shared_fns)*
        #impl_ffi
    }
}

fn gen_impl_ffi(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let send_predicates: Vec<_> = generics
        .type_params()
        .map(|param| {
            let ident = &param.ident;
            quote! { #ident: Send }
        })
        .collect();
    let sync_predicates: Vec<_> = generics
        .type_params()
        .map(|param| {
            let ident = &param.ident;
            quote! { #ident: Sync }
        })
        .collect();

    let send_where = if send_predicates.is_empty() {
        where_clause.map(|wc| quote!(#wc)).unwrap_or_default()
    } else if let Some(wc) = where_clause {
        quote!(#wc, #(#send_predicates),*)
    } else {
        quote!(where #(#send_predicates),*)
    };

    let sync_where = if sync_predicates.is_empty() {
        where_clause.map(|wc| quote!(#wc)).unwrap_or_default()
    } else if let Some(wc) = where_clause {
        quote!(#wc, #(#sync_predicates),*)
    } else {
        quote!(where #(#sync_predicates),*)
    };

    quote! {
        // SAFETY: The underlying data is unaliased, i.e. it is owned
        unsafe impl #impl_generics Send for #name #ty_generics #send_where {}
        // SAFETY: The underlying data is unaliased, i.e. it is owned
        unsafe impl #impl_generics Sync for #name #ty_generics #sync_where {}

        // SAFETY: Type is a thin wrapper around [`core::ptr::NonNull<co3::external::Extern>`]
        unsafe impl #impl_generics co3::external::External for #name #ty_generics #where_clause {
            fn as_ptr(&self) -> *const co3::external::Extern {
                self.0.as_ptr() as *const _
            }
            fn as_mut_ptr(&mut self) -> *mut co3::external::Extern {
                self.0.as_ptr()
            }
        }

        impl #impl_generics #name #ty_generics #where_clause {
            fn __co3_as_ref(&self) -> co3::external::ExternRef<'_, #name #ty_generics> {
                co3::external::ExternRef::new(self)
            }

            fn __co3_as_mut(&mut self) -> co3::external::ExternRefMut<'_, #name #ty_generics> {
                co3::external::ExternRefMut::new(self)
            }
        }

        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted;
        }

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #name #ty_generics #where_clause {
            type Target = *mut co3::external::Extern;

            #[inline(always)]
            fn is_valid(_: &Self::Target) -> bool {
                // NOTE: Opaque types are never dereferenced
                true
            }
        }
        impl #impl_generics co3::niche::NicheFamily for #name #ty_generics #where_clause {
            type Kind = co3::niche::WithStableNiche;
        }

        impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
            const NICHE_VALUE: <Self as co3::ExternC>::CType = core::ptr::null_mut();
        }

        unsafe impl #impl_generics co3::niche::StableNiche for #name #ty_generics #where_clause {}
    }
}

fn gen_wrapper_signature(fn_descriptor: &FnDescriptor) -> syn::Signature {
    let mut signature = fn_descriptor.sig.clone();

    let mut type_impl_trait_resolver = TypeImplTraitResolver;
    type_impl_trait_resolver.visit_signature_mut(&mut signature);
    struct HandleAttrStripper;
    impl VisitMut for HandleAttrStripper {
        fn visit_receiver_mut(&mut self, node: &mut syn::Receiver) {
            node.attrs.retain(|attr| !attr.path().is_ident("dispatch"));
            syn::visit_mut::visit_receiver_mut(self, node);
        }

        fn visit_pat_type_mut(&mut self, node: &mut syn::PatType) {
            node.attrs.retain(|attr| !attr.path().is_ident("dispatch"));
            syn::visit_mut::visit_pat_type_mut(self, node);
        }
    }
    HandleAttrStripper.visit_signature_mut(&mut signature);

    signature
}

pub fn wrap_method_with_import(
    fn_descriptor: &FnDescriptor,
    trait_path: Option<&Path>,
    import_crate_name: Option<&TokenStream>,
    import_fn_name: Option<&LitStr>,
    import_abi: Option<&syn::Abi>,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let signature = gen_wrapper_signature(fn_descriptor);
    let ffi_fn_name = fn_descriptor.sig.ident.clone();
    let is_passthrough_abi = import_abi.is_some_and(|abi| {
        abi.name
            .as_ref()
            .is_some_and(|name| matches!(name.value().as_str(), "Rust" | "C"))
    });
    let use_passthrough_shim = import_abi
        .zip(fn_descriptor.sig.abi.as_ref())
        .is_some_and(|(inner_abi, outer_abi)| is_passthrough_abi && inner_abi != outer_abi);
    let method_body = if use_passthrough_shim {
        let import_abi = import_abi.expect("checked by use_passthrough_shim");
        let ffi_decl = ffi_fn::gen_inline_passthrough_declaration(
            fn_descriptor,
            trait_path,
            import_crate_name,
            import_fn_name,
            import_abi,
            &ffi_fn_name,
            handle_id_specs,
        );
        gen_passthrough_wrapper_method_body(
            fn_descriptor,
            &ffi_fn_name,
            Some(ffi_decl),
            handle_id_specs,
        )
    } else {
        let ffi_decl = ffi_fn::gen_inline_declaration(
            fn_descriptor,
            trait_path,
            import_crate_name,
            import_fn_name,
            &ffi_fn_name,
            handle_id_specs,
        );
        gen_wrapper_method_body(fn_descriptor, &ffi_fn_name, Some(ffi_decl), handle_id_specs)
    };
    let ffi_fn_attrs = fn_descriptor.attrs.iter().copied().filter(|attr| {
        !attr.path().is_ident("link_name")
            && !attr.path().is_ident("link")
            && !attr.path().is_ident("dispatch")
            && !attr.path().is_ident("id_pos")
    });
    let method_doc = &fn_descriptor.doc;
    let visibility = if trait_path.is_none() {
        quote! { pub }
    } else {
        quote! {}
    };

    quote! {
        #(#method_doc)*
        #(#ffi_fn_attrs)*
        #visibility #signature {
            #method_body
        }
    }
}

fn gen_passthrough_wrapper_method_body(
    fn_descriptor: &FnDescriptor,
    ffi_fn_name: &Ident,
    ffi_decl: Option<TokenStream>,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let mut arg_names: Vec<TokenStream> = Vec::new();
    if let Some(receiver) = &fn_descriptor.receiver {
        let arg_name = receiver.name().clone();
        arg_names.push(quote!(#arg_name));
    }
    arg_names.extend(fn_descriptor.input_args.iter().map(|arg| {
        let arg_name = arg.name().clone();
        quote!(#arg_name)
    }));

    inject_handle_id_args(fn_descriptor, handle_id_specs, &mut arg_names);

    let call = quote!(#ffi_fn_name(#(#arg_names),*));
    let call_stmt = if fn_descriptor.output_arg.is_some() {
        quote!(#call)
    } else {
        quote! { #call; }
    };

    quote! {
        #ffi_decl
        unsafe { #call_stmt }
    }
}

fn gen_wrapper_method_body(
    fn_descriptor: &FnDescriptor,
    ffi_fn_name: &Ident,
    ffi_decl: Option<TokenStream>,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let input_conversions = gen_input_conversion_stmts(fn_descriptor);
    let ffi_fn_call_stmt = gen_ffi_fn_call_stmt(fn_descriptor, ffi_fn_name, handle_id_specs);
    let store_sync_stmts = gen_store_sync_stmts(fn_descriptor);
    let return_stmt = gen_return_stmt(fn_descriptor);

    quote! {
        #ffi_decl

        #input_conversions

        // SAFETY:
        // 1. call to FFI function is safe, i.e. it's implementation is free from UBs.
        // 2. out-pointer is initialized, i.e. MaybeUninit::assume_init() is not UB
        unsafe {
            #ffi_fn_call_stmt
            #store_sync_stmts
            #return_stmt
        }
    }
}

fn gen_store_sync_stmts(fn_descriptor: &FnDescriptor) -> TokenStream {
    let mut stmts = quote! {};
    if let Some(receiver) = &fn_descriptor.receiver
        && !receiver.is_handle()
        && matches!(
            receiver.src_type(),
            Type::Path(syn::TypePath { qself: None, path }) if path.is_ident("Self")
        )
    {
        let store_name = gen_store_name(receiver.name());
        let arg_name = receiver.name();
        stmts.extend(quote! {
            if co3::Store::sync(#store_name).is_none() {
                panic!("failed to sync store for {}", stringify!(#arg_name));
            }
        });
    }

    for arg in &fn_descriptor.input_args {
        if arg.is_handle() {
            continue;
        }
        let store_name = gen_store_name(arg.name());
        let arg_name = arg.name();

        stmts.extend(quote! {
            if co3::Store::sync(#store_name).is_none() {
                panic!("failed to sync store for {}", stringify!(#arg_name));
            }
        });
    }

    stmts
}

fn gen_input_conversion_stmts(fn_descriptor: &FnDescriptor) -> TokenStream {
    let self_ty = fn_descriptor.self_ty.as_ref();

    let mut stmts = quote! {};
    if let Some(arg) = &fn_descriptor.receiver {
        let arg_name = arg.name();

        if arg.is_handle() {
            let processed = process_handle_type(arg_name, arg.src_type())
                .expect("handle conversion should always be defined");
            stmts.extend(quote! {let #arg_name = self;});
            stmts.extend(quote!(let #arg_name = #processed;));
        } else if matches!(
            arg.src_type(),
            Type::Path(syn::TypePath { qself: None, path }) if path.is_ident("Self")
        ) {
            let store_name = gen_store_name(arg_name);
            stmts.extend(quote! {
                let #arg_name = self;
                let mut #store_name = Default::default();
                let #arg_name = co3::Encode::encode(#arg_name, &mut #store_name);
            });
        } else if let Some(processed) = process_self_type(arg_name, arg.src_type(), self_ty) {
            stmts.extend(quote! {let #arg_name = self;});
            stmts.extend(quote!(let #arg_name = #processed;));
        } else {
            stmts.extend(quote! {let #arg_name = co3::external::External::as_ptr(self);});
        }
    }
    for arg in &fn_descriptor.input_args {
        stmts.extend(gen_input_arg_src_to_ffi(arg, self_ty));
    }
    if let Some(arg) = &fn_descriptor.output_arg {
        let name = &arg.name();

        if !arg.src_type_is_empty_tuple() {
            stmts.extend(quote! {
                let mut #name = core::mem::MaybeUninit::uninit();
            });
        }
    }

    stmts
}

fn process_self_type(
    arg_name: &Ident,
    ty: &Type,
    self_ty: Option<&syn::Path>,
) -> Option<TokenStream> {
    if is_self_ty(ty, self_ty) {
        return Some(quote! { core::mem::ManuallyDrop::new(#arg_name).0.as_ptr() });
    }

    match ty {
        Type::Path(path_ty) => {
            let last_seg = path_ty.path.segments.last().unwrap();

            if last_seg.ident == "Box"
                && let syn::PathArguments::AngleBracketed(bracketed) = &last_seg.arguments
                && bracketed.args.len() == 1
                && let syn::GenericArgument::Type(boxed) = &bracketed.args[0]
                && let Some(processed) = process_self_type(arg_name, boxed, self_ty)
            {
                return Some(quote! {{
                    let #arg_name = *#arg_name;
                    #processed
                }});
            }

            None
        }
        Type::Reference(ref_ty) => {
            if !is_self_ty(&ref_ty.elem, self_ty) {
                return process_self_type(arg_name, &ref_ty.elem, self_ty);
            };

            if ref_ty.mutability.is_none() {
                Some(quote! { co3::external::External::as_ptr(#arg_name) })
            } else {
                Some(quote! { co3::external::External::as_mut_ptr(#arg_name) })
            }
        }
        _ => None,
    }
}

fn process_handle_type(arg_name: &Ident, ty: &Type) -> Option<TokenStream> {
    match ty {
        Type::Reference(ref_ty) => {
            if ref_ty.mutability.is_some() {
                Some(quote! { co3::external::External::as_mut_ptr(#arg_name) })
            } else {
                Some(quote! { co3::external::External::as_ptr(#arg_name) })
            }
        }
        Type::Path(path_ty) => {
            let last_seg = path_ty.path.segments.last()?;
            if last_seg.ident == "ExternRefMut" {
                return Some(quote! { co3::external::External::as_mut_ptr(#arg_name) });
            }
            if last_seg.ident == "ExternRef" {
                return Some(
                    quote! { co3::external::External::as_ptr(#arg_name) as *mut co3::external::Extern },
                );
            }
            Some(quote! { core::mem::ManuallyDrop::new(#arg_name).0.as_ptr() })
        }
        _ => Some(quote! { core::mem::ManuallyDrop::new(#arg_name).0.as_ptr() }),
    }
}

pub fn is_self_ty(ty: &Type, self_ty: Option<&syn::Path>) -> bool {
    if let Type::Path(syn::TypePath { qself: None, path }) = ty {
        return path.is_ident("Self") || self_ty.is_some_and(|self_ty| self_ty == path);
    }

    false
}

fn gen_input_arg_src_to_ffi(arg: &Arg, self_ty: Option<&syn::Path>) -> TokenStream {
    let arg_name = arg.name();

    let resolve_impl_trait = gen_resolve_type(arg);
    if arg.is_handle() {
        let processed = process_handle_type(arg.name(), arg.src_type())
            .expect("handle conversion should always be defined");
        return quote! {
            #resolve_impl_trait
            let #arg_name = #processed;
        };
    }

    let store_name = gen_store_name(arg_name);
    if let Some(processed) = process_self_type(arg.name(), arg.src_type(), self_ty) {
        return quote! {
            #resolve_impl_trait
            let mut #store_name = Default::default();
            let #arg_name = #processed;
        };
    }

    quote! {
        #resolve_impl_trait
        let mut #store_name = Default::default();
        let #arg_name = co3::Encode::encode(#arg_name, &mut #store_name);
    }
}

fn injected_handle_id_expr(
    fn_descriptor: &FnDescriptor,
    spec: &HandleIdSpec,
) -> Option<TokenStream> {
    let selector = &spec.selector;
    if matches!(selector, Type::Path(type_path) if type_path.qself.is_none() && type_path.path.is_ident("Self"))
        && let Some(self_ty) = fn_descriptor.self_ty.as_ref()
    {
        return Some(quote! {
            co3::Encode::encode(<#self_ty as co3::handle::Handle>::ID, &mut ())
        });
    }

    Some(quote! {
        co3::Encode::encode(<#selector as co3::handle::Handle>::ID, &mut ())
    })
}

fn inject_handle_id_args(
    fn_descriptor: &FnDescriptor,
    handle_id_specs: &[HandleIdSpec],
    args: &mut Vec<TokenStream>,
) {
    let mut inserts: Vec<(usize, usize, TokenStream)> = handle_id_specs
        .iter()
        .enumerate()
        .filter_map(|(order, spec)| {
            injected_handle_id_expr(fn_descriptor, spec).map(|expr| (spec.at, order, expr))
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

    for (at, _order, expr) in inserts {
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
            slots[pos] = Some(expr);
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

fn gen_ffi_fn_call_stmt(
    fn_descriptor: &FnDescriptor,
    ffi_fn_name: &Ident,
    handle_id_specs: &[HandleIdSpec],
) -> TokenStream {
    let mut arg_names: Vec<TokenStream> = Vec::new();
    if let Some(arg) = &fn_descriptor.receiver {
        let arg_name = &arg.name();

        arg_names.push(quote!(#arg_name));
    }
    for arg in &fn_descriptor.input_args {
        let arg_name = &arg.name();

        arg_names.push(quote!(#arg_name));
    }

    inject_handle_id_args(fn_descriptor, handle_id_specs, &mut arg_names);

    if let Some(arg) = &fn_descriptor.output_arg {
        let arg_name = &arg.name();

        if !arg.src_type_is_empty_tuple() {
            arg_names.push(quote!(#arg_name.as_mut_ptr()));
        }
    }

    let execution_fail_arm = fn_descriptor.output_arg.as_ref().map_or_else(
        || quote! {},
        |output| {
            if unwrap_result_type(output.src_type()).is_some() {
                quote! {
                    co3::FfiReturn::ExecutionFail => {
                        // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                        //return Err(Default::default());
                        unimplemented!("Error handling is not properly implemented yet");
                    }
                }
            } else {
                quote! {}
            }
        },
    );

    quote! {
        let __ffi_return = #ffi_fn_name(#(#arg_names),*);

        match __ffi_return {
            co3::FfiReturn::Ok => {},
            #execution_fail_arm
            _ => panic!(concat!(stringify!(#ffi_fn_name), " returned {}"), __ffi_return)
        }
    }
}

fn gen_return_stmt(fn_descriptor: &FnDescriptor) -> TokenStream {
    fn_descriptor.output_arg.as_ref().map_or_else(|| quote! {}, |output| {
        if output.src_type_is_empty_tuple() {
            return quote! {Ok(())};
        }

        let arg_name= output.name();

        let return_stmt = unwrap_result_type(output.src_type())
            .map_or_else(|| quote! {#arg_name}, |_| quote! { Ok(#arg_name) });

        quote! {
            let #arg_name = #arg_name.assume_init();
            let #arg_name = co3::out_ptr::OutPtrRead::try_read_out(#arg_name).expect("Invalid out-pointer value returned");
            #return_stmt
        }
    })
}
