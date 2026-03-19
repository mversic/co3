use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    FnArg, GenericArgument, GenericParam, PatType, Result, Type, TypePath,
    parse::{ParseStream, Parser},
    parse_quote, parse_quote_spanned,
    spanned::Spanned,
    visit_mut::VisitMut,
};

use crate::{ExportBlock, ExternBlock, ForeignItem, parse_handle_id_attr};

const FN_BODIES_NOT_ALLOWED_MSG: &str = "fn bodies are not allowed in declarations";

struct ConstGenericArgNormalizer {
    const_params: HashSet<syn::Ident>,
}

impl VisitMut for ConstGenericArgNormalizer {
    fn visit_generic_argument_mut(&mut self, node: &mut GenericArgument) {
        syn::visit_mut::visit_generic_argument_mut(self, node);

        let GenericArgument::Type(Type::Path(TypePath { qself: None, path })) = node else {
            return;
        };
        let Some(ident) = path.get_ident() else {
            return;
        };
        if !self.const_params.contains(ident) {
            return;
        }

        *node = GenericArgument::Const(parse_quote!({#ident}));
    }
}

fn is_fn_head(input: syn::parse::ParseStream) -> syn::Result<bool> {
    let ahead = input.fork();

    let _ = ahead.parse::<Option<syn::Token![const]>>()?;
    let _ = ahead.parse::<Option<syn::Token![async]>>()?;
    let _ = ahead.parse::<Option<syn::Token![unsafe]>>()?;
    let _ = ahead.parse::<Option<syn::Abi>>()?;

    Ok(ahead.peek(syn::Token![fn]))
}

fn preprocess_impl_header(header: TokenStream) -> syn::Result<TokenStream> {
    let mut out = TokenStream::new();
    let tokens = header.into_iter().collect::<Vec<_>>();
    let mut angle_depth = 0usize;
    let mut at_param_start = false;

    let mut idx = 0usize;
    while idx < tokens.len() {
        let tt = tokens[idx].clone();
        match &tt {
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<' => {
                angle_depth += 1;
                at_param_start = angle_depth == 1;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>' => {
                angle_depth = angle_depth.saturating_sub(1);
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ',' && angle_depth == 1 => {
                at_param_start = true;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Ident(ident)
                if angle_depth == 1 && at_param_start && ident == "dyn" =>
            {
                let Some(proc_macro2::TokenTree::Group(group)) = tokens.get(idx + 1) else {
                    let err_msg = "`#[dispatch]` impl type parameters must use `dyn(id_repr) T`";
                    return Err(syn::Error::new(ident.span(), err_msg));
                };
                if group.delimiter() != proc_macro2::Delimiter::Parenthesis {
                    let err_msg = "`#[dispatch]` impl type parameters must use `dyn(id_kind) T`";
                    return Err(syn::Error::new(group.span(), err_msg));
                }

                let repr = syn::parse2::<Type>(group.stream()).map_err(|_| {
                    syn::Error::new(group.span(), "expected id repr in `dyn(id_repr) T`")
                })?;

                out.extend(quote!(#[erased(#repr)]));
                at_param_start = true;
                idx += 2;
            }
            proc_macro2::TokenTree::Punct(punct) if angle_depth == 1 && punct.as_char() == '#' => {
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Ident(_) if angle_depth == 1 && at_param_start => {
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            _ => {
                out.extend(std::iter::once(tt));
                idx += 1;
            }
        }
    }

    Ok(out)
}

struct PreprocessedArg {
    tokens: TokenStream,
}

impl PreprocessedArg {
    fn parse_with(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let mut merged_attrs = attrs;
        if input.peek(syn::Token![move]) {
            input.parse::<syn::Token![move]>()?;
            merged_attrs.push(parse_quote!(#[by_val]));
        }

        let receiver = if input.peek(syn::Token![&]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![&]>()?;
            let _ = ahead.parse::<Option<syn::Lifetime>>()?;
            let _ = ahead.parse::<Option<syn::Token![mut]>>()?;
            ahead.peek(syn::Token![self])
        } else if input.peek(syn::Token![self]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![self]>()?;
            !ahead.peek(syn::Token![:])
        } else {
            false
        };

        if receiver {
            let mut receiver = input.parse::<syn::Receiver>()?;
            merged_attrs.append(&mut receiver.attrs);
            let ty = if let Some((and_token, lifetime)) = receiver.reference {
                let mutability = receiver.mutability;
                Type::Reference(syn::TypeReference {
                    and_token,
                    lifetime,
                    mutability,
                    elem: Box::new(parse_quote!(Self)),
                })
            } else {
                parse_quote!(Self)
            };

            return Ok(Self {
                tokens: quote!(#(#merged_attrs)* __co3_self: #ty),
            });
        }

        if input.peek(syn::Token![self]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![self]>()?;
            if ahead.peek(syn::Token![:]) {
                input.parse::<syn::Token![self]>()?;
                input.parse::<syn::Token![:]>()?;
                let ty = input.parse::<Type>()?;

                return Ok(Self {
                    tokens: quote! { #(#merged_attrs)* __co3_self: #ty },
                });
            }
        }

        let mut arg = input.parse::<PatType>()?;
        merged_attrs.append(&mut arg.attrs);
        arg.attrs = merged_attrs;
        Ok(Self {
            tokens: quote!(#arg),
        })
    }
}

fn preprocess_signature_inputs(inputs: TokenStream) -> syn::Result<TokenStream> {
    let parser = move |input: syn::parse::ParseStream| -> syn::Result<TokenStream> {
        let mut args = Vec::new();
        while !input.is_empty() {
            args.push(PreprocessedArg::parse_with(input)?.tokens);
            if input.is_empty() {
                break;
            }
            input.parse::<syn::Token![,]>()?;
        }
        Ok(quote!(#(#args),*))
    };
    parser.parse2(inputs)
}

fn preprocess_signature_tokens(signature_tokens: TokenStream) -> syn::Result<TokenStream> {
    let mut rewritten = Vec::new();
    let mut saw_inputs = false;
    for tt in signature_tokens {
        if let proc_macro2::TokenTree::Group(group) = &tt
            && group.delimiter() == proc_macro2::Delimiter::Parenthesis
            && !saw_inputs
        {
            let rewritten_args = preprocess_signature_inputs(group.stream())?;
            let mut new_group =
                proc_macro2::Group::new(proc_macro2::Delimiter::Parenthesis, rewritten_args);
            new_group.set_span(group.span());
            rewritten.push(proc_macro2::TokenTree::Group(new_group));
            saw_inputs = true;
            continue;
        }
        rewritten.push(tt);
    }

    Ok(rewritten.into_iter().collect())
}

fn parse_signature(input: syn::parse::ParseStream) -> syn::Result<syn::Signature> {
    let mut signature_tokens = TokenStream::new();
    while !input.peek(syn::Token![;]) && !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        signature_tokens.extend(std::iter::once(tt));
    }

    let rewritten = preprocess_signature_tokens(signature_tokens)?;
    let mut sig = syn::parse2::<syn::Signature>(rewritten)?;
    normalize_const_args_in_fn(&mut sig);

    Ok(sig)
}

fn parse_fn_item(input: syn::parse::ParseStream) -> syn::Result<syn::ItemFn> {
    let attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    let sig = parse_signature(input)?;
    if input.peek(syn::token::Brace) {
        return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
    }
    input.parse::<syn::Token![;]>()?;
    syn::parse2(quote! {
        #(#attrs)* #vis #sig {}
    })
}

fn normalize_const_generic_args_in_impl(impl_: &mut syn::ItemImpl) {
    let const_params = impl_
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Lifetime(_) | GenericParam::Type(_) => None,
        })
        .collect();

    ConstGenericArgNormalizer { const_params }.visit_item_impl_mut(impl_);
}

fn normalize_const_args_in_fn(sig: &mut syn::Signature) {
    let const_params = sig
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Lifetime(_) | GenericParam::Type(_) => None,
        })
        .collect();

    ConstGenericArgNormalizer { const_params }.visit_signature_mut(sig);
}

fn parse_impl_item(input: syn::parse::ParseStream) -> syn::Result<syn::ItemImpl> {
    fn preprocess_impl_items(input: syn::parse::ParseStream) -> syn::Result<TokenStream> {
        let mut out = TokenStream::new();
        while !input.is_empty() {
            while input.peek(syn::Token![;]) {
                let _: syn::Token![;] = input.parse()?;
            }
            if input.is_empty() {
                break;
            }

            let attrs = input.call(syn::Attribute::parse_outer)?;
            let vis = input.parse::<syn::Visibility>()?;
            let ahead = input.fork();
            if ahead.peek(syn::Token![type]) {
                let item = input.parse::<syn::ImplItemType>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }
            if ahead.peek(syn::Token![const]) && !is_fn_head(&ahead)? {
                let item = input.parse::<syn::ImplItemConst>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }

            let sig = parse_signature(input)?;
            let body = if input.peek(syn::token::Brace) {
                return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
            } else {
                input.parse::<syn::Token![;]>()?;
                quote!({})
            };
            out.extend(quote!(#(#attrs)* #vis #sig #body));
        }
        Ok(out)
    }

    let attrs = input.call(syn::Attribute::parse_outer)?;
    let defaultness = input.parse::<Option<syn::Token![default]>>()?;
    let unsafety = input.parse::<Option<syn::Token![unsafe]>>()?;
    input.parse::<syn::Token![impl]>()?;

    let mut header = TokenStream::new();
    while !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        header.extend(std::iter::once(tt));
    }

    let content;
    syn::braced!(content in input);
    let items = preprocess_impl_items(&content)?;

    let header = preprocess_impl_header(header)?;
    let impl_tokens = quote! {
        #(#attrs)* #defaultness #unsafety impl #header {
            #items
        }
    };
    let mut impl_: syn::ItemImpl = syn::parse2(impl_tokens)?;
    normalize_const_generic_args_in_impl(&mut impl_);
    for item in &mut impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };

        restore_synthetic_receiver(&mut method.sig);
        normalize_const_args_in_fn(&mut method.sig);
    }
    Ok(impl_)
}

fn parse_items_with_kind(input: syn::parse::ParseStream) -> syn::Result<Vec<ForeignItem>> {
    let mut items = Vec::new();

    while !input.is_empty() {
        let ahead = input.fork();
        let _ = ahead.call(syn::Attribute::parse_outer)?;
        let item = if ahead.peek(syn::Token![impl]) {
            ForeignItem::Impl(parse_impl_item(input)?)
        } else {
            let _ = ahead.parse::<syn::Visibility>()?;
            if ahead.peek(syn::Token![struct]) {
                let err_msg = "item not supported";
                return Err(ahead.error(err_msg));
            }
            if ahead.peek(syn::Token![type]) {
                let mut ty = input.parse::<syn::ForeignItemType>()?;
                let id = parse_handle_id_attr(&mut ty.attrs)?.map(Box::new);

                ForeignItem::Type(crate::ForeignItemType { id, ty, drop: None })
            } else if is_fn_head(&ahead)? {
                ForeignItem::Fn(parse_fn_item(input)?)
            } else {
                return Err(input.error("item not supported"));
            }
        };

        items.push(item);
    }

    Ok(items)
}

impl ExportBlock {
    pub(crate) fn parse_items(input: ParseStream) -> Result<Vec<ForeignItem>> {
        parse_items_with_kind(input)
    }
}
impl ExternBlock {
    pub(crate) fn parse_items(input: ParseStream) -> Result<Vec<ForeignItem>> {
        parse_items_with_kind(input)
    }
}

fn restore_synthetic_receiver(signature: &mut syn::Signature) {
    let Some(position) = signature.inputs.iter().position(|input| {
        matches!(
            input,
            FnArg::Typed(PatType { pat, .. })
                if matches!(pat.as_ref(), syn::Pat::Ident(pat_ident) if pat_ident.ident == "__co3_self")
        )
    }) else {
        return;
    };

    let Some(FnArg::Typed(PatType { attrs, ty, .. })) = signature.inputs.iter_mut().nth(position)
    else {
        return;
    };

    let receiver_span = ty.span();
    let ref_receiver = |mutability: Option<syn::Token![mut]>| {
        if mutability.is_some() {
            parse_quote_spanned!(receiver_span=> &mut self)
        } else {
            parse_quote_spanned!(receiver_span=> &self)
        }
    };
    let mut receiver: syn::Receiver = match &**ty {
        Type::Path(ty_path) if ty_path.qself.is_none() && ty_path.path.is_ident("Self") => {
            parse_quote_spanned!(receiver_span=> self)
        }
        Type::Reference(ty)
            if matches!(
                ty.elem.as_ref(),
                Type::Path(ty_path) if ty_path.qself.is_none() && ty_path.path.is_ident("Self")
            ) =>
        {
            ref_receiver(ty.mutability)
        }
        ty => {
            let ty = ty.clone();
            let mut receiver: syn::Receiver = parse_quote_spanned!(receiver_span=> self: #ty);
            receiver.colon_token = None;
            receiver
        }
    };
    receiver.attrs = attrs.clone();

    let mut inputs = core::mem::take(&mut signature.inputs)
        .into_iter()
        .collect::<Vec<_>>();

    inputs[position] = FnArg::Receiver(receiver);
    signature.inputs = inputs.into_iter().collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_move_self_receiver() {
        let impl_ = "impl Value { fn name(move self); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Receiver(receiver) = &method.sig.inputs[0] else {
            panic!("expected receiver");
        };

        assert!(
            receiver
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("by_val"))
        );
    }

    #[test]
    fn parses_dyn_dispatch_impl_param() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value<T> { fn name(&mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let Some(syn::GenericParam::Type(param)) = item.generics.params.first() else {
            panic!("expected type param");
        };
        assert!(
            param
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("erased"))
        );
    }

    #[test]
    fn restores_receiver_after_id_arg() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value { fn name(self_id: Self::ID, &mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let Some(FnArg::Receiver(receiver)) = method.sig.inputs.iter().nth(1) else {
            panic!("expected receiver");
        };

        assert!(receiver.reference.is_some());
        assert!(receiver.mutability.is_some());
    }
}
