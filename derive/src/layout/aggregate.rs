//! Fieldwise conversion of records and the active payload of an enum.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, Generics, Ident, Type, WherePredicate, parse_quote};

use crate::{
    codec::{Carrier, Conversion, TraitRef},
    layout::{
        ReprCAttrs, VariantReprCAttrs,
        attr::ReprKind,
        borrow::{either_variant_name, gen_either_name},
        ctype::{
            c_fn_last_field_bounds, gen_ctype_name, gen_union_extern_c_bounds,
            gen_variant_struct_name, projected_field_bounds,
        },
        enum_tag_type, enum_variant_tag_exprs,
        fields::{FieldType, field_types},
        inference::RecursiveFields,
        is_phantom_data, is_transparent_enum_repr,
        item::field_vars,
        recursive, reference,
    },
    utils::speculative_marker,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Encode,
    Decode,
    BorrowedEncode,
    BorrowedDecode,
    EncodeRef,
}

impl Operation {
    pub(super) fn encoding(self) -> bool {
        matches!(self, Self::Encode | Self::BorrowedEncode | Self::EncodeRef)
    }
    fn borrowed(self) -> bool {
        matches!(self, Self::BorrowedEncode | Self::BorrowedDecode)
    }
    pub(super) fn path(self, contract: &Conversion) -> TraitRef {
        if self == Self::EncodeRef {
            return TraitRef {
                module: TokenStream::new(),
                name: contract.trait_name("__Co3EncodeField"),
                arguments: contract.args(Some(parse_quote!('_ršč))),
            };
        }
        let carrier = if self.borrowed() {
            Carrier::Borrowed
        } else {
            Carrier::Move
        };
        let lifetime = (!self.encoding()).then(|| parse_quote!('_dšč));
        contract.bound(carrier, lifetime.as_ref(), quote!())
    }
}

struct Record<'a> {
    head: TokenStream,
    carrier: Ident,
    fields: &'a Fields,
    validator: Option<&'a syn::ExprClosure>,
}

impl Record<'_> {
    fn storage(&self, op: Operation, contract: &Conversion, owner: bool) -> Type {
        let path = op.path(contract);
        let state = if owner {
            format_ident!("Owner")
        } else {
            contract.store()
        };
        let fields = self.fields.iter().map(|field| {
            let ty = &field.ty;
            if op == Operation::EncodeRef && is_phantom_data(ty) {
                quote!(())
            } else {
                quote!(<#ty as #path>::#state)
            }
        });
        parse_quote!((#(#fields,)*))
    }

    fn body(
        &self,
        op: Operation,
        contract: &Conversion,
        tag: Option<&TokenStream>,
        variant_state: TokenStream,
        wrap: impl FnOnce(TokenStream) -> TokenStream,
    ) -> TokenStream {
        let fields = self.fields;
        let head = &self.head;
        let carrier = &self.carrier;
        let path = op.path(contract);
        let encoding = op.encoding();
        let method = contract.method(if op == Operation::EncodeRef {
            "encode_field"
        } else if encoding {
            "soft_encode"
        } else {
            "soft_decode"
        });
        let vars = field_vars(fields);
        let member_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
        // Keep generated bindings distinct from fields named store, source, or owner.
        let bindings: Vec<_> = (0..fields.len())
            .map(|i| format_ident!("__co3_field_{i}", span = proc_macro2::Span::mixed_site()))
            .collect();
        let members = match fields {
            Fields::Named(_) | Fields::Unit => quote!({ #(#member_names: #bindings),* }),
            Fields::Unnamed(_) => quote!((#(#bindings),*)),
        };
        let conversions = fields
            .iter()
            .zip(&bindings)
            .enumerate()
            .map(|(i, (f, var))| {
                let i = syn::Index::from(i);
                let ty = &f.ty;
                let expr = if op == Operation::EncodeRef && is_phantom_data(ty) {
                    quote!(core::marker::PhantomData)
                } else {
                    let owner = (op == Operation::BorrowedEncode).then(|| quote!(, &mut owner.#i));
                    let call = quote!(<#ty as #path>::#method(#var, &mut store.#i #owner));
                    if encoding && contract.checked() {
                        call
                    } else if !encoding && contract.checked() {
                        quote!(unsafe { #call }?)
                    } else {
                        quote!(unsafe { #call })
                    }
                };
                quote!(let #var = #expr;)
            });
        let validation = contract
            .checked()
            .then_some(self.validator)
            .flatten()
            .map(|validator| {
                let types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
                let args = bindings.iter().map(|b| quote!(&#b)).collect::<Vec<_>>();
                let call = super::gen_is_valid_call(validator, &types, &args);
                if encoding {
                    quote!(debug_assert!(#call, "encoding failed custom is_valid");)
                } else {
                    quote!(if !#call { return None; })
                }
            });
        let tag_field = tag.map(|tag| quote!(tag: #tag,));
        let tag_element = tag.map(|tag| quote!(#tag,));
        if encoding {
            let moves = (op != Operation::EncodeRef)
                .then(|| quote!(#(let #bindings = unsafe { core::ptr::read(#bindings) };)*));
            let values = bindings.iter().map(|b| {
                if tag.is_some() {
                    quote!(core::mem::MaybeUninit::new(#b))
                } else {
                    quote!(#b)
                }
            });
            let value = match fields {
                Fields::Unnamed(_) => quote!(#carrier(#tag_element #(#values),*)),
                _ => quote!(#carrier { #tag_field #(#vars: #values),* }),
            };
            let value = wrap(value);
            quote!(#head #members => { #moves #validation #variant_state #(#conversions)* #value })
        } else {
            let ignore_tag = tag.map(|_| quote!(_,));
            let ignore_named = tag.map(|_| quote!(tag: _,));
            let pattern = match fields {
                Fields::Unnamed(_) => quote!(#carrier(#ignore_tag #(#bindings),*)),
                _ => quote!(#carrier { #ignore_named #(#vars: #bindings),* }),
            };
            let unwrap =
                tag.map(|_| quote!(#(let #bindings = unsafe { #bindings.assume_init() };)*));
            let result = contract.success(quote!(#head #members));
            quote!(let #pattern = source; #unwrap #variant_state #(#conversions)* #validation #result)
        }
    }
}

pub(super) struct Aggregate<'a> {
    input: &'a DeriveInput,
    records: Vec<Record<'a>>,
    tags: Option<(Type, Vec<TokenStream>)>,
    inline: bool,
    carrier: Ident,
    generics: Generics,
    fields: Vec<&'a Type>,
    repr_bounds: Vec<WherePredicate>,
    recursion: &'a RecursiveFields<'a>,
}

impl<'a> Aggregate<'a> {
    pub(super) fn new(
        input: &'a DeriveInput,
        repr: Option<&ReprKind>,
        attrs: &'a ReprCAttrs,
        variants: &'a [VariantReprCAttrs],
        recursion: &'a RecursiveFields<'a>,
    ) -> Self {
        let carrier = gen_ctype_name(&input.ident);
        let mut generics = input.generics.clone();
        let mut tags = None;
        let mut repr_bounds = Vec::new();
        let records = match &input.data {
            syn::Data::Struct(s) => vec![Record {
                head: quote!(Self),
                carrier: carrier.clone(),
                fields: &s.fields,
                validator: attrs.is_valid.as_ref(),
            }],
            syn::Data::Enum(e) => {
                let transparent = is_transparent_enum_repr(repr, &e.variants);
                if transparent {
                    if let Some(f) = e
                        .variants
                        .first()
                        .and_then(|v| v.fields.iter().next_back())
                        .filter(|f| !is_phantom_data(&f.ty))
                    {
                        let bound = FieldType::new(&f.ty, &generics)
                            .bound(quote!(co3::ReprC<CType: Sized>));
                        generics.make_where_clause().predicates.push(bound);
                    }
                } else {
                    let ty = enum_tag_type(repr, e.variants.len());
                    let values = enum_variant_tag_exprs(&e.variants, &ty);
                    tags = Some((ty, values));
                    repr_bounds = gen_union_extern_c_bounds(&input.ident, &generics, &e.variants);
                }
                e.variants
                    .iter()
                    .zip(variants)
                    .map(|(v, a)| {
                        let name = &v.ident;
                        Record {
                            head: quote!(Self::#name),
                            carrier: if transparent {
                                carrier.clone()
                            } else {
                                gen_variant_struct_name(&input.ident, name)
                            },
                            fields: &v.fields,
                            validator: a.is_valid.as_ref(),
                        }
                    })
                    .collect()
            }
            _ => unreachable!(),
        };
        let fields = records
            .iter()
            .flat_map(|r| r.fields.iter().map(|f| &f.ty))
            .collect();
        Self {
            input,
            records,
            tags,
            inline: !matches!(repr, Some(ReprKind::C(Some(_)))),
            carrier,
            generics,
            fields,
            repr_bounds,
            recursion,
        }
    }

    fn storage(&self, op: Operation, contract: &Conversion, owner: bool) -> Type {
        let mut states = self.records.iter().map(|r| r.storage(op, contract, owner));
        if self.tags.is_none() {
            return states.next().unwrap();
        }
        let either = gen_either_name(self.records.len());
        parse_quote!(Option<co3::either::#either<#(#states),*>>)
    }

    fn body(&self, op: Operation, contract: &Conversion) -> TokenStream {
        let encoding = op.encoding();
        let mut arms = Vec::new();
        for (i, record) in self.records.iter().enumerate() {
            if let Some((tag_ty, tags)) = &self.tags {
                let tag = &tags[i];
                let inline_tag = self.inline.then(
                    || quote!(unsafe { co3::soft_encode_unchecked(#tag as #tag_ty, &mut ()) }),
                );
                let either = gen_either_name(self.records.len());
                let variant = either_variant_name(i);
                let init = |slot: Ident| {
                    quote! {
                        let co3::either::#either::#variant(#slot) = #slot.insert(co3::either::#either::#variant(Default::default())) else { unreachable!() };
                    }
                };
                let store = init(format_ident!("store"));
                let owner = (op == Operation::BorrowedEncode).then(|| init(format_ident!("owner")));
                let syn::Data::Enum(e) = &self.input.data else {
                    unreachable!()
                };
                let variant = &e.variants[i].ident;
                let carrier = &self.carrier;
                if encoding {
                    let wrap = |payload| {
                        if self.inline {
                            quote!(#carrier { #variant: #payload })
                        } else {
                            let union = format_ident!("{}Payload", self.input.ident);
                            quote!(#carrier { tag: #tag as #tag_ty, payload: core::mem::MaybeUninit::new(#union { #variant: #payload }) })
                        }
                    };
                    arms.push(record.body(
                        op,
                        contract,
                        inline_tag.as_ref(),
                        quote!(#store #owner),
                        wrap,
                    ));
                } else {
                    let body = record.body(op, contract, inline_tag.as_ref(), store, |v| v);
                    let payload = if self.inline {
                        quote!(source.#variant)
                    } else {
                        quote!(source.payload.assume_init().#variant)
                    };
                    arms.push(quote!(value if value == #tag => { let source = unsafe { #payload }; #body }));
                }
            } else {
                let body = record.body(op, contract, None, quote!(), |v| v);
                if !encoding {
                    return body;
                }
                arms.push(body);
            }
        }
        if encoding {
            let source = if op == Operation::EncodeRef {
                quote!(self)
            } else {
                quote!(&*core::mem::ManuallyDrop::new(self))
            };
            quote!(match #source { #(#arms,)* })
        } else {
            let (ty, _) = self.tags.as_ref().unwrap();
            let tag = if self.inline {
                quote!(unsafe { *core::ptr::from_ref(&source).cast::<#ty>() })
            } else {
                quote!(source.tag)
            };
            let invalid = if contract.checked() {
                quote!(None)
            } else {
                quote!(unsafe { core::hint::unreachable_unchecked() })
            };
            quote!(match #tag { #(#arms,)* _ => #invalid })
        }
    }

    pub(super) fn repr(&self) -> TokenStream {
        let name = &self.input.ident;
        let (params, args, clause) = self.generics.split_for_impl();
        let predicates = clause.map(|c| &c.predicates);
        let carrier = &self.carrier;
        let bounds = &self.repr_bounds;
        let external =
            projected_field_bounds(&self.generics, &self.fields, &self.recursion.carriers);
        let has_validator = self.records.iter().any(|record| record.validator.is_some());
        let validity = has_validator.then(|| {
            quote!(#name #args: co3::rust_spec::RustSpec<Trap = co3::rust_spec::layout::NonRobust>,)
        });
        quote!(impl #params co3::ReprC for #name #args where #validity #(#bounds,)* #(#external,)* #predicates { type CType = #carrier #args; })
    }

    pub(super) fn emit(&self, op: Operation, contract: &Conversion, is_move: bool) -> TokenStream {
        let (sized, omit) = c_fn_last_field_bounds(&self.fields, &self.generics);
        if omit {
            return quote!();
        }
        let name = &self.input.ident;
        let (_, args, _) = self.generics.split_for_impl();
        let mut generics = contract.generics(&self.generics);
        if !op.encoding() {
            generics.params.insert(0, parse_quote!('_dšč));
        }
        let bounds = &self.repr_bounds;
        let equalities = &self.recursion.equalities;
        let spec = speculative_marker(self.generics.params.is_empty());
        let drop_bound = quote!(#spec Self: co3::rust_spec::RustSpec, #spec <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop,);
        let codec = op.path(contract);
        let stores = &self.recursion.stores;
        let field_bounds = |types: &[&Type], path: &TraitRef| {
            field_types(types, &self.generics)
                .filter(|field| !field.phantom)
                .map(|field| field.bound(quote!(#path)))
                .collect::<Vec<_>>()
        };
        let marker = (!op.borrowed()).then(|| {
            let lifetime = (!op.encoding()).then(|| parse_quote!('_dšč));
            let path = contract.bound(Carrier::Value, lifetime.as_ref(), quote!());
            let fields = field_bounds(&stores.marker_fields, &path);
            let drop = (!is_move).then_some(&drop_bound);
            let dispatch = (stores.marker_fields.len() != stores.codec_fields.len())
                .then(|| quote!(Self: #codec,));
            let (params, _, clause) = generics.split_for_impl();
            let predicates = clause.map(|c| &c.predicates);
            quote! {
                impl #params #path for #name #args
                where #drop #sized #(#bounds,)* #(#equalities,)* #dispatch #(#fields,)* #predicates {}
            }
        });
        let predicates = &mut generics.make_where_clause().predicates;
        // Assemble generated predicates separately from the nominal type's own bounds.
        predicates.extend(sized);
        predicates.extend(bounds.iter().chain(equalities).cloned());
        predicates.extend(field_bounds(&stores.codec_fields, &codec));
        if op.borrowed() {
            predicates.push(parse_quote!(Self: co3::rust_spec::RustSpec));
            predicates.push(
                parse_quote!(#spec <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop),
            );
        }
        let store = self.storage(op, contract, false);
        let owner = (op == Operation::BorrowedEncode && !contract.checked())
            .then(|| self.storage(op, contract, true));
        let body = self.body(op, contract);
        let implementation = recursive::value_impl(
            self.input,
            stores,
            op,
            contract,
            recursive::ValueImpl {
                generics,
                store,
                owner,
                body,
            },
        );
        quote!(#implementation #marker)
    }

    pub(super) fn reference(&self, contract: &Conversion) -> TokenStream {
        if !cfg!(feature = "repr-rust") {
            return quote!();
        }

        let (sized_bound, omit) = c_fn_last_field_bounds(&self.fields, &self.generics);
        if omit {
            return quote!();
        }
        let fields = &self.fields;
        let generics = &self.generics;
        let name = &self.input.ident;
        let (_, ty_generics, clause) = generics.split_for_impl();
        let leading_predicates = clause.map(|w| {
            let p = w.predicates.iter();
            quote!(#(#p,)*)
        });
        let speculative = speculative_marker(generics.params.is_empty());
        let repr_c_bounds = &self.repr_bounds;
        let carrier_equalities = &self.recursion.equalities;
        let carrier = &self.carrier;
        let reference_repr = quote!(co3::ReprC<CType = #carrier #ty_generics>);
        let mut g = contract.generics(generics);
        g.params.insert(0, parse_quote!('_ršč));
        let (ref_impl_generics, _, _) = g.split_for_impl();
        let enc = reference::RefEncoder::new(false, contract);
        let reference::RefEncoder {
            name: trait_name,
            method,
            store: store_name,
            args,
            unsafe_fn,
            ..
        } = &enc;
        let output = reference::encoder_output(quote!(&'_ršč Self));
        let abi_bound = contract.compatibility();
        let validation_bound = contract.checked().then(|| quote!(&'_ršč Self: #abi_bound,));
        let validation = contract.checked().then(||quote!(debug_assert!(unsafe { <&'_ršč Self as #abi_bound>::is_valid(&encoded) }, "encoding failed AbiValidate::is_valid");));
        let field_trait = Operation::EncodeRef.path(contract);
        let field_store = self.storage(Operation::EncodeRef, contract, false);
        let encode_body = self.body(Operation::EncodeRef, contract);
        let field_bounds = field_types(fields, generics)
            .filter(|field| !field.phantom)
            .map(|field| field.bound(quote!(#field_trait)));
        let implementations = quote! {
            unsafe impl #ref_impl_generics #trait_name #args for #name #ty_generics
            where
                #sized_bound
                #(#repr_c_bounds,)*
                #(#carrier_equalities,)*
                #(#field_bounds,)*
                #leading_predicates
                #speculative Self: #reference_repr + '_ršč,
                &'_ršč Self: co3::ReprC<CType = co3::reference::abi::SoftRef<<Self as co3::ReprC>::CType>>,

            {
                type #store_name = co3::sync::RefSizedEncodeStore<Self, #field_store>;
                #unsafe_fn fn #method<'itm>(&'_ršč self, store: &'itm mut Self::#store_name) -> #output where '_ršč: 'itm {
                    store.encode(|store| { #encode_body })
                }
            }
            unsafe impl #ref_impl_generics #trait_name #args for #name #ty_generics
            where
                #sized_bound
                #leading_predicates
                #(#repr_c_bounds,)*
                #speculative <Self as co3::ReprC>::CType: Sized,
                #speculative Self: #abi_bound
                    + Sized + '_ršč,
                &'_ršč Self: co3::ReprC<CType = co3::reference::abi::Ref<'_ršč, <Self as co3::ReprC>::CType>>,
                #validation_bound
            {
                type #store_name = ();
                #unsafe_fn fn #method<'itm>(&'_ršč self, _: &'itm mut ()) -> #output where '_ršč: 'itm {
                    let encoded = unsafe { co3::reference::abi::Ref::from_raw(core::ptr::from_ref(self).cast()) };
                    #validation
                    encoded
                }
            }
        };
        enc.group(implementations, true)
    }
}
