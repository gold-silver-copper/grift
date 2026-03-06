use indexmap::IndexMap;
use syn::visit::Visit;

/// Visitor that finds `Err(ArenaError::Variant)` expressions in function bodies.
pub struct ErrorSiteExtractor {
    /// variant name → list of function names where it is returned
    pub sites: IndexMap<String, Vec<String>>,
    current_fn: Option<String>,
}

impl ErrorSiteExtractor {
    pub fn new() -> Self {
        Self {
            sites: IndexMap::new(),
            current_fn: None,
        }
    }
}

impl<'ast> Visit<'ast> for ErrorSiteExtractor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let prev = self.current_fn.take();
        self.current_fn = Some(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let prev = self.current_fn.take();
        self.current_fn = Some(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        // Look for Err(ArenaError::Variant)
        if let syn::Expr::Path(func_path) = &*node.func {
            if path_ends_with(&func_path.path, "Err") && node.args.len() == 1 {
                if let syn::Expr::Path(arg_path) = &node.args[0] {
                    if is_arena_error_path(&arg_path.path) {
                        if let Some(variant) = arg_path.path.segments.last() {
                            let variant_name = variant.ident.to_string();
                            if let Some(fn_name) = &self.current_fn {
                                self.sites
                                    .entry(variant_name)
                                    .or_default()
                                    .push(fn_name.clone());
                            }
                        }
                    }
                }
                // Also match Err(ArenaError::Variant { .. })
                if let syn::Expr::Struct(s) = &node.args[0] {
                    if is_arena_error_path(&s.path) {
                        if let Some(variant) = s.path.segments.last() {
                            let variant_name = variant.ident.to_string();
                            if let Some(fn_name) = &self.current_fn {
                                self.sites
                                    .entry(variant_name)
                                    .or_default()
                                    .push(fn_name.clone());
                            }
                        }
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

fn path_ends_with(path: &syn::Path, name: &str) -> bool {
    path.segments.last().is_some_and(|s| s.ident == name)
}

fn is_arena_error_path(path: &syn::Path) -> bool {
    let segs: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    // Match ArenaError::Variant or crate::arena::ArenaError::Variant
    segs.len() >= 2 && segs[segs.len() - 2] == "ArenaError"
}
