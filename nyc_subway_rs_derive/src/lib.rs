use proc_macro::TokenStream;
use proc_macro_error::{proc_macro_error, Diagnostic, Level};
use quote::{quote, format_ident};
use syn::Data;

#[proc_macro_derive(Deserialize_enum_or, attributes(fallback))]
pub fn deserialize_enum_or_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();

    impl_deserialize_enum(&ast)
}

#[proc_macro_error(allow_not_macro)]
fn impl_deserialize_enum(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    if let Data::Enum(data) = &ast.data {
        let fallback_variant = &data
            .variants
            .iter()
            .find(|variant| {
                variant
                    .attrs
                    .iter()
                    .find(|attr| attr.path().is_ident("fallback"))
                    .is_some()
            })
            .or_else(|| data.variants.first())
            .unwrap()
            .ident;
        let variants = data.variants.iter().map(|variant| &variant.ident);
        let disc = data.variants.iter().map(|variant| &variant.discriminant.as_ref().unwrap().1);
        let visitor_name = format_ident!("My{}Visitor", name);
        let gen = quote! {
            impl<'de> Deserialize<'de> for #name {
                fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    struct #visitor_name {}
                    impl<'de> Visitor<'de> for #visitor_name {
                        type Value = #name;

                        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                            write!(formatter, "an integer between 0 and {}", u8::MAX)
                        }

                        fn visit_u8<E>(self, v: u8) -> std::result::Result<Self::Value, E>
                            where
                                E: serde::de::Error, {
                            match v {
                                #(#disc => Ok(Self::Value::#variants)),*,
                                _ => Ok(Self::Value::#fallback_variant),
                            }
                        }

                        fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
                            where
                                E: serde::de::Error, {
                            Ok(Self::Value::#fallback_variant)
                        }

                        fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
                            where
                                D: Deserializer<'de>, {
                            deserializer.deserialize_u8(self)
                        }
                    }

                    deserializer.deserialize_option(#visitor_name {})
                }
            }
        };
        gen.into()
    } else {
        Diagnostic::spanned(
            ast.ident.span().unwrap().into(),
            Level::Error,
            "expected enum".to_owned(),
        )
        .abort();
    }
}
