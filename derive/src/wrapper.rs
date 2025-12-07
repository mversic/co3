use manyhow::emit;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Ident, Type, parse_quote, visit_mut::VisitMut};

use crate::{
    attr_parse::derive::{Derive, RustcDerive},
    convert::FfiTypeInput,
    emitter::Emitter,
    ffi_fn,
    impl_visitor::{Arg, FnDescriptor, ImplDescriptor, TypeImplTraitResolver},
    utils::{gen_resolve_type, gen_store_name, unwrap_result_type},
};

fn add_handle_bound(name: &Ident, generics: &mut syn::Generics) {
    let cloned_generics = generics.clone();
    let (_, ty_generics, _) = cloned_generics.split_for_impl();

    generics
        .make_where_clause()
        .predicates
        .push(parse_quote! {#name #ty_generics: co3::handle::Handle});
}

fn impl_clone_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Clone for #name #ty_generics #where_clause {
            fn clone(&self) -> Self {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let clone_result = unsafe {
                    crate::__clone(
                        co3::Encode::encode(handle_id, &mut ()),
                        co3::Encode::encode(self.as_ref(), &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if clone_result != co3::FfiReturn::Ok  {
                    panic!("Clone returned: {}", clone_result);
                }

                unsafe {co3::out_ptr::OutPtrRead::try_read_out(output.assume_init()).expect("Invalid output")}
            }
        }
    }
}

fn impl_default_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Default for #name #ty_generics #where_clause {
            fn default() -> Self {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let default_result = unsafe {
                    crate::__default(
                        co3::Encode::encode(handle_id, &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if default_result != co3::FfiReturn::Ok  {
                    panic!("Default returned: {}", default_result);
                }

                unsafe {co3::out_ptr::OutPtrRead::try_read_out(output.assume_init()).expect("Invalid output")}
            }
        }
    }
}

fn impl_eq_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! { impl #impl_generics Eq for #name #ty_generics #where_clause {} }
}
fn impl_partial_eq_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics PartialEq for #name #ty_generics #where_clause {
            fn eq(&self, other: &Self) -> bool {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let eq_result = unsafe {
                    crate::__eq(
                        co3::Encode::encode(handle_id, &mut ()),
                        co3::Encode::encode(self.as_ref(), &mut ()),
                        co3::Encode::encode(other.as_ref(), &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if eq_result != co3::FfiReturn::Ok  {
                    panic!("Eq returned: {}", eq_result);
                }

                unsafe {co3::out_ptr::OutPtrRead::try_read_out(output.assume_init()).expect("Invalid output")}
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
fn impl_ord_for_opaque(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics Ord for #name #ty_generics #where_clause {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;
                let mut output = core::mem::MaybeUninit::uninit();

                let cmp_result = unsafe {
                    crate::__ord(
                        co3::Encode::encode(handle_id, &mut ()),
                        co3::Encode::encode(self.as_ref(), &mut ()),
                        co3::Encode::encode(other.as_ref(), &mut ()),
                        output.as_mut_ptr(),
                    )
                };

                if cmp_result != co3::FfiReturn::Ok  {
                    panic!("Ord returned: {}", cmp_result);
                }

                unsafe {co3::out_ptr::OutPtrRead::try_read_out(output.assume_init()).expect("Invalid output")}
            }
        }
    }
}

fn gen_shared_fns(emitter: &mut Emitter, input: &FfiTypeInput) -> Vec<TokenStream> {
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
                    shared_fn_impls.push(impl_clone_for_opaque(name, &input.generics));
                }
                RustcDerive::Default => {
                    shared_fn_impls.push(impl_default_for_opaque(name, &input.generics));
                }
                RustcDerive::PartialEq => {
                    shared_fn_impls.push(impl_partial_eq_for_opaque(name, &input.generics));
                }
                RustcDerive::Eq => {
                    shared_fn_impls.push(impl_eq_for_opaque(name, &input.generics));
                }
                RustcDerive::PartialOrd => {
                    shared_fn_impls.push(impl_partial_ord_for_opaque(name, &input.generics));
                }
                RustcDerive::Ord => {
                    shared_fn_impls.push(impl_ord_for_opaque(name, &input.generics));
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
                emit!(
                    emitter,
                    name,
                    "Opaque type should not implement `{}` trait",
                    derive
                );
            }
        }
    }

    shared_fn_impls
}

