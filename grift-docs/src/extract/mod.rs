pub mod builtin;
pub mod enums;
pub mod error_sites;
pub mod methods;
pub mod prelude;
pub mod singletons;
pub mod source_map;

pub use builtin::BuiltinEntry;
pub use source_map::SourceMap;

/// Extract doc comments from `#[doc = "..."]` attributes.
pub fn extract_doc(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|a| {
            if a.path().is_ident("doc") {
                if let syn::Meta::NameValue(nv) = &a.meta {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        let t = s.value();
                        return Some(t.strip_prefix(' ').unwrap_or(&t).to_owned());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract inner doc comments (`//!`) from a syn::File.
pub fn extract_inner_doc(file: &syn::File) -> String {
    file.attrs
        .iter()
        .filter_map(|a| {
            if !matches!(a.style, syn::AttrStyle::Inner(_)) {
                return None;
            }
            if a.path().is_ident("doc") {
                if let syn::Meta::NameValue(nv) = &a.meta {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        let t = s.value();
                        return Some(t.strip_prefix(' ').unwrap_or(&t).to_owned());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}
