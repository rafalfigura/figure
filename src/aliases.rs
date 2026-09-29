//! Import aliases as a language-neutral table: a specifier that starts with `prefix` lives in
//! the module `target`. Adapters fill it (TypeScript reads `tsconfig.json` and `package.json`);
//! the resolver only asks it to expand a specifier.

use std::collections::BTreeSet;

use crate::model::ModPath;

/// One alias: specifiers starting with `prefix` (or equal to it, when `exact`) live in `target`.
#[derive(Debug)]
struct Rule {
    prefix: String,
    exact: bool,
    target: ModPath,
}

/// All aliases of a project.
#[derive(Debug, Default)]
pub struct Aliases {
    /// Longest prefix first once [`Aliases::finish`] ran.
    rules: Vec<Rule>,
    /// Bare specifiers are also looked up below this module (`baseUrl`).
    base: Option<ModPath>,
}

impl Aliases {
    /// Adds an alias for `pattern`'s `prefix`.
    pub fn add(&mut self, prefix: &str, exact: bool, target: ModPath) {
        self.rules.push(Rule {
            prefix: prefix.to_string(),
            exact,
            target,
        });
    }

    /// Looks bare specifiers up below `base` too.
    pub fn set_base(&mut self, base: Option<ModPath>) {
        self.base = base;
    }

    /// Orders the rules so the longest prefix wins. Call once after the last [`Aliases::add`].
    pub fn finish(&mut self) {
        self.rules.sort_by(|a, b| {
            b.prefix
                .len()
                .cmp(&a.prefix.len())
                .then(a.prefix.cmp(&b.prefix))
        });
    }

    /// The module path a bare specifier stands for, if an alias or the base claims it. The last
    /// segments may still carry a file extension: the adapter normalizes them.
    pub fn expand(&self, segs: &[String], modules: &BTreeSet<ModPath>) -> Option<ModPath> {
        let joined = segs.join("/");
        for rule in &self.rules {
            let rest = if rule.exact {
                (joined == rule.prefix).then_some("")
            } else {
                joined.strip_prefix(&rule.prefix)
            };
            if let Some(rest) = rest {
                let mut path = rule.target.clone();
                path.extend(rest.split('/').filter(|p| !p.is_empty()).map(String::from));
                return Some(path);
            }
        }
        let base = self.base.as_ref()?;
        let mut path = base.clone();
        path.push(segs.first()?.clone());
        if !modules.contains(&path) {
            return None;
        }
        path.truncate(base.len());
        path.extend(segs.iter().cloned());
        Some(path)
    }
}
