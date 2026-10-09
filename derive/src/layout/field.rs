use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, GenericParam, Type, TypeParamBound, WherePredicate, parse_quote};

use super::{ReprCAttrs, attr::ReprKind, gen_is_valid_call};

fn is_unsized_field(input: &DeriveInput, field_ty: &Type) -> bool {
    match field_ty {
        Type::Slice(_) | Type::TraitObject(_) => true,
        Type::Path(path) if path.qself.is_none() && path.path.is_ident("str") => true,
        Type::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
            let ident = &path.path.segments[0].ident;
            let inline = input.generics.params.iter().any(|param| {
                matches!(param, GenericParam::Type(param)
                    if param.ident == *ident && param.bounds.iter().any(|bound|
                        matches!(bound, TypeParamBound::Trait(bound)
                            if bound.maybe.is_some() && bound.path.is_ident("Sized"))))
            });
            let in_where_clause = input.generics.where_clause.as_ref().is_some_and(|clause| {
                clause.predicates.iter().any(|predicate| {
                    matches!(predicate, WherePredicate::Type(predicate)
                        if matches!(&predicate.bounded_ty, Type::Path(path)
                            if path.qself.is_none() && path.path.is_ident(ident))
                        && predicate.bounds.iter().any(|bound|
                            matches!(bound, TypeParamBound::Trait(bound)
                                if bound.maybe.is_some() && bound.path.is_ident("Sized"))))
                })
            });
            inline || in_where_clause
        }
        _ => false,
    }
}

