use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    DeclItem, DispatchItem, DropImpl, ForeignItem, ForeignItemType,
    dispatch::{erase_handle_types, gen_dispatch_export},
    ffi_fn::{self, emit_extern_definition, gen_extern_fn_signature, normalize_fn_signature},
    repr::gen_sized_size_family,
    utils::DispatchMonomorphizer,
    wrapper::{gen_extern_decl, wrap_fn_definition, wrap_impl_definition},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OwnershipMode {
    #[default]
    Borrow,
    ByValue,
}

pub(crate) fn emit_decl_exports(abi: syn::Abi, decls: Vec<DeclItem>) -> TokenStream {
    let exports = decls.into_iter().map(|decl| match decl {
        DeclItem::Item(ForeignItem::Type(ForeignItemType { id, ty, drop })) => {
            let opaque = derive_opaque_item(id.map(|id| *id), ty.clone());

            let drop_impl = drop.as_ref().map(|drop| match drop {
                DropImpl::Dispatch(item) => &item.impl_,
                DropImpl::Impl(impl_) => impl_,
            });

            let drop_impl_check = gen_drop_impl_check(&ty.ident, drop_impl.unwrap());

            let drop = drop.map(|drop| match drop {
                DropImpl::Impl(impl_) => gen_drop_impl_definition(&abi, impl_),
                DropImpl::Dispatch(item) => gen_dispatch_export(&abi, item),
            });

            quote! {
                #opaque
                #drop

                #drop_impl_check
            }
        }
        DeclItem::Item(ForeignItem::Fn(item)) => ffi_fn::gen_fn_definition(&abi, item),
        DeclItem::Item(ForeignItem::Impl(impl_)) => ffi_fn::gen_impl_definition(&abi, impl_),
        DeclItem::Dispatch(item) => gen_dispatch_export(&abi, item),
    });

    quote! { #( const _: () = { #exports }; )* }
}

pub(crate) fn expand_extern_import_decls(
    abi: syn::Abi,
    attrs: &[syn::Attribute],
    decls: Vec<DeclItem>,
) -> TokenStream {
    let imports = decls.into_iter().map(|decl| match decl {
        DeclItem::Item(ForeignItem::Type(ForeignItemType { id, ty, drop })) => {
            let ty = wrap_extern_type_decl(id.as_deref(), ty);

            let drop = drop.map(|drop| match drop {
                DropImpl::Dispatch(dispatch) => {
                    let DispatchItem { impl_, .. } = dispatch;

                    if let Some(id_ty) = &id {
                        expand_dispatch_drop_import(&abi, attrs, impl_, id_ty)
                    } else {
                        quote! {}
                    }
                }
                DropImpl::Impl(impl_) => {
                    let import = wrap_impl_definition(&impl_);
                    let extern_decl = gen_impl_extern_fn_decls(&abi, attrs, impl_, None);

                    quote! {
                        const _: () = {
                            #(#extern_decl)*
                            #import
                        };
                    }
                }
            });

            quote! {
                #ty
                #drop
            }
        }
        DeclItem::Item(ForeignItem::Fn(item)) => wrap_fn_definition(&abi, attrs, item),
        DeclItem::Item(ForeignItem::Impl(impl_)) => {
            let import = wrap_impl_definition(&impl_);
            let extern_decl = gen_impl_extern_fn_decls(&abi, attrs, impl_, None);

            quote! {
                const _: () = {
                    #(#extern_decl)*
                    #import
                };
            }
        }
        DeclItem::Dispatch(dispatch) => {
            let DispatchItem { impl_, args, .. } = dispatch;
            let wrapped = wrap_impl_definition(&impl_);

            let imports = expand_extern_dispatch_impl(&wrapped, &args);
            let extern_decl = gen_impl_extern_fn_decls(&abi, attrs, impl_, Some(&args));

            quote! {
                const _: () = {
                    #(#extern_decl)*
                    #(#imports)*
                };
            }
        }
    });

    quote! { #(#imports)* }
}

fn gen_drop_impl_definition(abi: &syn::Abi, mut impl_: ItemImpl) -> TokenStream {
    let self_ty = &impl_.self_ty;

    impl_.items.iter_mut().for_each(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return;
        };

        normalize_fn_signature(&mut item.sig, Some(self_ty));
    });

    let syn::ImplItem::Fn(item) = impl_.items.first().unwrap() else {
        unreachable!()
    };

    let fn_signature = gen_extern_fn_signature(Some(&impl_.generics), item.sig.clone());
    let ffi_fn_body = quote! {{
        let __co3_self: &mut #self_ty = unsafe {
            co3::Decode::decode(__co3_self, &mut ())
        }.ok_or(co3::FfiReturn::TrapRepresentation)?;

        unsafe { core::ptr::drop_in_place(__co3_self as *mut _) };

        Ok(())
    }};

    emit_extern_definition(abi, &item.attrs, fn_signature, ffi_fn_body)
}

