use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    DropImpl, DynImpl, ForeignItem, ForeignItemType,
    dispatch::{erase_handle_types, gen_dispatch_export},
    ffi_fn::{
        self, emit_extern_definition, gen_extern_fn_signature, merge_generics,
        normalize_fn_signature,
    },
    repr::gen_sized_size_family,
    utils::DispatchMonomorphizer,
    wrapper::{
        gen_extern_decl, strip_internal_generic_attrs, wrap_fn_definition, wrap_impl_definition,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OwnershipMode {
    #[default]
    Borrow,
    ByValue,
}

pub(crate) fn emit_decl_exports(abi: syn::Abi, decls: Vec<ForeignItem>) -> TokenStream {
    let exports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            dyn_self_impls,
            drop,
        }) => {
            let opaque = derive_opaque_item(id.as_deref(), ty.clone());

            let dispatch = dyn_self_impls
                .into_iter()
                .map(|dispatch| gen_dispatch_export(&abi, dispatch, id.as_deref()));

            let drop_impl = drop.as_ref().map(|drop| match drop {
                DropImpl::DynSelfImpl(item) => &item.impl_,
                DropImpl::DynImpl(item) => &item.impl_,
                DropImpl::Impl(impl_) => impl_,
            });

            let drop_check = gen_drop_impl_check(&ty, drop_impl.unwrap());
            let drop = drop.map(|drop| match drop {
                DropImpl::DynSelfImpl(item) => gen_dispatch_export(&abi, item, id.as_deref()),
                DropImpl::DynImpl(item) => gen_dispatch_export(&abi, item, None),
                DropImpl::Impl(impl_) => gen_drop_impl_definition(&abi, impl_),
            });

            quote! {
                #opaque

                #drop
                #drop_check

                #(#dispatch)*
            }
        }
        ForeignItem::Fn(item) => ffi_fn::gen_fn_definition(&abi, item),
        ForeignItem::Impl(impl_) => ffi_fn::gen_impl_definition(&abi, impl_),
        ForeignItem::DynImpl(item) => gen_dispatch_export(&abi, item, None),
    });

    quote! { #( const _: () = { #exports }; )* }
}

pub(crate) fn expand_extern_import_decls(
    abi: syn::Abi,
    attrs: &[syn::Attribute],
    decls: Vec<ForeignItem>,
) -> TokenStream {
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

    fn gen_impl_extern_fn_decls(
        abi: &syn::Abi,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
        self_id: Option<&syn::Type>,
        args: Option<&Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>>,
    ) -> Vec<TokenStream> {
        let self_ty = &impl_.self_ty;

        impl_
            .items
            .into_iter()
            .filter_map(|item| {
                let syn::ImplItem::Fn(mut item) = item else {
                    return None;
                };

                normalize_fn_signature(&mut item.sig, Some(&impl_.self_ty));
                merge_generics(impl_.generics.clone(), &mut item.sig.generics);

                if let Some(args) = args {
                    erase_handle_types(&impl_.generics, self_id, self_ty, &mut item.sig, args);
                }

                let decl = gen_extern_fn_signature(item.sig);
                Some(gen_extern_decl(abi, attrs, &item.attrs, decl))
            })
            .collect()
    }

    fn expand_impl_import(
        abi: &syn::Abi,
        attrs: &[syn::Attribute],
        impl_: ItemImpl,
    ) -> TokenStream {
        let import = wrap_impl_definition(&impl_, None);
        let extern_decl = gen_impl_extern_fn_decls(abi, attrs, impl_, None, None);

        quote! {
            const _: () = {
                #(#extern_decl)*
                #import
            };
        }
    }

    fn expand_dispatch_import(
        abi: &syn::Abi,
        attrs: &[syn::Attribute],
        self_id: Option<&syn::Type>,
        dispatch: DynImpl,
    ) -> TokenStream {
        let DynImpl { impl_, args, .. } = dispatch;
        let wrapped = wrap_impl_definition(&impl_, self_id);
        let imports = expand_extern_dispatch_impl(&wrapped, &args);
        let extern_decl = gen_impl_extern_fn_decls(abi, attrs, impl_, self_id, Some(&args));

        quote! {
            const _: () = {
                #(#extern_decl)*
                #(#imports)*
            };
        }
    }

    let imports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            dyn_self_impls,
            drop,
        }) => {
            let ty = wrap_extern_type_decl(id.as_deref(), ty);

            let dispatch = dyn_self_impls
                .into_iter()
                .map(|dispatch| expand_dispatch_import(&abi, attrs, id.as_deref(), dispatch));

            let drop = drop.map(|drop| match drop {
                DropImpl::DynSelfImpl(d) | DropImpl::DynImpl(d) => {
                    expand_dispatch_drop_import(&abi, attrs, d.impl_, id.as_deref().unwrap())
                }
                DropImpl::Impl(impl_) => expand_impl_import(&abi, attrs, impl_),
            });

            quote! {
                #ty
                #drop
                #(#dispatch)*
            }
        }
        ForeignItem::Fn(item) => wrap_fn_definition(&abi, attrs, item),
        ForeignItem::Impl(impl_) => expand_impl_import(&abi, attrs, impl_),
        ForeignItem::DynImpl(dispatch) => expand_dispatch_import(&abi, attrs, None, dispatch),
    });

    quote! { #(#imports)* }
}