pub fn wrap_as_opaque(emitter: &mut Emitter, mut input: FfiTypeInput) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;

    add_handle_bound(name, &mut input.generics);
    let (impl_generics, ty_generics, handle_bounded_where_clause) = input.generics.split_for_impl();

    let phantom_data_type_defs: Vec<_> = input
        .generics
        .type_params()
        .map(|param| quote! { core::marker::PhantomData<#param> })
        .collect();

    let impl_ffi = gen_impl_ffi(name, &input.generics);

    let shared_fns = gen_shared_fns(emitter, &input);
    // TODO: which attributes do we need to keep?
    // in darling there is mechanism to forwards attrs, but it needs to be an whitelist
    // it seems that as of now no such forwarding needs to take place
    // so we just drop all attributes
    let attrs = Vec::<Attribute>::new();

    quote! {
        #(#attrs)*
        #[repr(transparent)]
        #vis struct #name #ty_generics(core::ptr::NonNull<co3::external::Extern> #(, #phantom_data_type_defs)*)
        #handle_bounded_where_clause;

        impl #impl_generics Drop for #name #ty_generics #handle_bounded_where_clause {
            fn drop(&mut self) {
                let handle_id = <#name #ty_generics as co3::handle::Handle>::ID;

                let drop_result = unsafe {
                    crate::__drop(
                        co3::Encode::encode(handle_id, &mut ()),
                        co3::Encode::encode(self.0, &mut ())
                    )
                };

                if drop_result != co3::FfiReturn::Ok  {
                    panic!("Drop returned: {}", drop_result);
                }
            }
        }

        #(#shared_fns)*
        #impl_ffi
    }
}

fn gen_impl_ffi(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let send_predicates = generics.type_params().map(|param| {
        quote! { #param: Send }
    });
    let sync_predicates = generics.type_params().map(|param| {
        quote! { #param: Sync }
    });

    quote! {
        // SAFETY: The underlying data is unaliased, i.e. it is owned
        unsafe impl #impl_generics Send for #name #ty_generics #where_clause #(, #send_predicates)* {}
        // SAFETY: The underlying data is unaliased, i.e. it is owned
        unsafe impl #impl_generics Sync for #name #ty_generics #where_clause #(, #sync_predicates)* {}

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
            fn as_ref(&self) -> co3::external::ExternRef<'_, #name #ty_generics> {
                co3::external::ExternRef::new(self)
            }

            fn as_mut(&mut self) -> co3::external::ExternRefMut<'_, #name #ty_generics> {
                co3::external::ExternRefMut::new(self)
            }
        }

        co3::mineral! {
            unsafe impl #impl_generics Transparent for #name #ty_generics #where_clause {
                type Target = core::ptr::NonNull<co3::external::Extern>;
            }
        }
    }
}

pub fn wrap_impl_items(impl_desc: &ImplDescriptor) -> TokenStream {
    let impl_attrs = &impl_desc.attrs;

    if impl_desc.fns.is_empty() {
        return quote! {};
    }
    let self_ty = &impl_desc.fns[0].self_ty;
    let mut self_methods = Vec::new();
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

    for fn_ in &impl_desc.fns {
        let trait_name = impl_desc.trait_name();

        if let Some(wrapped) = is_shared_fn(fn_, trait_name) {
            return wrapped;
        }

        self_methods.push(wrap_method(fn_, trait_name));
    }

    let mut result = Vec::new();
    if !self_methods.is_empty() {
        result.push(quote! {
            #(#impl_attrs)*
            impl #impl_trait_for #self_ty {
                #(type #associated_names = #associated_types;)*
                #(#self_methods)*
            }
        });
    }
    quote! { #(#result)* }
}

fn gen_wrapper_signature(fn_descriptor: &FnDescriptor) -> syn::Signature {
    let mut signature = fn_descriptor.sig.clone();

    let mut type_impl_trait_resolver = TypeImplTraitResolver;
    type_impl_trait_resolver.visit_signature_mut(&mut signature);

    signature
}

