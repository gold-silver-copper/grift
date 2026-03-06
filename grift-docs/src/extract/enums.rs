use syn::visit::Visit;

use super::extract_doc;

/// Documentation for an enum.
#[derive(Debug, Clone)]
pub struct EnumDoc {
    pub name: String,
    pub doc: String,
    pub variants: Vec<VariantDoc>,
}

/// Documentation for a single enum variant.
#[derive(Debug, Clone)]
pub struct VariantDoc {
    pub name: String,
    pub doc: String,
    pub fields: Vec<FieldDoc>,
}

/// Documentation for a field of a variant.
#[derive(Debug, Clone)]
pub struct FieldDoc {
    pub name: Option<String>,
    pub ty: String,
    pub doc: String,
}

/// Visitor that extracts documentation from specific enums.
pub struct EnumExtractor {
    target_names: Vec<String>,
    pub results: Vec<EnumDoc>,
}

impl EnumExtractor {
    pub fn new(targets: &[&str]) -> Self {
        Self {
            target_names: targets.iter().map(|s| s.to_string()).collect(),
            results: Vec::new(),
        }
    }

    pub fn find(&self, name: &str) -> Option<&EnumDoc> {
        self.results.iter().find(|e| e.name == name)
    }
}

impl<'ast> Visit<'ast> for EnumExtractor {
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let name = node.ident.to_string();
        if !self.target_names.contains(&name) {
            return;
        }

        let doc = extract_doc(&node.attrs);
        let mut variants = Vec::new();

        for v in &node.variants {
            let vdoc = extract_doc(&v.attrs);
            let mut fields = Vec::new();

            match &v.fields {
                syn::Fields::Named(named) => {
                    for f in &named.named {
                        let ty = &f.ty;
                        fields.push(FieldDoc {
                            name: f.ident.as_ref().map(|i| i.to_string()),
                            ty: quote::quote!(#ty).to_string(),
                            doc: extract_doc(&f.attrs),
                        });
                    }
                }
                syn::Fields::Unnamed(unnamed) => {
                    for f in &unnamed.unnamed {
                        let ty = &f.ty;
                        fields.push(FieldDoc {
                            name: None,
                            ty: quote::quote!(#ty).to_string(),
                            doc: extract_doc(&f.attrs),
                        });
                    }
                }
                syn::Fields::Unit => {}
            }

            variants.push(VariantDoc {
                name: v.ident.to_string(),
                doc: vdoc,
                fields,
            });
        }

        self.results.push(EnumDoc {
            name,
            doc,
            variants,
        });
    }
}
