//! Symbol interning.
//!
//! Symbols are never collected. Each symbol records whether its name starts
//! with `$`, so the sigil rule costs a single branch at call sites.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::value::SymId;

/// The symbol table.
#[derive(Clone, Debug, Default)]
pub(crate) struct Symbols {
    names: Vec<Box<str>>,
    dollar: Vec<bool>,
    ids: BTreeMap<Box<str>, SymId>,
}

impl Symbols {
    /// Intern `name`, returning its id.
    pub(crate) fn intern(&mut self, name: &str) -> SymId {
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = SymId(u32::try_from(self.names.len()).expect("symbol table overflow"));
        self.names.push(name.into());
        self.dollar.push(name.starts_with('$'));
        self.ids.insert(name.into(), id);
        id
    }

    /// The name of an interned symbol.
    pub(crate) fn name(&self, id: SymId) -> &str {
        &self.names[id.0 as usize]
    }

    /// True if the symbol's name starts with `$`.
    #[inline]
    pub(crate) fn is_dollar(&self, id: SymId) -> bool {
        self.dollar[id.0 as usize]
    }
}
