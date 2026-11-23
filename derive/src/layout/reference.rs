//! Reference encoder generation for transparent wrappers and aggregate structs.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use super::fields::FieldType;
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
    let trait_name = format_ident!("SoftEncode{borrow}{suffix}");
    let move_trait = format_ident!("EncodeMove{suffix}");
    let store = if unchecked {
        quote!(SoftStore)
    } else {
        quote!(Store)
    };
    let borrow = if mutable { "mut" } else { "ref" };
    let suffix = if unchecked { "_unchecked" } else { "" };
    let method = format_ident!("soft_encode_{borrow}{suffix}");
    let move_method = format_ident!("soft_encode{suffix}");
    (
        quote!(co3::convert::#trait_name),
        quote!(co3::convert::#move_trait),
        store,
        quote!(#method),
        quote!(#move_method),
    )
}

pub(super) fn encoder_output(reference: TokenStream) -> TokenStream {
    quote!(<#reference as co3::ReprC>::CType)
}

fn encoding_call(call: TokenStream, unchecked: bool) -> TokenStream {
    if unchecked {
        quote!(unsafe { #call })
    } else {
        call
    }
}

/// Encode stable sized referents directly when robust or unchecked.
pub(super) fn gen_stable_direct_mut_impls(input: &syn::DeriveInput) -> TokenStream {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    generics.params.insert(0, parse_quote!('_ršč));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(input.generics.params.is_empty());

    // The selected reference carrier distinguishes the branches. Do not repeat
    // the Layout/Trap conditions already used by native reference selection.
    let encoders = [true, false];
    let implementations = encoders.into_iter().map(|unchecked| {
        let (trait_name, move_trait, store_name, method, _) =
            encoder_names(true, unchecked);
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let output = encoder_output(quote!(&'_ršč mut Self));
        let carrier_bound = if unchecked {
            quote!(co3::ReprC<CType: co3::reference::abi::RefMutCarrier<Pointee = <Self as co3::ReprC>::CType>>)
        } else {
            quote!(co3::ReprC<CType = co3::reference::abi::RefMut<'_ršč, <Self as co3::ReprC>::CType>>)
        };
        let abi_bound = if unchecked {
            quote!(co3::marker::AbiCompatible)
        } else {
            quote!(co3::convert::AbiCompatibleValidate)
        };
        let validation = (!unchecked).then(|| quote! {
            debug_assert!(unsafe { <&'_ršč mut Self as co3::convert::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
        });
        let encode = quote!(co3::convert::EncodeMove::soft_encode(value, fields));
        let nonrobust = (!unchecked).then(|| quote! {
            unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
            where
                #speculative Self: Clone + #move_trait + co3::marker::AbiCompatible
                    + co3::convert::DecodeMove<'_ršč, Store: co3::convert::EmptyStore + '_ršč> + '_ršč,
                &'_ršč mut Self: co3::ReprC<CType = co3::reference::abi::SoftRefMut<<Self as co3::ReprC>::CType>> + co3::marker::AbiCompatible,
                #predicates
            {
                type #store_name = co3::sync::RefMutSizedEncodeStore<'_ršč, Self, <Self as #move_trait>::#store_name>;
                #unsafe_fn fn #method<'itm>(&'_ršč mut self, store: &'itm mut Self::#store_name) -> #output
                where '_ršč: 'itm {
                    store.encode(self, |original, fields| {
                        let value = Self::clone(original);
                        #encode
                    })
                }
            }
        });
        quote! {
            unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
            where
                #speculative <Self as co3::ReprC>::CType: Sized,
                #speculative Self: #abi_bound + Sized + '_ršč,
                &'_ršč mut Self: #carrier_bound + #abi_bound,
                #predicates
            {
                type #store_name = ();
                #unsafe_fn fn #method<'itm>(&'_ršč mut self, _: &'itm mut ()) -> #output
                where '_ršč: 'itm {
                    // Do not delegate back to EncodeMove for &mut Self: its stored
                    // branch requires SoftEncodeMut and would create a trait-solver cycle.
                    let encoded = unsafe {
                        co3::reference::abi::RefMutCarrier::from_raw(core::ptr::from_mut(self).cast())
                    };
                    #validation
                    encoded
                }
            }
            #nonrobust


        }
    });
    group_aggregate_ref_impls(quote!(#(#implementations)*), false)
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
    field_helpers: bool,
) -> TokenStream {
    let field_encoders = field_helpers.then(gen_shared_field_encoders);
    let file: syn::File = syn::parse2(implementations).expect("reference encoder impls");
    let mut groups: [Vec<TokenStream>; 4] = Default::default();
    for item in file.items {
        let syn::Item::Impl(mut implementation) = item else {
            unreachable!("reference generators emit impls");
        };
        let path = &mut implementation.trait_.as_mut().expect("trait impl").0;
        let segment = path.segments.last().unwrap().clone();
        let index = match segment.ident.to_string().as_str() {
            "SoftEncodeRefUnchecked" => 0,
            "SoftEncodeRef" => 1,
            "SoftEncodeMutUnchecked" => 2,
            "SoftEncodeMut" => 3,
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
        let output = encoder_output(quote!(&'a #mutability Self));
        let supertrait = match (mutable, unchecked) {
            (_, true) => quote!(co3::ReprC + 'a),
            (false, false) => quote!(co3::convert::SoftEncodeRefUnchecked<'a>),
            (true, false) => quote!(co3::convert::SoftEncodeMutUnchecked<'a>),
        };
        Some(quote! {
            use #trait_path;
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                #[allow(clippy::missing_safety_doc, reason = "internal conversion trait generated by ReprC")]
                pub unsafe trait #name<'a>: #supertrait
                where &'a #mutability Self: co3::ReprC<CType: Sized>
                {
                    type #store_name: co3::convert::Store + Default;
                    #unsafe_fn fn #method<'itm>(&'a #mutability self, store: &'itm mut Self::#store_name) -> #output
                    where
                        &'a #mutability Self: co3::ReprC<CType: Sized>,
                        'a: 'itm;
                }
                #(#implementations)*
            }
        })
    });
    quote! {
        const _: () = {
            #field_encoders
            #(#blocks)*
        };
    }
}

/// Generate sized reference decoders without a blanket implementation.
pub(super) fn gen_sized_decode_impls(
    input: &syn::DeriveInput,
    repr: Option<&super::attr::ReprKind>,
    attrs: &super::ReprCAttrs,
    variant_attrs: &[super::VariantReprCAttrs],
    equalities: &[syn::WherePredicate],
) -> TokenStream {
    let transparent = matches!(repr, Some(super::attr::ReprKind::Transparent))
        && !attrs.is_identity
        && attrs.is_valid.is_none()
        && variant_attrs.iter().all(|attrs| attrs.is_valid.is_none());
    let field = if transparent {
        match &input.data {
            syn::Data::Struct(data) if data.fields.len() == 1 => {
                Some((data.fields.iter().next().unwrap(), None))
            }
            syn::Data::Enum(data)
                if data.variants.len() == 1 && data.variants[0].fields.len() == 1 =>
            {
                Some((
                    data.variants[0].fields.iter().next().unwrap(),
                    Some(&data.variants[0].ident),
                ))
            }
            _ => None,
        }
    } else {
        None
    };
    // PhantomData is reconstructed directly by the structural decoder.
    let shared = if let Some((field, variant)) =
        field.filter(|(field, _)| !super::is_phantom_data(&field.ty))
    {
        gen_transparent_decode_ref(input, field, variant)
    } else {
        gen_structural_decode(input, repr, attrs, variant_attrs, false, equalities)
    };
    let mutable = gen_structural_decode(input, repr, attrs, variant_attrs, true, equalities);
    quote! {
        const _: () = {
            trait __Co3NonCustomDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::NoDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::AutoDrop {}

            #shared
            #mutable
        };
    }
}

/// Decode a transparent Rust wrapper by borrowing the decoded field.
fn gen_transparent_decode_ref(
    input: &syn::DeriveInput,
    field: &syn::Field,
    variant: Option<&Ident>,
) -> TokenStream {
    let name = &input.ident;
    let companion = super::ctype::gen_ctype_name(name);
    let (_, args, _) = input.generics.split_for_impl();
    let carrier_bound = quote!(co3::ReprC<CType = #companion #args>);
    let ty = &field.ty;
    let member = field_member(0, field);
    let speculative = speculative_marker(input.generics.params.is_empty());
    let predicates = input.generics.where_clause.as_ref().map(|w| &w.predicates);
    let mut generics = input.generics.clone();
    generics.params.insert(0, parse_quote!('_co3_decode));
    let (decode_generics, _, _) = generics.split_for_impl();
    let store_param = (0..)
        .map(|i| format_ident!("__Co3FieldStore{i}"))
        .find(|name| !input.generics.type_params().any(|p| p.ident == *name))
        .unwrap();
    let mut take_generics = input.generics.clone();
    take_generics.params.push(parse_quote!(#store_param));
    let (take_generics, _, _) = take_generics.split_for_impl();
    let constructor = match (variant, &field.ident) {
        (None, None) => quote!(#name(value)),
        (None, Some(member)) => quote!(#name { #member: value }),
        (Some(variant), None) => quote!(#name::#variant(value)),
        (Some(variant), Some(member)) => quote!(#name::#variant { #member: value }),
    };
    let field_bound = FieldType::new(ty, &input.generics).bound(quote!(
        co3::ReprC<CType: Sized + '_co3_decode> + Sized + '_co3_decode
    ));
    let field_impl = quote! {
            impl<'d, R, L> __Co3TransparentFieldDecode<'d, R> for L
            where
                R: co3::ReprC<CType: Sized + 'd>
                    + co3::rust_spec::RustSpec<Layout = L> + Sized + 'd,
                &'d R: co3::convert::DecodeMove<'d>
                    + co3::ReprC<CType: co3::reference::abi::RefCarrier<Pointee = R::CType>>,
            {
                unsafe fn decode<'itm: 'd>(
                    source: &'d R::CType,
                    store: &'itm mut <&'d R as co3::convert::DecodeMove<'d>>::Store,
                ) -> Option<&'d R> {
                    let carrier = unsafe { co3::reference::abi::RefCarrier::from_raw(core::ptr::from_ref(source)) };
                    unsafe { <&'d R as co3::convert::DecodeMove<'d>>::soft_decode(carrier, store) }
                }

                unsafe fn decode_unchecked<'itm: 'd>(
                    source: &'d R::CType,
                    store: &'itm mut <&'d R as co3::convert::DecodeMove<'d>>::Store,
                ) -> &'d R {
                    let carrier = unsafe { co3::reference::abi::RefCarrier::from_raw(core::ptr::from_ref(source)) };
                    unsafe { <&'d R as co3::convert::DecodeMove<'d>>::soft_decode_unchecked(carrier, store) }
                }
            }
    };
    quote! {
        const _: () = {
            use co3::convert::SoftDecodeRef;

            trait __Co3TransparentFieldDecode<'d, R>
            where
                R: co3::ReprC<CType: Sized> + 'd,
                &'d R: co3::convert::DecodeMove<'d>,
            {
                unsafe fn decode<'itm: 'd>(
                    source: &'d R::CType,
                    store: &'itm mut <&'d R as co3::convert::DecodeMove<'d>>::Store,
                ) -> Option<&'d R>;
                unsafe fn decode_unchecked<'itm: 'd>(
                    source: &'d R::CType,
                    store: &'itm mut <&'d R as co3::convert::DecodeMove<'d>>::Store,
                ) -> &'d R;
            }

            #field_impl

            #[doc(hidden)]
            pub struct __Co3TransparentDecodeStore<R, S>(S, core::marker::PhantomData<fn() -> R>);

            impl<R, S: Default> Default for __Co3TransparentDecodeStore<R, S> {
                fn default() -> Self {
                    Self(S::default(), core::marker::PhantomData)
                }
            }

            impl<R, S: co3::convert::Store> co3::convert::Store for __Co3TransparentDecodeStore<R, S> {
                fn sync(self) -> Option<()> {
                    self.0.sync()
                }
            }

            // This adapter adds no backing storage; reference validity comes from the field store.
            unsafe impl<R, S: co3::convert::EmptyStore> co3::convert::EmptyStore
                for __Co3TransparentDecodeStore<R, S> {}

            unsafe impl #take_generics co3::sync::TakeDecoded
                for __Co3TransparentDecodeStore<#name #args, #store_param>
            where
                #speculative #ty: Sized,
                #name #args: Sized,
                #store_param: co3::sync::TakeDecoded<Value = #ty>,
                #predicates
            {
                type Value = #name #args;

                unsafe fn take_decoded(store: *mut Self) -> Option<Self::Value> {
                    // Extract exactly once, preserving the inner store's extraction contract.
                    let value = unsafe {
                        #store_param::take_decoded(core::ptr::addr_of_mut!((*store).0))?
                    };
                    Some(#constructor)
                }
            }

            unsafe impl #decode_generics SoftDecodeRef<'_co3_decode> for #name #args
            where
                Self: co3::rust_spec::RustSpec,
                #speculative Self: #carrier_bound + Sized + '_co3_decode,
                #speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop,
                #ty: co3::ReprC + co3::rust_spec::RustSpec,
                #field_bound,
                &'_co3_decode Self: co3::ReprC<CType = co3::reference::abi::SoftRef<<Self as co3::ReprC>::CType>>,
                &'_co3_decode #ty: co3::convert::DecodeMove<'_co3_decode>,
                <#ty as co3::rust_spec::RustSpec>::Layout:
                    __Co3TransparentFieldDecode<'_co3_decode, #ty>,
                #predicates
            {
                type Store = __Co3TransparentDecodeStore<
                    Self, <&'_co3_decode #ty as co3::convert::DecodeMove<'_co3_decode>>::Store,
                >;

                unsafe fn soft_decode_ref<'itm: '_co3_decode>(
                    source: <&'_co3_decode Self as co3::ReprC>::CType,
                    store: &'itm mut Self::Store,
                ) -> Option<&'_co3_decode Self> {
                    let source = unsafe { source.into_ref()? };
                    let value = unsafe {
                        <<#ty as co3::rust_spec::RustSpec>::Layout as __Co3TransparentFieldDecode<'_co3_decode, #ty>>::decode(
                            &source.#member, &mut store.0,
                        )?
                    };
                    // Both Rust types are transparent over this single field. The field
                    // decoder established validity; the reference keeps its original lifetime.
                    Some(unsafe { &*core::ptr::from_ref(value).cast::<Self>() })
                }

                unsafe fn soft_decode_ref_unchecked<'itm: '_co3_decode>(
                    source: <&'_co3_decode Self as co3::ReprC>::CType,
                    store: &'itm mut Self::Store,
                ) -> &'_co3_decode Self {
                    let source = unsafe { source.into_ref().unwrap_unchecked() };
                    let value = unsafe {
                        <<#ty as co3::rust_spec::RustSpec>::Layout as __Co3TransparentFieldDecode<'_co3_decode, #ty>>::decode_unchecked(
                            &source.#member, &mut store.0,
                        )
                    };
                    unsafe { &*core::ptr::from_ref(value).cast::<Self>() }
                }
            }
        };
    }
}

/// Decode each active field through its shared reference decoder. No owning
/// aggregate carrier is copied or borrow-cast. Custom-drop owners are excluded.
fn gen_structural_decode(
    input: &syn::DeriveInput,
    repr: Option<&super::attr::ReprKind>,
    attrs: &super::ReprCAttrs,
    variant_attrs: &[super::VariantReprCAttrs],
    mutable: bool,
    equalities: &[syn::WherePredicate],
) -> TokenStream {
    use super::attr::ReprKind;
    // Identity carriers are stable and already use direct reference decoding.
    if attrs.is_identity {
        return quote!();
    }
    let name = &input.ident;
    let carrier = super::ctype::gen_ctype_name(name);
    let (_, args, _) = input.generics.split_for_impl();
    let carrier_bound = quote!(co3::ReprC<CType = #carrier #args>);
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
            let field_type = FieldType::new(ty, &input.generics);
            let index = syn::Index::from(types.len());
            types.push(ty);
            let repr_bound = if field_type.phantom {
                let known = crate::known_type::recognize(ty).unwrap();
                let inner = known.inner.unwrap();
                quote!(co3::ReprC<CType = core::marker::PhantomData<<#inner as co3::ReprC>::CType>>)
            } else if tagged_enum || mutable {
                quote!(co3::ReprC<CType: Copy>)
            } else {
                quote!(co3::ReprC<CType: Sized>)
            };
            let field_bound = field_type.bound(quote!(#repr_bound + '_dšč));
            let field_decoder = if mutable {
                quote!(#ty)
            } else {
                quote!(&'_dšč #ty)
            };
            if mutable {
                let sized_bound = field_type.bound(quote!(Sized));
                let spec = speculative_marker(field_type.concrete);
                bounds.push(quote! {
                    #sized_bound,
                    #field_bound,
                    #spec #field_decoder: co3::borrow::BorrowedDecode<'_dšč>,
                });
            } else if !field_type.phantom {
                let decode_bound = field_type.bound(quote!(__Co3DecodeField<'_dšč>));
                bounds.push(quote! {
                    #field_bound,
                    #decode_bound,
                });
            }
            if mutable {
                stores.push(
                    quote!(Option<<#field_decoder as co3::borrow::BorrowedDecode<'_dšč>>::Store>),
                );
            } else if field_type.phantom {
                stores.push(quote!(()));
            } else {
                stores.push(quote!(Option<<#ty as __Co3DecodeField<'_dšč>>::Store>));
            }
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
                    if inline_tag {
                        quote!(unsafe { source.#member.assume_init() })
                    } else {
                        quote!(source.#member)
                    }
                } else {
                    if inline_tag {
                        quote!(unsafe { source.#member.assume_init_ref() })
                    } else {
                        quote!(&source.#member)
                    }
                };
                let decoder_trait = if mutable {
                    quote!(co3::borrow::BorrowedDecode)
                } else {
                    quote!(co3::convert::DecodeMove)
                };
                let decode = quote!(unsafe {
                    <#field_decoder as #decoder_trait<'_dšč>>::#method(
                        #carrier, fields.#index.insert(Default::default()),
                    )
                }#check);
                let decode = if mutable {
                    decode
                } else if field_type.phantom {
                    quote!(core::marker::PhantomData)
                } else {
                    {
                        let method = if unchecked == 1 {
                            quote!(decode_field_unchecked)
                        } else {
                            quote!(decode_field)
                        };
                        quote!(unsafe {
                            <#ty as __Co3DecodeField<'_dšč>>::#method(
                                #carrier, fields.#index.insert(Default::default()),
                            )
                        }#check)
                    }
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
                let payload = if inline_tag { quote!(&source.#v) } else { quote!(&source.payload.assume_init_ref().#v) };
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
            (false, 0) => quote!(soft_decode_ref), (false, _) => quote!(soft_decode_ref_unchecked),
            (true, 0) => quote!(soft_decode_mut), (true, _) => quote!(soft_decode_mut_unchecked),
        };
        let mutability = mutable.then(|| quote!(mut));
        let ret = if unchecked == 1 { quote!(&'_dšč #mutability Self) }
            else { quote!(Option<&'_dšč #mutability Self>) };
        let source = if mutable {
            if unchecked == 1 { quote!(&*pointer) } else { quote!(pointer.as_ref()?) }
        } else if unchecked == 1 { quote!(source.into_ref().unwrap_unchecked()) } else { quote!(source.into_ref()?) };
        let finish = (unchecked == 1).then(|| quote!(.expect("unchecked decoding produces a value")));
        // Construct the closure outside the unsafe call so user validators retain
        // their own safety context and explicit unsafe blocks remain necessary.
        let decode = if mutable {
            quote!({
                let __co3_decode = |fields: &'itm mut (#(#stores,)*)| { #body };
                unsafe { store.decode(pointer, __co3_decode) }
            })
        } else {
            quote!(store.decode(|fields| { #body }))
        };
        quote! {
            unsafe fn #method<'itm: '_dšč>(source: <&'_dšč #mutability Self as co3::ReprC>::CType, store: &'itm mut Self::Store) -> #ret {
                let pointer = source.as_ptr();
                let source = unsafe { #source };
                #decode #finish
            }
        }
    });
    let trait_name = if mutable {
        quote!(SoftDecodeMut)
    } else {
        quote!(SoftDecodeRef)
    };
    let store = if mutable {
        quote!(RefMutSizedDecodeStore)
    } else {
        quote!(RefSizedDecodeStore)
    };
    let mutability = mutable.then(|| quote!(mut));
    let reference = if mutable {
        quote!(co3::reference::abi::SoftRefMut<<Self as co3::ReprC>::CType>)
    } else {
        quote!(co3::reference::abi::SoftRef<<Self as co3::ReprC>::CType>)
    };
    let writeback = mutable.then(
        || quote!(#speculative Self: co3::convert::EncodeMove<Store: co3::convert::EmptyStore>,),
    );
    let implementation = quote! {
        unsafe impl #impl_generics #trait_name<'_dšč> for #name #ty_generics
        where
            Self: co3::rust_spec::RustSpec,
            #speculative Self: #carrier_bound + Sized + '_dšč,
            #speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop,
            &'_dšč #mutability Self: co3::ReprC<CType = #reference>,
            #writeback
            #(#equalities,)*
            #(#bounds)*
            #predicates
        {
            type Store = co3::sync::#store<Self, (#(#stores,)*)>;
            #(#methods)*
        }
    };
    if mutable {
        return quote! {
            const _: () = {
                use co3::convert::#trait_name;
                #implementation
            };
        };
    }
    let field_decoder = gen_shared_field_decoder();
    quote! {
        const _: () = {
            use co3::convert::#trait_name;
            #field_decoder
            #implementation
        };
    }
}

/// Dispatch each field independently, avoiding a branch for every combination of field layouts.
fn gen_shared_field_encoders() -> TokenStream {
    let traits = [true, false].into_iter().map(|unchecked| {
        let (name, move_trait, store_name, method, move_method) = if unchecked {
            (quote!(__Co3EncodeFieldUnchecked), quote!(co3::convert::EncodeMoveUnchecked), quote!(SoftStore), quote!(encode_field_unchecked), quote!(soft_encode_unchecked))
        } else {
            (quote!(__Co3EncodeField), quote!(co3::convert::EncodeMove), quote!(Store), quote!(encode_field), quote!(soft_encode))
        };
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let supertrait = if unchecked {
            quote!(co3::ReprC<CType: Copy> + 'a)
        } else {
            quote!(__Co3EncodeFieldUnchecked<'a>)
        };
            let encode = if unchecked { encoding_call(quote!(<&'a R as #move_trait>::#move_method(self, store)), true) } else { quote!(co3::convert::EncodeMove::soft_encode(self, store)) };
        let implementation = quote! {
                unsafe impl<'a, R: co3::ReprC + 'a> #name<'a> for R
                where
                    <R as co3::ReprC>::CType: Copy,
                    &'a R: #move_trait + co3::ReprC<CType: co3::reference::abi::RefCarrier<Pointee = <R as co3::ReprC>::CType>>,
                {
                    type #store_name = <&'a R as #move_trait>::#store_name;
                    #unsafe_fn fn #method<'itm>(&'a self, store: &'itm mut Self::#store_name) -> Self::CType
                    where 'a: 'itm,
                    {
                        let carrier = #encode;
                        // The exact shared carrier permits reading initialized storage;
                        // Copy prevents moving ownership out of the borrowed field.
                        unsafe { co3::reference::abi::RefCarrier::as_ptr(&carrier).read() }
                    }
                }
            }
        ;
        quote! {
                #[allow(clippy::missing_safety_doc, reason = "internal conversion trait generated by ReprC")]
                pub unsafe trait #name<'a>: #supertrait {
                    type #store_name: co3::convert::Store + Default;
                    #unsafe_fn fn #method<'itm>(&'a self, store: &'itm mut Self::#store_name) -> Self::CType
                    where 'a: 'itm;
                }
                #implementation
        }
    });
    quote!(#(#traits)*)
}

fn gen_shared_field_decoder() -> TokenStream {
    let layout = quote!(<R as co3::rust_spec::RustSpec>::Layout);
    let implementation = quote! {
            unsafe impl<'d, R: co3::ReprC + 'd> __Co3DecodeField<'d> for R
            where
                <R as co3::ReprC>::CType: Sized + 'd,
                R: co3::rust_spec::RustSpec,
                &'d R: co3::convert::DecodeMove<'d, Store: co3::convert::Store + Default + Sized>
                    + co3::ReprC<CType: co3::reference::abi::RefCarrier<Pointee = R::CType>>,
                co3::sync::DecodedField<'d, R, <&'d R as co3::convert::DecodeMove<'d>>::Store, #layout>:
                    co3::sync::TakeDecoded<Value = R>,
            {
                type Store = <&'d R as co3::convert::DecodeMove<'d>>::Store;
                unsafe fn decode_field<'itm: 'd>(source: &'d Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                    let carrier = unsafe { co3::reference::abi::RefCarrier::from_raw(core::ptr::from_ref(source)) };
                    unsafe { co3::sync::DecodedField::<R, _, #layout>::decode(carrier, store) }
                }
                unsafe fn decode_field_unchecked<'itm: 'd>(source: &'d Self::CType, store: &'itm mut Self::Store) -> Self {
                    let carrier = unsafe { co3::reference::abi::RefCarrier::from_raw(core::ptr::from_ref(source)) };
                    unsafe { co3::sync::DecodedField::<R, _, #layout>::decode_unchecked(carrier, store) }
                }
            }
    };
    quote! {
            #[allow(clippy::missing_safety_doc, reason = "internal conversion trait generated by ReprC")]
            pub unsafe trait __Co3DecodeField<'d>: co3::ReprC<CType: Sized + 'd> + Sized + 'd {
                type Store: co3::convert::Store + Default;
                unsafe fn decode_field<'itm: 'd>(source: &'d Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
                unsafe fn decode_field_unchecked<'itm: 'd>(source: &'d Self::CType, store: &'itm mut Self::Store) -> Self;
            }
            #implementation
    }
}

/// Transparent wrappers delegate reference encoding to the borrowed inner field.
pub(super) fn gen_transparent_ref_impls(
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
    let field_bound = quote!(#field_ty: co3::ReprC + co3::rust_spec::RustSpec + '_ršč);

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
            let output = encoder_output(quote!(&'_ršč #mutability Self));
            implementations.push(quote! {
                unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
                where
                    Self: co3::ReprC
                        + co3::rust_spec::RustSpec<Size = <#field_ty as co3::rust_spec::RustSpec>::Size, Mutability = <#field_ty as co3::rust_spec::RustSpec>::Mutability> + '_ršč,
                    #field_bound,
                    &'_ršč #mutability #field_ty: #move_trait + co3::ReprC<CType: Copy>,
                    &'_ršč #mutability Self: co3::ReprC<CType: Copy>,
                    #predicates
                {
                    type #store_name = <&'_ršč #mutability #field_ty as #move_trait>::#store_name;
                    #unsafe_fn fn #method<'itm>(&'_ršč #mutability self, store: &'itm mut Self::#store_name) -> #output
                    where
                        '_ršč: 'itm,
                    {
                        let carrier = #call;
                        // Rust repr(transparent) preserves field layout and metadata;
                        // the named C companion is transparent over the field carrier.
                        assert_eq!(core::mem::size_of_val(&carrier), core::mem::size_of::<<&'_ršč #mutability Self as co3::ReprC>::CType>());
                        unsafe { core::mem::transmute_copy(&carrier) }
                    }
                }
            });
        }
    }
    group_aggregate_ref_impls(quote!(#(#implementations)*), false)
}