pub(super) fn derive_field_repr_c(
    input: &DeriveInput,
    repr: Option<&ReprKind>,
    attrs: &ReprCAttrs,
) -> TokenStream {
    let syn::Data::Struct(data) = &input.data else {
        unreachable!("validated single-field struct")
    };
    let field = data.fields.iter().next().expect("validated single field");
    let field_ty = &field.ty;
    let member = field.ident.as_ref().map_or_else(
        || syn::Member::Unnamed(syn::Index::from(0)),
        |ident| syn::Member::Named(ident.clone()),
    );
    let name = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let construct = match &field.ident {
        Some(ident) => quote! { Self { #ident: value } },
        None => quote! { Self(value) },
    };

    let mut repr_generics = input.generics.clone();
    repr_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#field_ty: co3::ReprC));
    let (repr_impl_generics, _, repr_where_clause) = repr_generics.split_for_impl();

    let niche_bound: Option<WherePredicate> = attrs
        .niche_value
        .as_ref()
        .map(|_| parse_quote!(<#field_ty as co3::ReprC>::CType: Copy + PartialEq));
    let niche_bounds = niche_bound.iter().collect::<Vec<_>>();
    let niche_rejection = attrs.niche_value.as_ref().map(|value| {
        quote! {
            if source == #value {
                return None;
            }
        }
    });
    let checked_niche_rejection = attrs.niche_value.as_ref().map(|value| {
        quote! {
            if *target == #value {
                return false;
            }
        }
    });
    let checked_custom_validation = attrs.is_valid.as_ref().map(|is_valid| {
        let call = gen_is_valid_call(is_valid, &[field_ty], &[quote!(field)]);
        quote! {
            let field: &#field_ty = unsafe { &*(core::ptr::from_ref(target) as *const #field_ty) };
            if !#call {
                return false;
            }
        }
    });
    let decode_custom_validation = attrs.is_valid.as_ref().map(|is_valid| {
        let call = gen_is_valid_call(is_valid, &[field_ty], &[quote!(&value)]);
        quote! {
            if !#call {
                return None;
            }
        }
    });

    let checked = matches!(repr, Some(ReprKind::Transparent)).then(|| {
        let mut generics = input.generics.clone();
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#field_ty: co3::transmute::CheckedTransmute));
        generics
            .make_where_clause()
            .predicates
            .extend(niche_bound.iter().cloned());
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        quote! {
            unsafe impl #impl_generics co3::transmute::CheckedTransmute
                for #name #ty_generics #where_clause
            {
                unsafe fn is_valid(target: &Self::CType) -> bool {
                    #checked_niche_rejection
                    if !unsafe { <#field_ty as co3::transmute::CheckedTransmute>::is_valid(target) } {
                        return false;
                    }
                    #checked_custom_validation
                    true
                }
            }
        }
    });

    let codecs = (!is_unsized_field(input, field_ty)).then(|| {
        let (impl_generics, _, where_clause) = input.generics.split_for_impl();
        let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
        let speculative = (input.generics.type_params().count() == 0).then_some(quote!(for<'_dummy>));
        let encode_niche_check = attrs.niche_value.as_ref().map(|value| {
            quote! {
                debug_assert!(encoded != #value, "encoding produced the reserved NICHE");
            }
        });
        let has_custom_drop = input.attrs.iter().any(|attr| {
            attr.path().is_ident("rust_spec")
                && attr
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .is_ok_and(|args| args.iter().any(|arg| arg.path().is_ident("custom_drop")))
        });
        let custom_impl = has_custom_drop.then(|| {
            let mut custom_generics = input.generics.clone();
            custom_generics.params.push(parse_quote!(__Co3Drop: co3::rust_spec::drop::DropKind));
            let (custom_impl_generics, _, _) = custom_generics.split_for_impl();
            quote! {
                unsafe impl #custom_impl_generics EncodeOwned for #name #ty_generics
                where
                    #speculative #name #ty_generics: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::CustomDrop<__Co3Drop>>,
                    #field_ty: co3::stored::EncodeOwned,
                    #(#niche_bounds,)*
                    #predicates
                {
                    type Store = <#field_ty as co3::stored::EncodeOwned>::Store;

                    fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType
                    where Self: '_išč {
                        let owner = core::mem::ManuallyDrop::new(self);
                        let value = unsafe { core::ptr::read(&raw const (*owner).#member) };
                        let encoded = co3::stored::EncodeOwned::soft_encode(value, store);
                        #encode_niche_check
                        encoded
                    }
                }
            }
        });

        let mut decode_generics = input.generics.clone();
        decode_generics.params.insert(0, parse_quote!('_dšč));
        decode_generics.make_where_clause().predicates.push(parse_quote!(#field_ty: co3::stored::DecodeOwned<'_dšč>));
        decode_generics.make_where_clause().predicates.extend(niche_bound.iter().cloned());
        let (decode_impl_generics, _, decode_where_clause) = decode_generics.split_for_impl();

        let niche_impl = if let Some(value) = attrs.niche_value.as_ref() {
            let mut generics = input.generics.clone();
            generics.make_where_clause().predicates.extend(niche_bound.iter().cloned());
            generics.make_where_clause().predicates.push(parse_quote!(
                #name #ty_generics: co3::rust_spec::RustSpec<
                    Niche = co3::rust_spec::niche::WithNiche<co3::rust_spec::Unstable>
                >
            ));
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            quote! {
                impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
                    const NICHE: Self::CType = #value;
                }
            }
        } else {
            let mut generics = input.generics.clone();
            let speculative = (input.generics.type_params().count() == 0).then_some(quote!(for<'_dummy>));
            generics.make_where_clause().predicates.push(parse_quote!(#speculative #field_ty: co3::niche::Niche));
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            quote! {
                impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
                    const NICHE: Self::CType = <#field_ty as co3::niche::Niche>::NICHE;
                }
            }
        };

        quote! {
            #niche_impl

            const _: () = {
                use co3::stored::EncodeOwned;

                co3::disjoint_impls! {
                    #[disjoint_impls(remote)]
                    #[allow(clippy::missing_safety_doc)]
                    pub unsafe trait EncodeOwned: co3::ReprC<CType: Sized> + Sized {
                        type Store: co3::stored::Store + Default;

                        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
                        where Self: 'itm;
                    }

                    unsafe impl #impl_generics EncodeOwned for #name #ty_generics
                    where
                        #speculative #name #ty_generics: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::NoDrop>,
                        #field_ty: co3::stored::EncodeOwned,
                        #(#niche_bounds,)*
                        #predicates
                    {
                        type Store = <#field_ty as co3::stored::EncodeOwned>::Store;

                        fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType
                        where Self: '_išč {
                            let owner = core::mem::ManuallyDrop::new(self);
                            let value = unsafe { core::ptr::read(&raw const (*owner).#member) };
                            let encoded = co3::stored::EncodeOwned::soft_encode(value, store);
                            #encode_niche_check
                            encoded
                        }
                    }

                    unsafe impl #impl_generics EncodeOwned for #name #ty_generics
                    where
                        #speculative #name #ty_generics: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>,
                        #field_ty: co3::stored::EncodeOwned,
                        #(#niche_bounds,)*
                        #predicates
                    {
                        type Store = <#field_ty as co3::stored::EncodeOwned>::Store;

                        fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType
                        where Self: '_išč {
                            let owner = core::mem::ManuallyDrop::new(self);
                            let value = unsafe { core::ptr::read(&raw const (*owner).#member) };
                            let encoded = co3::stored::EncodeOwned::soft_encode(value, store);
                            #encode_niche_check
                            encoded
                        }
                    }

                    #custom_impl
                }
            };

            unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč>
                for #name #ty_generics #decode_where_clause
            {
                type Store = <#field_ty as co3::stored::DecodeOwned<'_dšč>>::Store;

                unsafe fn soft_decode<'_išč: '_dšč>(
                    source: Self::CType,
                    store: &'_išč mut Self::Store,
                ) -> Option<Self> {
                    #niche_rejection
                    let value = unsafe { <#field_ty as co3::stored::DecodeOwned<'_dšč>>::soft_decode(source, store)? };
                    #decode_custom_validation
                    Some(#construct)
                }
            }

            impl #impl_generics co3::Encode for #name #ty_generics
            where #name #ty_generics: co3::stored::EncodeOwned, #predicates {}
            impl #decode_impl_generics co3::Decode<'_dšč> for #name #ty_generics #decode_where_clause {}
        }
    });

    quote! {
        impl #repr_impl_generics co3::ReprC for #name #ty_generics #repr_where_clause {
            type CType = <#field_ty as co3::ReprC>::CType;
        }

        #checked
        #codecs
    }
}