fn derive_opaque_item(
    id: Option<syn::Type>,
    syn::ForeignItemType {
        attrs: _,
        ident,
        generics,
        ..
    }: syn::ForeignItemType,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.map(|w| &w.predicates);
    let params = &generics.params;

    let size_family_impl = gen_sized_size_family(&ident, &generics);
    let handle_family_impl = id
        .as_ref()
        .and_then(|id| gen_handle_family_impl(id, &ident, &generics));

    // TODO: Implement ?Sized Opaque types
    let sized_impls = quote! {
        #size_family_impl

        impl #impl_generics co3::borrow::Borrow for #ident #ty_generics #where_clause {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Store = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'__co3_r, #params> co3::borrow::ToOwned<'__co3_r> for #ident #ty_generics where Self: '__co3_r, #predicates {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'__co3_r>) -> Self {
                borrowed
            }
        }

        impl #impl_generics co3::niche::Niche for #ident #ty_generics #where_clause {
            const NICHE_VALUE: co3::boxed::CBox<Self> = co3::boxed::CBox::none();
        }
    };

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #ident #ty_generics #where_clause {
            type Kind = co3::ir::Opaque;
        }

        unsafe impl #impl_generics co3::external::External for #ident #ty_generics #where_clause {
            fn as_ptr(&self) -> *const co3::external::Extern {
                (self as *const Self).cast()
            }

            fn as_mut_ptr(&mut self) -> *mut co3::external::Extern {
                (self as *mut Self).cast()
            }
        }

        impl #impl_generics co3::borrow::DropFamily for #ident #ty_generics #where_clause {
            type Kind = co3::borrow::NoDrop;
        }

        impl #impl_generics co3::niche::NicheFamily for #ident #ty_generics #where_clause {
            type Kind = co3::niche::WithCustomNiche;
        }

        #handle_family_impl
        #sized_impls
    }
}