fn gen_drop_impl_definition(abi: &syn::Abi, mut impl_: ItemImpl) -> TokenStream {
    let self_ty = &impl_.self_ty;

    let Some(syn::ImplItem::Fn(mut item)) = impl_.items.pop() else {
        unreachable!()
    };

    let ffi_fn_body = quote! {{
        let __co3_self: &mut #self_ty = unsafe {
            co3::Decode::decode(__co3_self, &mut ())
        }.ok_or(co3::FfiReturn::TrapRepresentation)?;

        unsafe { core::ptr::drop_in_place(__co3_self as *mut _) };

        Ok(())
    }};

    normalize_fn_signature(&mut item.sig, Some(self_ty));
    merge_generics(impl_.generics.clone(), &mut item.sig.generics);
    let fn_signature = gen_extern_fn_signature(item.sig);
    emit_extern_definition(abi, &item.attrs, fn_signature, ffi_fn_body)
}

fn derive_opaque_item(
    id: Option<&syn::Type>,
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
    let handle_family_impl = id.and_then(|id| gen_handle_family_impl(id, &ident, &generics));

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

    let (impl_generics, _, _) = generics.split_for_impl();
    let predicates = generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

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
                    let #pat: #id_ty = {
                        // FIXME: https://github.com/mversic/co3/issues/93
                        // Should it be required that HandleFamily::Kind: Copy
                        let __co3_handle_id = <Self as co3::handle::Handle>::ID;
                        unsafe { core::mem::transmute_copy(&__co3_handle_id)
                    }};
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
            FnArg::Receiver(_) => (quote!(*mut core::ffi::c_void), quote!(__co3_self)),
            FnArg::Typed(syn::PatType { pat, .. }) => {
                (quote!(<#id_ty as co3::ExternC>::CType), quote!(#pat))
            }
        })
        .unzip();

    quote! {
        #(#impl_attrs)*
        impl #impl_generics Drop for #self_ty where
            Self: co3::handle::Handle,
            #predicates
        {
            #(#wrapper_attrs)*
            fn drop(&mut self) {
                unsafe #abi {
                    #(#attrs)*

                    #link_name
                    fn drop(#(#values: #inputs),*) -> co3::FfiReturn;
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

fn gen_drop_impl_check(item: &syn::ForeignItemType, impl_: &ItemImpl) -> TokenStream {
    let mut impl_generics = impl_.generics.clone();
    strip_internal_generic_attrs(&mut impl_generics);

    let ident = &item.ident;
    let self_ty = &impl_.self_ty;

    let item_attrs = impl_
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("dispatch"));

    let marker_fields = item.generics.params.iter().map(|param| match param {
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

    let (decl_generics, _, item_where_clause) = item.generics.split_for_impl();
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();

    quote! {{
        #(#item_attrs)*
        struct #ident #decl_generics (#(#marker_fields),*) #item_where_clause;

        impl #impl_generics Drop for #self_ty #where_clause {
            fn drop(&mut self) {}
        }
    }}
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
