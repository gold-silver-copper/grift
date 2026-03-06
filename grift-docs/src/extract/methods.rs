use indexmap::IndexMap;
use syn::visit::Visit;

use super::extract_doc;

/// Documentation for a single method.
#[derive(Debug, Clone)]
pub struct MethodDoc {
    pub name: String,
    pub doc: String,
}

/// Index of all documented methods, keyed by method name.
pub type MethodIndex = IndexMap<String, MethodDoc>;

/// Visitor that extracts documentation from all impl methods.
pub struct ImplMethodExtractor {
    pub methods: MethodIndex,
}

impl ImplMethodExtractor {
    pub fn new() -> Self {
        Self {
            methods: IndexMap::new(),
        }
    }
}

impl<'ast> Visit<'ast> for ImplMethodExtractor {
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let doc = extract_doc(&node.attrs);
        if !doc.is_empty() {
            let name = node.sig.ident.to_string();
            self.methods.insert(
                name.clone(),
                MethodDoc { name, doc },
            );
        }
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let doc = extract_doc(&node.attrs);
        if !doc.is_empty() {
            let name = node.sig.ident.to_string();
            self.methods.insert(
                name.clone(),
                MethodDoc { name, doc },
            );
        }
        syn::visit::visit_item_fn(self, node);
    }
}
