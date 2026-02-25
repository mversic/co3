//! This module provides parsing of standard rust `#[repr(...)]` attributes.

use std::str::FromStr;

use darling::{FromAttributes, error::Accumulator, util::SpannedValue};
use proc_macro2::{Delimiter, Span};
use strum::{Display, EnumString};
use syn::{
    Attribute, Meta, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned as _,
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum ReprPrimitive {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    I64,
    U64,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ReprKind {
    C(Option<ReprPrimitive>),
    Primitive(ReprPrimitive),
    Transparent,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ReprAlignment {
    Packed,
    Aligned(u32),
}

#[derive(Debug)]
enum ReprToken {
    Kind(ReprKind),
    Alignment(ReprAlignment),
}

#[derive(Debug)]
struct SpannedReprToken {
    span: Span,
    token: ReprToken,
}

impl quote::ToTokens for ReprPrimitive {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ty = match self {
            Self::U8 => quote::quote! {u8},
            Self::I8 => quote::quote! {i8},
            Self::U16 => quote::quote! {u16},
            Self::I16 => quote::quote! {i16},
            Self::U32 => quote::quote! {u32},
            Self::I32 => quote::quote! {i32},
            Self::U64 => quote::quote! {u64},
            Self::I64 => quote::quote! {i64},
        };

        ty.to_tokens(tokens);
    }
}

impl Parse for SpannedReprToken {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let (span, token) = input.step(|cursor| {
            let Some((ident, after_token)) = cursor.ident() else {
                return Err(cursor.error("Expected repr kind"));
            };

            let mut span = ident.span();
            let str = ident.to_string();

            if let Ok(primitive) = ReprPrimitive::from_str(&str) {
                return Ok((
                    (span, ReprToken::Kind(ReprKind::Primitive(primitive))),
                    after_token,
                ));
            }

            match str.as_str() {
                "transparent" => Ok(((span, ReprToken::Kind(ReprKind::Transparent)), after_token)),
                "C" => Ok(((span, ReprToken::Kind(ReprKind::C(None))), after_token)),
                "packed" => Ok((
                    (span, ReprToken::Alignment(ReprAlignment::Packed)),
                    after_token,
                )),
                "aligned" => {
                    let Some((inside_of_group, group_span, after_group)) =
                        after_token.group(Delimiter::Parenthesis)
                    else {
                        return Ok((
                            (span, ReprToken::Alignment(ReprAlignment::Aligned(1))),
                            after_token,
                        ));
                    };

                    span = span.join(group_span.span()).unwrap_or(span);
                    let alignment = syn::parse2::<syn::LitInt>(inside_of_group.token_stream())
                        .and_then(|lit| lit.base10_parse::<u32>())
                        .unwrap_or(1);

                    Ok((
                        (
                            span,
                            ReprToken::Alignment(ReprAlignment::Aligned(alignment)),
                        ),
                        after_group,
                    ))
                }
                _ => Err(cursor.error("Unrecognized repr kind")),
            }
        })?;

        Ok(SpannedReprToken { span, token })
    }
}

#[derive(Debug)]
struct ReprTokens(Vec<SpannedReprToken>);

impl Parse for ReprTokens {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self(
            Punctuated::<_, Token![,]>::parse_terminated(input)?
                .into_iter()
                .collect(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct Repr {
    /// Repr kind
    ///
    /// The value of None means no repr was specified.
    /// It corresponds to what is called `repr()` in the Rust reference.
    pub kind: Option<SpannedValue<ReprKind>>,
}

impl FromAttributes for Repr {
    fn from_attributes(attrs: &[Attribute]) -> darling::Result<Self> {
        let mut alignment: Option<(ReprAlignment, Span)> = None;
        let mut kind: Option<(ReprKind, Span)> = None;
        let mut accumulator = Accumulator::default();

        let repr_attrs: Vec<_> = attrs
            .iter()
            .filter(|attr| attr.path().is_ident("repr"))
            .collect();

        if repr_attrs.len() > 1 {
            for attr in &repr_attrs[1..] {
                accumulator
                    .push(darling::Error::custom("Multiple repr attributes").with_span(attr));
            }

            return accumulator.finish_with(Self::default());
        }

        let Some(&attr) = repr_attrs.first() else {
            return accumulator.finish_with(Self::default());
        };

        let Meta::List(list) = &attr.meta else {
            return accumulator.finish_with(Self::default());
        };

        let Some(tokens) =
            accumulator.handle(syn::parse2::<ReprTokens>(list.tokens.clone()).map_err(Into::into))
        else {
            return accumulator.finish_with(Self::default());
        };

        for SpannedReprToken { token, span } in tokens.0 {
            match token {
                ReprToken::Kind(new_kind) => match (&mut kind, new_kind) {
                    (Some((ReprKind::C(None), _)), ReprKind::Primitive(prim)) => {
                        kind = Some((ReprKind::C(Some(prim)), span));
                    }
                    (Some((ReprKind::Primitive(prim), _)), ReprKind::C(None)) => {
                        kind = Some((ReprKind::C(Some(*prim)), span));
                    }
                    (Some(_), _) => {
                        accumulator.push(
                            darling::error::Error::custom("Duplicate repr kind within attribute")
                                .with_span(&span),
                        );
                    }
                    (None, new_kind) => kind = Some((new_kind, span)),
                },
                ReprToken::Alignment(new_alignment) => {
                    if alignment.is_some() {
                        accumulator.push(
                            darling::error::Error::custom(
                                "Duplicate repr alignment within attribute",
                            )
                            .with_span(&span),
                        );
                    }
                    alignment = Some((new_alignment, span));
                }
            }
        }

        accumulator.finish_with(Self {
            kind: kind.map(|(k, s)| SpannedValue::new(k, s)),
        })
    }
}

#[cfg(test)]
mod test {
    use darling::FromAttributes as _;
    use proc_macro2::TokenStream;
    use quote::quote;

    use super::{Repr, ReprAlignment, ReprKind, ReprPrimitive};

    fn parse_repr(attrs: TokenStream) -> darling::Result<Repr> {
        let attrs = crate::parse_attributes(attrs);
        Repr::from_attributes(&attrs)
    }

    macro_rules! assert_repr_ok {
        ($( #[$meta:meta] )*,
            Repr {
                kind: $kind:expr,
                alignment: $alignment:expr,
            }
        ) => {
            {
                let repr = parse_repr(quote!(
                    $( #[$meta] )*
                )).unwrap();
                assert_eq!(repr.kind.map(|v| *v.as_ref()), $kind, "The parsed repr kind does not match the expected one");
                let _alignment: Option<ReprAlignment> = $alignment;
            }
        };
    }

    #[test]
    fn repr_empty() {
        assert_repr_ok!(
            #[aboba], // unrelated attr
            Repr {
                kind: None,
                alignment: None,
            }
        );
    }

    #[test]
    fn repr_c() {
        assert_repr_ok!(
            #[repr(C)],
            Repr {
                kind: Some(ReprKind::C(None)),
                alignment: None,
            }
        );
    }

    #[test]
    fn aligned() {
        assert_repr_ok!(
            #[repr(aligned(4))],
            Repr {
                kind: None,
                alignment: Some(ReprAlignment::Aligned(4)),
            }
        );
    }

    #[test]
    fn primitive() {
        assert_repr_ok!(
            #[repr(u8)],
            Repr {
                kind: Some(ReprKind::Primitive(ReprPrimitive::U8)),
                alignment: None,
            }
        );
    }

    #[test]
    fn kind_and_alignment() {
        assert_repr_ok!(
            #[repr(C, aligned(4))],
            Repr {
                kind: Some(ReprKind::C(None)),
                alignment: Some(ReprAlignment::Aligned(4)),
            }
        );
    }

    #[test]
    fn repr_c_with_primitive() {
        assert_repr_ok!(
            #[repr(C, u8)],
            Repr {
                kind: Some(ReprKind::C(Some(ReprPrimitive::U8))),
                alignment: None,
            }
        );
    }

    macro_rules! assert_repr_err {
        ($( #[$meta:meta] )*, $error:expr) => {
            assert_eq!(
                parse_repr(quote!(
                    $( #[$meta] )*
                ))
                .unwrap_err()
                .to_string(),
                $error,
                "The error message does not match the expected one"
            )
        };
    }

    // we don't care __that__ much about good errors here
    // rustc should already handle the #[repr] attributes and produce reasonable errors
    #[test]
    fn err_multiple_repr_attributes() {
        assert_repr_err!(
            #[repr(C)] #[repr(C)],
            "Multiple repr attributes"
        );
        assert_repr_err!(
            #[repr(C)] #[repr(u32)],
            "Multiple repr attributes"
        );
        assert_repr_err!(
            #[repr(aligned(4))] #[repr(aligned(4))],
            "Multiple repr attributes"
        );
        assert_repr_err!(
            #[repr(aligned(4))] #[repr(aligned(8))],
            "Multiple repr attributes"
        );
    }

    #[test]
    fn err_duplicate_kind_within_attribute() {
        assert_repr_err!(
            #[repr(C, transparent)],
            "Duplicate repr kind within attribute"
        );
        assert_repr_err!(
            #[repr(u8, u16)],
            "Duplicate repr kind within attribute"
        );
    }

    #[test]
    fn err_duplicate_alignment_within_attribute() {
        assert_repr_err!(
            #[repr(aligned(4), packed)],
            "Duplicate repr alignment within attribute"
        );
        assert_repr_err!(
            #[repr(aligned(4), aligned(8))],
            "Duplicate repr alignment within attribute"
        );
    }

    #[test]
    fn incomplete_alignment_defaults_to_one() {
        // When alignment can't be parsed, default to 1 (has no effect)
        assert_repr_ok!(
            #[repr(aligned)],
            Repr {
                kind: None,
                alignment: Some(ReprAlignment::Aligned(1)),
            }
        );
        assert_repr_ok!(
            #[repr(aligned())],
            Repr {
                kind: None,
                alignment: Some(ReprAlignment::Aligned(1)),
            }
        );
        assert_repr_ok!(
            #[repr(aligned(4,))],
            Repr {
                kind: None,
                alignment: Some(ReprAlignment::Aligned(1)),
            }
        );
        assert_repr_ok!(
            #[repr(aligned(4, 8))],
            Repr {
                kind: None,
                alignment: Some(ReprAlignment::Aligned(1)),
            }
        );
    }

    #[test]
    fn err_unknown_kind() {
        assert_repr_err!(
            #[repr(unknown)],
            "Unrecognized repr kind"
        );
    }
}
