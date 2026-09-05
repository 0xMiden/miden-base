use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Field, Fields, GenericArgument, PathArguments, Result, Type};

use super::{FieldKindOverride, ProstField, ident_name};

pub(super) fn expand(input: DeriveInput, runtime: TokenStream) -> Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(input.generics.span(), "generic messages are not supported"));
    }
    if let Some(attribute) = input.attrs.iter().find(|attr| attr.path().is_ident("proto_decode")) {
        return Err(syn::Error::new(
            attribute.span(),
            "ProtoDecodeFields has no message-level configuration; implement Verify on the record",
        ));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(input.span(), "ProtoDecodeFields requires a named struct"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(data.fields.span(), "ProtoDecodeFields requires named fields"));
    };

    let source = &input.ident;
    let record = format_ident!("Decoded{}", ident_name(source));
    let visibility = &input.vis;
    let mut declarations = Vec::new();
    let mut initializers = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let name = ident_name(ident);
        let prost = ProstField::parse(field)?;
        if prost.oneof.is_some() || prost.enumeration.is_some() || prost.map || prost.boxed {
            return Err(syn::Error::new(
                field.span(),
                "ProtoDecodeFields does not yet support enums, oneofs, maps, or boxed messages",
            ));
        }
        let presence = FieldKindOverride::parse(&field.attrs)?;
        if presence.is_some() && (!prost.message || !prost.optional || prost.repeated) {
            return Err(syn::Error::new(
                field.span(),
                "presence overrides require a singular Option<Message> field",
            ));
        }

        let (ty, value) = if !prost.message {
            let ty = &field.ty;
            (quote!(#ty), quote!(message.#ident))
        } else if prost.repeated {
            let inner = container_type(field, "Vec")?;
            (
                quote!(#runtime::Vec<<#inner as #runtime::DecodeMessage>::Decoded>),
                quote!(#runtime::decode(#runtime::RepeatedField::new(#name, message.#ident))?),
            )
        } else if prost.optional {
            let inner = container_type(field, "Option")?;
            let decoded = quote!(<#inner as #runtime::DecodeMessage>::Decoded);
            if matches!(presence, Some(FieldKindOverride::Optional)) {
                (
                    quote!(::core::option::Option<#decoded>),
                    quote!(#runtime::decode(#runtime::OptionalField::new(#name, message.#ident))?),
                )
            } else {
                (
                    decoded,
                    quote!(#runtime::decode(
                        #runtime::RequiredField::<#source, _>::new(#name, message.#ident)
                    )?),
                )
            }
        } else {
            let inner = &field.ty;
            (
                quote!(<#inner as #runtime::DecodeMessage>::Decoded),
                quote!(#runtime::decode(#runtime::ValueField::new(#name, message.#ident))?),
            )
        };
        let docs = field.attrs.iter().filter(|attribute| attribute.path().is_ident("doc"));
        let visibility = &field.vis;
        declarations.push(quote!(#(#docs)* #visibility #ident: #ty));
        initializers.push(quote!(#ident: #value));
    }

    let doc = format!("Decoded fields of [`{source}`]. Domain invariants have not been verified.");
    Ok(quote! {
        #[doc = #doc]
        #[derive(Debug)]
        #[must_use = "decoded fields have not been verified"]
        #visibility struct #record {
            #(#declarations,)*
        }

        impl #runtime::DecodeMessage for #source {
            type Decoded = #record;
        }

        impl ::core::convert::TryFrom<#source> for #record {
            type Error = #runtime::ConversionError;

            fn try_from(message: #source) -> ::core::result::Result<Self, Self::Error> {
                ::core::result::Result::Ok(Self { #(#initializers,)* })
            }
        }
    })
}

fn container_type<'a>(field: &'a Field, container: &str) -> Result<&'a Type> {
    if let Type::Path(ty) = &field.ty
        && ty.qself.is_none()
        && let Some(segment) = ty.path.segments.last()
        && segment.ident == container
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && args.args.len() == 1
        && let Some(GenericArgument::Type(inner)) = args.args.first()
    {
        return Ok(inner);
    }
    Err(syn::Error::new(
        field.ty.span(),
        format!("expected a Prost {container}<T> field"),
    ))
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn unsupported_fields_fail_explicitly() {
        for field in [
            quote!(#[prost(oneof = "Choice", tags = "1, 2")] value: Option<Choice>),
            quote!(#[prost(enumeration = "Kind", tag = "1")] value: i32),
            quote!(#[prost(enumeration = "Kind", repeated, tag = "1")] value: Vec<i32>),
            quote!(#[prost(map = "string, uint32", tag = "1")] value: HashMap<String, u32>),
            quote!(#[prost(btree_map = "string, uint32", tag = "1")] value: BTreeMap<String, u32>),
            quote!(#[prost(message, optional, boxed, tag = "1")] value: Option<Box<Nested>>),
        ] {
            let input = syn::parse2(quote!(struct Message { #field })).unwrap();
            let error = expand(input, quote!(::runtime)).unwrap_err();
            assert!(error.to_string().contains("does not yet support"), "{error}");
        }
    }

    #[test]
    fn rejects_constructor_configuration() {
        let error = expand(
            parse_quote! {
                #[proto_decode(target(Domain), constructor(Domain::new(value)))]
                struct Message { #[prost(uint32, tag = "1")] value: u32 }
            },
            quote!(::runtime),
        )
        .unwrap_err();
        assert!(error.to_string().contains("implement Verify"));
    }

    #[test]
    fn rejects_invalid_presence_overrides() {
        let error = expand(
            parse_quote! {
                struct Message {
                    #[prost(message, repeated, tag = "1")]
                    #[proto_decode(optional)]
                    value: Vec<Nested>,
                }
            },
            quote!(::runtime),
        )
        .unwrap_err();
        assert!(error.to_string().contains("singular Option<Message>"));
    }
}