fn is_shared_fn(fn_descriptor: &FnDescriptor, trait_name: Option<&Ident>) -> Option<TokenStream> {
    let mut generics = parse_quote! {};

    if let Some(trait_name) = trait_name {
        let self_ty = fn_descriptor.self_ty_name().expect("Method without Self");
        add_handle_bound(self_ty, &mut generics);

        if trait_name == "Clone" {
            return Some(impl_clone_for_opaque(self_ty, &generics));
        }
        if trait_name == "Default" {
            return Some(impl_default_for_opaque(self_ty, &generics));
        }
        if trait_name == "PartialEq" {
            return Some(impl_partial_eq_for_opaque(self_ty, &generics));
        }
        if trait_name == "Eq" {
            return Some(impl_eq_for_opaque(self_ty, &generics));
        }
        if trait_name == "PartialOrd" {
            return Some(impl_partial_ord_for_opaque(self_ty, &generics));
        }
        if trait_name == "Ord" {
            return Some(impl_ord_for_opaque(self_ty, &generics));
        }
    }

    None
}

pub fn wrap_method(fn_descriptor: &FnDescriptor, trait_name: Option<&Ident>) -> TokenStream {
    let signature = gen_wrapper_signature(fn_descriptor);
    let ffi_fn_name = ffi_fn::gen_fn_name(fn_descriptor, trait_name);
    let method_body = gen_wrapper_method_body(fn_descriptor, &ffi_fn_name);
    let ffi_fn_attrs = &fn_descriptor.attrs;
    let method_doc = &fn_descriptor.doc;
    let visibility = if trait_name.is_none() {
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

fn gen_wrapper_method_body(fn_descriptor: &FnDescriptor, ffi_fn_name: &Ident) -> TokenStream {
    let input_conversions = gen_input_conversion_stmts(fn_descriptor);
    let ffi_fn_call_stmt = gen_ffi_fn_call_stmt(fn_descriptor, ffi_fn_name);
    let return_stmt = gen_return_stmt(fn_descriptor);

    quote! {
        #input_conversions

        // SAFETY:
        // 1. call to FFI function is safe, i.e. it's implementation is free from UBs.
        // 2. out-pointer is initialized, i.e. MaybeUninit::assume_init() is not UB
        unsafe {
            #ffi_fn_call_stmt
            #return_stmt
        }
    }
}

fn gen_input_conversion_stmts(fn_descriptor: &FnDescriptor) -> TokenStream {
    let self_ty = fn_descriptor.self_ty.as_ref();

    let mut stmts = quote! {};
    if let Some(arg) = &fn_descriptor.receiver {
        let arg_name = arg.name();

        if let Some(processed) = process_self_type(arg_name, arg.src_type(), self_ty) {
            stmts.extend(quote! {let #arg_name = self;});
            stmts.extend(quote!(let #arg_name = #processed;));
        } else {
            stmts.extend(quote! {let #arg_name = self.as_ptr();});
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

pub fn is_self_ty(ty: &Type, self_ty: Option<&syn::Path>) -> bool {
    if let Type::Path(syn::TypePath { qself: None, path }) = ty {
        return path.is_ident("Self") || self_ty.is_some_and(|self_ty| self_ty == path);
    }

    false
}

fn gen_input_arg_src_to_ffi(arg: &Arg, self_ty: Option<&syn::Path>) -> TokenStream {
    let arg_name = arg.name();

    let resolve_impl_trait = gen_resolve_type(arg);
    let store_name = gen_store_name(arg_name);

    let mut stmts = quote! {
        #resolve_impl_trait
        let mut #store_name = Default::default();
    };

    stmts.extend(
        if let Some(processed) = process_self_type(arg.name(), arg.src_type(), self_ty) {
            quote!(let #arg_name = #processed;)
        } else {
            quote!(let #arg_name = co3::Encode::encode(#arg_name, &mut #store_name);)
        },
    );

    stmts
}

fn gen_ffi_fn_call_stmt(fn_descriptor: &FnDescriptor, ffi_fn_name: &Ident) -> TokenStream {
    let mut arg_names = quote! {};
    if let Some(arg) = &fn_descriptor.receiver {
        let arg_name = &arg.name();

        arg_names.extend(quote! {
            #arg_name,
        });
    }
    for arg in &fn_descriptor.input_args {
        let arg_name = &arg.name();

        arg_names.extend(quote! {
            #arg_name,
        });
    }
    if let Some(arg) = &fn_descriptor.output_arg {
        let arg_name = &arg.name();

        if !arg.src_type_is_empty_tuple() {
            arg_names.extend(quote! {
                #arg_name.as_mut_ptr()
            });
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
        let __ffi_return = #ffi_fn_name(#arg_names);

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
