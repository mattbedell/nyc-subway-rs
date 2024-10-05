use proc_macro::TokenStream;
use proc_macro_error::{proc_macro_error, Diagnostic, Level};
use quote::{format_ident, quote};
use syn::{Arm, Attribute, Data, Expr, ExprMatch, Pat, Variant};

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
        let gen = quote! {
            impl #name {
                fn print_name() {
                    println!("Macro dervice impl for: {}", stringify!(#name));
                }

                fn do_match(input: u16) -> #name {
                    match input {
                        #(#disc => #name::#variants),*,
                        _ => #name::#fallback_variant,
                    }
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
