//! Reference encoder generation for transparent wrappers and aggregate structs.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use super::is_parametrized;
use crate::utils::speculative_marker;

/// Select the reference trait, value trait, store, and their encoding methods.
pub(super) fn encoder_names(
    mutable: bool,
    unchecked: bool,
) -> (
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
) {
    let borrow = if mutable { "Mut" } else { "Ref" };
    let suffix = if unchecked { "Unchecked" } else { "" };
    let trait_name = format_ident!("Encode{borrow}{suffix}");
    let move_trait = format_ident!("EncodeMove{suffix}");
    let store = if unchecked {
        quote!(SoftStore)
    } else {
        quote!(Store)
    };
    let borrow = if mutable { "mut" } else { "ref" };
    let suffix = if unchecked { "_unchecked" } else { "" };
    let method = format_ident!("encode_{borrow}{suffix}");
    let move_method = format_ident!("soft_encode{suffix}");
    (
        quote!(co3::stored::#trait_name),
        quote!(co3::stored::#move_trait),
        store,
        quote!(#method),
        quote!(#move_method),
    )
}

fn encoding_call(call: TokenStream, unchecked: bool) -> TokenStream {
    if unchecked {
        quote!(unsafe { #call })
    } else {
        call
    }
}

/// Encode stable, robust sized referents directly through their reference carrier.
pub(super) fn gen_stable_direct_mut_impls(input: &syn::DeriveInput) -> TokenStream {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    generics.params.insert(0, parse_quote!('_ršč));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(input.generics.params.is_empty());

    // The reference encoder uses the source's sized C carrier. For views,
    // each field here is already `<Field as Borrow>::Borrowed`.
    let fields: Vec<_> = match &input.data {
        syn::Data::Struct(data) => data.fields.iter().collect(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter())
            .collect(),
        syn::Data::Union(_) => unreachable!(),
    };
    let field_bounds = fields
        .into_iter()
        .filter_map(|field| {
            let ty = &field.ty;
            if super::is_phantom_data(ty) {
                return None;
            }
            let speculative = speculative_marker(!super::is_parametrized(ty, &input.generics));
            Some(quote!(#speculative #ty: co3::ReprC<CType: Sized>,))
        })
        .collect::<Vec<_>>();

    let implementations = [true, false].into_iter().map(|unchecked| {
        let (trait_name, move_trait, store_name, method, move_method) =
            encoder_names(true, unchecked);
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let encode = encoding_call(quote!(<#name #ty_generics as #move_trait>::#move_method(value, fields)), unchecked);
        quote! {
            unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
            where
                #speculative Self: co3::transmute::AbiCompatibleValidate
                    + co3::rust_spec::RustSpec<Layout = co3::rust_spec::Stable, Trap = co3::rust_spec::layout::Robust>
                    + co3::ReprC<CType: co3::CType + Sized> + Sized + '_ršč,
                &'_ršč mut Self: co3::ReprC<CType = co3::reference::CRefMut<<Self as co3::ReprC>::CType, true>>
                    + co3::transmute::AbiCompatibleValidate,
                #(#field_bounds)*
                #predicates
            {
                type #store_name = ();
                #unsafe_fn fn #method<'itm>(&'_ršč mut self, _: &'itm mut ()) -> <&'_ršč mut Self as co3::ReprC>::CType
                where '_ršč: 'itm {
                    // Do not delegate back to EncodeMove for &mut Self: its stored
                    // branch requires EncodeMut and would create a trait-solver cycle.
                    let encoded = unsafe {
                        co3::reference::CRefMut::from_raw(core::ptr::from_mut(self).cast())
                    };
                    debug_assert!(unsafe { <&'_ršč mut Self as co3::transmute::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
                    encoded
                }
            }
            unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
            where
                #speculative Self: co3::rust_spec::RustSpec<Layout = co3::rust_spec::Stable, Trap = co3::rust_spec::layout::NonRobust>
                    + co3::ReprC<CType: co3::CType + Sized> + Clone + #move_trait
                    + co3::stored::DecodeMove<'_ršč, Store: co3::stored::EmptyStore + '_ršč> + '_ršč,
                &'_ršč mut Self: co3::ReprC<CType = co3::reference::CRefMut<<Self as co3::ReprC>::CType, true>>,
                #(#field_bounds)*
                #predicates
            {
                type #store_name = co3::sync::RefMutSizedEncodeStore<'_ršč, Self, <Self as #move_trait>::#store_name>;
                #unsafe_fn fn #method<'itm>(&'_ršč mut self, store: &'itm mut Self::#store_name) -> <&'_ršč mut Self as co3::ReprC>::CType
                where '_ršč: 'itm {
                    store.encode(self, |original, fields| {
                        let value = Self::clone(original);
                        #encode
                    })
                }
            }

        }
    });
    group_aggregate_ref_impls(quote!(#(#implementations)*), true)
}

fn field_member(index: usize, field: &syn::Field) -> syn::Member {
    field
        .ident
        .clone()
        .map(syn::Member::Named)
        .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index)))
}

/// Keep the size marker and dispatch helpers inside an unnameable scope.
pub(super) fn group_aggregate_ref_impls(
    implementations: TokenStream,
    dispatch: bool,
) -> TokenStream {
    let extern_thin = (!dispatch).then(|| {
        quote! {
            impl Thin for co3::rust_spec::size::ExternTypeLike {}
        }
    });
    let thin = quote! {
        trait Thin {}
        impl<K: co3::rust_spec::size::SizedKind> Thin for co3::rust_spec::size::Sized<K> {}
        #extern_thin
    };
    if !dispatch {
        return quote!(const _: () = { #thin #implementations };);
    }
    let file: syn::File = syn::parse2(implementations).expect("reference encoder impls");
    let mut groups: [Vec<TokenStream>; 4] = Default::default();
    for item in file.items {
        let syn::Item::Impl(mut implementation) = item else {
            unreachable!("reference generators emit impls");
        };
        let path = &mut implementation.trait_.as_mut().expect("trait impl").0;
        let segment = path.segments.last().unwrap().clone();
        let index = match segment.ident.to_string().as_str() {
            "EncodeRefUnchecked" => 0,
            "EncodeRef" => 1,
            "EncodeMutUnchecked" => 2,
            "EncodeMut" => 3,
            _ => unreachable!("reference encoder trait"),
        };
        *path = parse_quote!(#segment);
        groups[index].push(quote!(#implementation));
    }
    let blocks = groups.into_iter().enumerate().filter_map(|(index, implementations)| {
        if implementations.is_empty() {
            return None;
        }
        let mutable = index >= 2;
        let unchecked = index % 2 == 0;
        let (trait_path, _, store_name, method, _) = encoder_names(mutable, unchecked);
        let path: syn::Path = syn::parse2(trait_path.clone()).expect("encoder path");
        let name = &path.segments.last().unwrap().ident;
        if implementations.len() == 1 {
            return Some(quote! {
                use #trait_path;
                #(#implementations)*
            });
        }
        let mutability = mutable.then(|| quote!(mut));
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let supertrait = match (mutable, unchecked) {
            (_, true) => quote!(co3::ReprC + 'a),
            (false, false) => quote!(co3::stored::EncodeRefUnchecked<'a>),
            (true, false) => quote!(co3::stored::EncodeMutUnchecked<'a>),
        };
        Some(quote! {
            use #trait_path;
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait #name<'a>: #supertrait {
                    type #store_name: co3::stored::Store + Default;
                    #unsafe_fn fn #method<'itm>(&'a #mutability self, store: &'itm mut Self::#store_name) -> <&'a #mutability Self as co3::ReprC>::CType
                    where
                        &'a #mutability Self: co3::ReprC<CType: Sized>,
                        'a: 'itm;
                }
                #(#implementations)*
            }
        })
    });
    quote! {
        #[allow(clippy::missing_safety_doc)]
        const _: () = {
            #thin
            #(#blocks)*
        };
    }
}

/// Transparent wrappers delegate reference encoding to the borrowed inner field.
pub(super) fn gen_field_ref_impls(
    input: &syn::DeriveInput,
    field: &syn::Field,
    variant: Option<&Ident>,
) -> TokenStream {
    let name = &input.ident;
    let field_ty = &field.ty;
    let member = field_member(0, field);
    let mut generics = input.generics.clone();
    generics.params.insert(0, parse_quote!('_ršč));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(input.generics.params.is_empty());
    let field_speculative = speculative_marker(!is_parametrized(field_ty, &input.generics));

    let mut implementations = Vec::new();
    for mutable in [false, true] {
        for unchecked in [true, false] {
            let (trait_name, move_trait, store_name, method, encode) =
                encoder_names(mutable, unchecked);
            let mutability = mutable.then(|| quote!(mut));
            let unsafe_fn = unchecked.then(|| quote!(unsafe));
            let borrow = match (variant, &field.ident) {
                (None, _) => quote!(&#mutability self.#member),
                (Some(variant), Some(ident)) => {
                    quote!(match self { Self::#variant { #ident: field } => field })
                }
                (Some(variant), None) => quote!(match self { Self::#variant(field) => field }),
            };
            let call = encoding_call(quote!(#move_trait::#encode(#borrow, store)), unchecked);
            implementations.push(quote! {
                unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
                where
                    #speculative Self: co3::ReprC<CType = <#field_ty as co3::ReprC>::CType> + '_ršč,
                    #field_speculative #field_ty: co3::ReprC + '_ršč,
                    &'_ršč #mutability #field_ty: #move_trait + co3::ReprC<CType: Sized>,
                    &'_ršč #mutability Self: co3::ReprC<CType = <&'_ršč #mutability #field_ty as co3::ReprC>::CType>,
                    #predicates
                {
                    type #store_name = <&'_ršč #mutability #field_ty as #move_trait>::#store_name;
                    #unsafe_fn fn #method<'itm>(&'_ršč #mutability self, store: &'itm mut Self::#store_name) -> <&'_ršč #mutability Self as co3::ReprC>::CType
                    where
                        '_ršč: 'itm,
                    {
                        #call
                    }
                }
            });
        }
    }
    quote!(#(#implementations)*)
}

/// Generate sized reference decoders without a blanket implementation.
pub(super) fn gen_sized_decode_impls(
    input: &syn::DeriveInput,
    repr: Option<&super::attr::ReprKind>,
    attrs: &super::ReprCAttrs,
    variant_attrs: &[super::VariantReprCAttrs],
) -> TokenStream {
    let shared = gen_structural_decode(input, repr, attrs, variant_attrs, false);
    let mutable = gen_structural_decode(input, repr, attrs, variant_attrs, true);
    quote!(#shared #mutable)
}

/// Decode each active field through its shared reference decoder. No owning
/// aggregate carrier is copied or borrow-cast, including for CustomDrop types.
fn gen_structural_decode(
    input: &syn::DeriveInput,
    repr: Option<&super::attr::ReprKind>,
    attrs: &super::ReprCAttrs,
    variant_attrs: &[super::VariantReprCAttrs],
    mutable: bool,
) -> TokenStream {
    use super::attr::ReprKind;
    // Identity carriers are stable and already use direct reference decoding.
    if attrs.is_identity {
        return quote!();
    }
    let name = &input.ident;
    let carrier = if attrs.is_view {
        super::borrow::gen_view_ctype_name(name)
    } else {
        super::ctype::gen_ctype_name(name)
    };
    let mut generics = input.generics.clone();
    generics.params.insert(0, parse_quote!('_dšč));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let speculative = speculative_marker(input.generics.params.is_empty());
    let mut types = Vec::new();
    let mut stores = Vec::new();
    let mut bounds = Vec::new();
    let records: Vec<_> = match &input.data {
        syn::Data::Struct(data) => vec![(quote!(Self), &data.fields, attrs.is_valid.as_ref())],
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .zip(variant_attrs)
            .map(|(v, a)| {
                let vname = &v.ident;
                (quote!(Self::#vname), &v.fields, a.is_valid.as_ref())
            })
            .collect(),
        syn::Data::Union(_) => unreachable!(),
    };
    let transparent_enum = matches!(&input.data, syn::Data::Enum(data)
        if super::is_transparent_enum_repr(repr, &data.variants));
    let tagged_enum = matches!(&input.data, syn::Data::Enum(_)) && !transparent_enum;
    let inline_tag = tagged_enum && !matches!(repr, Some(ReprKind::C(Some(_))));
    let mut bodies = [Vec::new(), Vec::new()];
    for (head, fields, validate) in records {
        let mut values = Vec::new();
        let mut calls = [Vec::new(), Vec::new()];
        for (i, field) in fields.iter().enumerate() {
            let ty = &field.ty;
            let index = syn::Index::from(types.len());
            types.push(ty);
            let spec = speculative_marker(!is_parametrized(ty, &input.generics));
            let repr_bound = if super::is_phantom_data(ty) {
                quote!(co3::ReprC<CType = #ty>)
            } else if tagged_enum || mutable {
                quote!(co3::ReprC<CType: co3::CType + Copy + Sized>)
            } else {
                quote!(co3::ReprC<CType: co3::CType + Sized>)
            };
            let field_decoder = if mutable {
                quote!(co3::borrow::Borrowed<#ty>)
            } else {
                quote!(&'_dšč #ty)
            };
            if mutable {
                bounds.push(quote! {
                    #spec #ty: Sized,
                    #spec #ty: #repr_bound + '_dšč,
                    #spec #field_decoder: co3::stored::DecodeMove<'_dšč> + co3::ReprC<
                        CType = co3::borrow::Borrowed<<#ty as co3::ReprC>::CType>>,
                });
            } else {
                bounds.push(quote! {
                    #spec #ty: Clone + #repr_bound + '_dšč,
                    &'_dšč #ty: co3::stored::DecodeMove<'_dšč>
                        + co3::ReprC<CType = co3::reference::CRef<<#ty as co3::ReprC>::CType>>,
                });
            }
            stores.push(quote!(Option<<#field_decoder as co3::stored::DecodeMove<'_dšč>>::Store>));
            let member = if field.ident.is_some() {
                field_member(i, field)
            } else {
                syn::Member::Unnamed(syn::Index::from(i + usize::from(inline_tag)))
            };
            let var = format_ident!("__co3_field_{i}");
            values.push(var.clone());
            for (unchecked, calls) in calls.iter_mut().enumerate() {
                let method = if unchecked == 1 {
                    quote!(soft_decode_unchecked)
                } else {
                    quote!(soft_decode)
                };
                let check = (unchecked == 0).then(|| quote!(?));
                let carrier = if mutable {
                    // Only the C field is copied; the transparent wrapper marks
                    // borrowed ownership without a layout cast.
                    quote!(co3::borrow::Borrowed::new(source.#member))
                } else {
                    quote!(co3::reference::CRef::from_ref(&source.#member))
                };
                let decode = quote!(unsafe {
                    <#field_decoder as co3::stored::DecodeMove<'_dšč>>::#method(
                        #carrier, fields.#index.insert(Default::default()),
                    )
                }#check);
                let decode = if mutable {
                    quote!(co3::borrow::Borrowed::into_inner(#decode))
                } else {
                    quote!(<#ty as Clone>::clone(#decode))
                };
                calls.push(quote!(let #var = #decode;));
            }
        }
        let construct = match fields {
            syn::Fields::Named(_) => {
                let members = fields.iter().map(|f| &f.ident);
                quote!(#head { #(#members: #values),* })
            }
            syn::Fields::Unnamed(_) => quote!(#head(#(#values),*)),
            syn::Fields::Unit => quote!(#head),
        };
        let validation = validate.map(|v| {
            let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
            let args = values.iter().map(|v| quote!(&#v)).collect::<Vec<_>>();
            let call = super::gen_is_valid_call(v, &field_types, &args);
            quote!(if !#call { return None; })
        });
        for (i, calls) in calls.into_iter().enumerate() {
            let validation = (i == 0).then_some(&validation);
            bodies[i].push(quote! { #(#calls)* #validation Some(#construct) });
        }
    }
    let methods = bodies.into_iter().enumerate().map(|(unchecked, bodies)| {
        let body = if let syn::Data::Enum(data) = &input.data && tagged_enum {
            let tag_ty = super::enum_tag_type(repr, data.variants.len());
            let tags = super::enum_variant_tag_exprs(&data.variants, &tag_ty);
            let arms = data.variants.iter().zip(tags).zip(bodies).map(|((v, tag), body)| {
                let v = &v.ident;
                let payload = if inline_tag { quote!(&source.#v) } else { quote!(&source.payload.#v) };
                quote!(tag if tag == #tag => {
                    let source = unsafe { #payload };
                    #body
                })
            });
            quote! {
                // Only the Copy discriminant is read before selecting the active payload.
                let tag = unsafe { core::ptr::from_ref(source).cast::<#tag_ty>().read() };
                match tag { #(#arms,)* _ => None }
            }
        } else { bodies.into_iter().next().unwrap_or_else(|| quote!(None)) };
        let method = match (mutable, unchecked) {
            (false, 0) => quote!(decode_ref), (false, _) => quote!(decode_ref_unchecked),
            (true, 0) => quote!(decode_mut), (true, _) => quote!(decode_mut_unchecked),
        };
        let mutability = mutable.then(|| quote!(mut));
        let ret = if unchecked == 1 { quote!(&'_dšč #mutability Self) }
            else { quote!(Option<&'_dšč #mutability Self>) };
        let source = if mutable {
            if unchecked == 1 { quote!(&*pointer) } else { quote!(pointer.as_ref()?) }
        } else if unchecked == 1 { quote!(source.into_ref().unwrap_unchecked()) } else { quote!(source.into_ref()?) };
        let finish = (unchecked == 1).then(|| quote!(.expect("unchecked decoding produces a value")));
        let decode = if mutable { quote!(unsafe { store.decode(pointer, |fields| { #body }) }) }
            else { quote!(store.decode(|fields| { #body })) };
        quote! {
            unsafe fn #method<'itm: '_dšč>(source: <&'_dšč #mutability Self as co3::ReprC>::CType, store: &'itm mut Self::Store) -> #ret {
                let pointer = source.as_ptr();
                let source = unsafe { #source };
                #decode #finish
            }
        }
    });
    let trait_name = if mutable {
        quote!(DecodeMut)
    } else {
        quote!(DecodeRef)
    };
    let store = if mutable {
        quote!(RefMutSizedDecodeStore)
    } else {
        quote!(RefSizedDecodeStore)
    };
    let mutability = mutable.then(|| quote!(mut));
    let reference = if mutable {
        quote!(co3::reference::CRefMut<<Self as co3::ReprC>::CType, true>)
    } else {
        quote!(co3::reference::CRef<<Self as co3::ReprC>::CType>)
    };
    let writeback = mutable.then(
        || quote!(#speculative Self: co3::stored::EncodeMove<Store: co3::stored::EmptyStore>,),
    );
    quote! {
        unsafe impl #impl_generics co3::stored::#trait_name<'_dšč> for #name #ty_generics
        where
            #speculative Self: co3::ReprC<CType = #carrier #ty_generics>
                + co3::rust_spec::RustSpec<Layout = co3::rust_spec::Unstable> + Sized + '_dšč,
            &'_dšč #mutability Self: co3::ReprC<CType = #reference>,
            #writeback
            #(#bounds)*
            #predicates
        {
            type Store = co3::sync::#store<Self, (#(#stores,)*)>;
            #(#methods)*
        }
    }
}