fn expand_dispatch_drop_import(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    impl_: ItemImpl,
    id_ty: &syn::Type,
) -> TokenStream {
    let ItemImpl {
        attrs: impl_attrs,
        generics,
        self_ty,
        items,
        ..
    } = &impl_;

    // FIXME: https://github.com/mversic/co3/issues/93
    let handle_bound = quote! { Self: co3::handle::Handle };
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let where_clause = if let Some(where_clause) = where_clause {
        quote! { #where_clause, #handle_bound }
    } else {
        quote! { where #handle_bound }
    };

    let ImplItem::Fn(ImplItemFn {
        attrs: wrapper_attrs,
        sig,
        ..
    }) = items.iter().next().unwrap()
    else {
        unreachable!()
    };

    let link_name = wrapper_attrs
        .iter()
        .find(|attr| attr.path().is_ident("link_name"));
    let wrapper_attrs = wrapper_attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("link_name"));

    let handle_id_conversion_stmts = sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(syn::PatType { pat, .. }) = arg {
                Some(quote! {
                    let __co3_id = co3::Encode::encode(<Self as co3::handle::Handle>::ID, &mut ());

                    let #pat = unsafe {
                        // FIXME: THIS IS HACKED: https://github.com/mversic/co3/issues/93
                        core::ptr::read((&__co3_id as *const <<Self as co3::handle::HandleFamily>::Kind as co3::ExternC>::CType).cast::<#id_ty>())
                    };
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let (inputs, values): (Vec<_>, Vec<_>) = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(_) => (
                quote!(__co3_self: *mut core::ffi::c_void),
                quote!(__co3_self),
            ),
            FnArg::Typed(syn::PatType { pat, .. }) => (quote!(#pat: #id_ty), quote!(#pat)),
        })
        .unzip();

    quote! {
        #(#impl_attrs)*
        impl #impl_generics Drop for #self_ty #where_clause {
            #(#wrapper_attrs)*
            fn drop(&mut self) {
                unsafe #abi {
                    #(#attrs)*

                    #link_name
                    fn drop(#(#inputs),*) -> co3::FfiReturn;
                }

                let __co3_self = self as *mut #self_ty as *mut core::ffi::c_void;
                #(#handle_id_conversion_stmts)*
                unsafe { drop(#(#values),*) };
            }
        }
    }
}

pub(crate) fn is_unsafe_no_mangle(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("unsafe") {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let Ok(metas) =
        meta_list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    else {
        return false;
    };

    metas.into_iter().any(|meta| match meta {
        syn::Meta::Path(path) => path.is_ident("no_mangle"),
        _ => false,
    })
}

fn expand_extern_dispatch_impl(
    impl_: &ItemImpl,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> Vec<ItemImpl> {
    if args.is_empty() {
        return vec![impl_.clone()];
    }

    args.iter()
        .map(|entry| {
            let mut monomorphized = impl_.clone();
            monomorphized.generics.params.clear();

            DispatchMonomorphizer::new(&impl_.generics, entry)
                .visit_item_impl_mut(&mut monomorphized);

            monomorphized
        })
        .collect()
}

fn gen_drop_impl_check(ident: &syn::Ident, impl_: &ItemImpl) -> TokenStream {
    let (impl_generics, _, where_clause) = impl_.generics.split_for_impl();
    let self_ty = &impl_.self_ty;

    let decl_params = drop_check_decl_params(impl_);
    let marker_fields = decl_params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote!(core::marker::PhantomData<&#lifetime ()>)
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote!(core::marker::PhantomData<#ident>)
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote!([(); #ident])
        }
    });

    let fields = quote!((#(#marker_fields),*););
    let decl_generics = (!decl_params.is_empty()).then(|| quote!(<#decl_params>));

    quote! {
        {
            struct #ident #decl_generics #fields #where_clause

            impl #impl_generics Drop for #self_ty #where_clause {
                fn drop(&mut self) {}
            }
        }
    }
}

fn drop_check_decl_params(impl_: &ItemImpl) -> Punctuated<syn::GenericParam, syn::Token![,]> {
    if !impl_.generics.params.is_empty() {
        return impl_.generics.params.clone();
    }

    let syn::Type::Path(type_path) = impl_.self_ty.as_ref() else {
        return Punctuated::new();
    };
    let Some(segment) = type_path.path.segments.last() else {
        return Punctuated::new();
    };

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Punctuated::new();
    };

    let mut params = Punctuated::new();
    for (idx, arg) in args.args.iter().enumerate() {
        match arg {
            syn::GenericArgument::Lifetime(_) => {
                let lifetime =
                    syn::Lifetime::new(&format!("'__co3_l{idx}"), proc_macro2::Span::call_site());
                params.push(syn::parse_quote!(#lifetime));
            }
            syn::GenericArgument::Type(_) => {
                let ident = format_ident!("__Co3T{idx}");
                params.push(syn::parse_quote!(#ident));
            }
            syn::GenericArgument::Const(_) => {
                let ident = format_ident!("__CO3_N{idx}");
                params.push(syn::parse_quote!(const #ident: usize));
            }
            syn::GenericArgument::AssocType(_)
            | syn::GenericArgument::AssocConst(_)
            | syn::GenericArgument::Constraint(_) => {}
            _ => {}
        }
    }

    params
}

fn gen_impl_extern_fn_decls(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    impl_: ItemImpl,
    args: Option<&Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>>,
) -> Vec<TokenStream> {
    impl_
        .items
        .into_iter()
        .filter_map(|item| {
            let syn::ImplItem::Fn(item) = item else {
                return None;
            };

            let mut sig = item.sig;
            normalize_fn_signature(&mut sig, Some(&impl_.self_ty));

            if let Some(args) = args {
                erase_handle_types(&impl_.generics, &impl_.self_ty, &mut sig, args);
            }

            let decl = ffi_fn::gen_extern_fn_signature(Some(&impl_.generics), sig);
            Some(gen_extern_decl(abi, attrs, &item.attrs, decl))
        })
        .collect()
}

fn wrap_extern_type_decl(
    id: Option<&syn::Type>,
    syn::ForeignItemType {
        attrs,
        vis,
        ident,
        mut generics,
        ..
    }: syn::ForeignItemType,
) -> TokenStream {
    if !generics.params.is_empty() {
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote! { Self: co3::handle::Handle });
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let handle_family_impl = id
        .as_ref()
        .and_then(|id| gen_handle_family_impl(id, &ident, &generics));
    let predicates = where_clause.map(|w| &w.predicates);
    let params = &generics.params;

    use syn::GenericParam::*;
    let phantom_data_fields = generics.params.iter().filter_map(|param| match param {
        Lifetime(param) => {
            let lifetime = &param.lifetime;
            Some(quote! { core::marker::PhantomData<&#lifetime mut ()> })
        }
        Type(param) => {
            let ident = &param.ident;
            Some(quote! { core::marker::PhantomData<#ident> })
        }
        Const(_) => None,
    });

    quote! {
        #(#attrs)*
        #[repr(transparent)]
        // TODO: Should we use CBox<Extern>? NonNull has niche optimization attached tho
        #vis struct #ident #impl_generics(core::ptr::NonNull<co3::external::Extern> #(, #phantom_data_fields)*) #where_clause;

        unsafe impl #impl_generics co3::external::External for #ident #ty_generics #where_clause {
            fn as_ptr(&self) -> *const co3::external::Extern {
                self.0.as_ptr() as *const _
            }
            fn as_mut_ptr(&mut self) -> *mut co3::external::Extern {
                self.0.as_ptr()
            }
        }

        impl #impl_generics #ident #ty_generics #where_clause {
            fn as_ref(&self) -> co3::external::ExternRef<'_, #ident #ty_generics> {
                co3::external::ExternRef::new(self)
            }

            fn as_mut(&mut self) -> co3::external::ExternRefMut<'_, #ident #ty_generics> {
                co3::external::ExternRefMut::new(self)
            }
        }

        #handle_family_impl

        impl #impl_generics co3::ir::ReprFamily for #ident #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted;
        }
        impl #impl_generics co3::ir::SizeFamily for #ident #ty_generics #where_clause {
            // FIXME: Likely it should be unsized. This probably means we'll need a custom IR type
            type Kind = co3::ir::Sized_;
        }
        impl #impl_generics co3::borrow::DropFamily for #ident #ty_generics #where_clause {
            type Kind = co3::borrow::NoDrop;
        }
        impl #impl_generics co3::niche::NicheFamily for #ident #ty_generics #where_clause {
            type Kind = co3::niche::WithStableNiche;
        }

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            type Target = *mut co3::external::Extern;

            #[inline(always)]
            fn is_valid(_: &Self::Target) -> bool {
                // NOTE: Opaque types are never dereferenced
                true
            }
        }

        unsafe impl #impl_generics co3::transmute::EncodeTransmuted<false> for #ident #ty_generics #where_clause {
            type Store = <Self::Target as co3::Encode>::Store;
        }

        impl #impl_generics co3::borrow::Borrow for #ident #ty_generics #where_clause {
            type Borrowed<'itm> = Self
            where
                Self: 'itm;

            type Store = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'__co3_r, #params> co3::borrow::ToOwned<'__co3_r> for #ident #ty_generics where Self: '__co3_r, #predicates {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'__co3_r>) -> Self {
                borrowed
            }
        }

        impl #impl_generics co3::niche::Niche for #ident #ty_generics #where_clause {
            const NICHE_VALUE: <Self as co3::ExternC>::CType = core::ptr::null_mut();
        }

        unsafe impl #impl_generics co3::niche::StableNiche for #ident #ty_generics #where_clause {}
    }
}

fn gen_handle_family_impl(
    id_ty: &syn::Type,
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> Option<TokenStream> {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Some(quote! {
        impl #impl_generics co3::handle::HandleFamily for #ident #ty_generics #where_clause {
            type Kind = #id_ty;
        }
    })
}
