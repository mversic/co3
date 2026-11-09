//! Common field classification for generated conversion bounds.

use quote::ToTokens;
use syn::{Generics, Type, WherePredicate, parse_quote};

use super::{is_parametrized, is_phantom_data};
use crate::utils::speculative_marker;

pub(super) struct FieldType<'a> {
    pub(super) ty: &'a Type,
    pub(super) phantom: bool,
    pub(super) concrete: bool,
}

impl<'a> FieldType<'a> {
    pub(super) fn new(ty: &'a Type, generics: &Generics) -> Self {
        Self {
            ty,
            phantom: is_phantom_data(ty),
            concrete: !is_parametrized(ty, generics),
        }
    }

    pub(super) fn bound(&self, bounds: impl ToTokens) -> WherePredicate {
        let ty = self.ty;
        let speculative = speculative_marker(self.concrete);
        parse_quote!(#speculative #ty: #bounds)
    }
}

pub(super) fn field_types<'a>(
    fields: &'a [&'a Type],
    generics: &'a Generics,
) -> impl Iterator<Item = FieldType<'a>> {
    fields.iter().map(|ty| FieldType::new(ty, generics))
}
