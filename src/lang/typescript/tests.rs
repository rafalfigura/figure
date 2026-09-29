//! Unit tests of the TypeScript adapter.

use std::path::Path;

use super::*;
use crate::model::{ItemKind, Vis};

fn parse(src: &str) -> FileIndex {
    TypeScript.parse(src, Path::new("a/b.ts"))
}

/// `index.ts` defines its directory; tests, declaration files and other languages are skipped.
#[test]
fn module_paths() {
    let m = |p: &str| TypeScript.module_of(Path::new(p));
    assert_eq!(m("index.ts"), Some(vec![]));
    assert_eq!(m("api/index.tsx"), Some(vec!["api".to_string()]));
    assert_eq!(m("api/user.js"), Some(vec!["api".into(), "user".into()]));
    assert_eq!(m("api/user.test.ts"), None);
    assert_eq!(m("types.d.ts"), None);
    assert_eq!(m("style.css"), None);
}

/// Exported declarations are public; signatures stop at the body; JSDoc prose is the doc.
#[test]
fn items_and_docs() {
    let f = parse(
        "/** Adds.\n * @param a first\n */\nexport function add<T>(a: T): T { return a; }\n\
         function hidden() {}\nexport const twice = (n: number): number => n * 2;\n\
         export const LIMIT = 10;\nexport type Id = string;\nexport enum Color { Red, Blue }\n",
    );
    let got: Vec<_> = f
        .items
        .iter()
        .map(|i| (i.kind, i.name.as_str(), i.vis, i.signature.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (ItemKind::Fn, "add", Vis::Pub, "function add<T>(a: T): T"),
            (ItemKind::Fn, "hidden", Vis::Private, "function hidden()"),
            (
                ItemKind::Fn,
                "twice",
                Vis::Pub,
                "const twice = (n: number): number =>"
            ),
            (ItemKind::Const, "LIMIT", Vis::Pub, "const LIMIT = 10"),
            (ItemKind::Type, "Id", Vis::Pub, "type Id = string"),
            (ItemKind::Enum, "Color", Vis::Pub, "enum Color"),
        ]
    );
    assert_eq!(f.items[0].doc, ["Adds."]);
    assert_eq!(f.items[0].start_line, 1);
    assert_eq!(f.items[0].decl_line, 4);
    assert_eq!(f.items[5].fields, ["Red", "Blue"]);
}

/// Methods are items owned by their class; `private` and `#x` members are private.
#[test]
fn class_members() {
    let f = parse(
        "export class Store implements Repo {\n  size = 0;\n  /** Gets. */\n  get(id: string): Item { return x; }\n\
         private secret() {}\n  #hash() {}\n}\n",
    );
    let names: Vec<_> = f
        .items
        .iter()
        .map(|i| (i.qualified_name(), i.vis))
        .collect();
    assert_eq!(
        names,
        [
            ("Store".to_string(), Vis::Pub),
            ("Store::get".to_string(), Vis::Pub),
            ("Store::secret".to_string(), Vis::Private),
            ("Store::#hash".to_string(), Vis::Private),
        ]
    );
    assert_eq!(f.items[0].fields, ["size"]);
    assert_eq!(f.items[1].doc, ["Gets."]);
    assert_eq!(f.trait_impls[0].trait_name, "Repo");
}

/// Relative imports are anchored at the source root; bare ones stay as written.
#[test]
fn imports_become_paths() {
    let f = parse(
        "import a, { b as c } from './util';\nimport * as d from '../lib/index.js';\n\
         import React from 'react';\nimport { x } from '@scope/pkg/deep';\nimport './style.css';\n\
         const m = require('./m');\nexport { y } from './y';\n",
    );
    let paths: Vec<String> = f
        .refs
        .iter()
        .map(|r| {
            format!(
                "{}{}",
                if r.anchored { "/" } else { "" },
                r.segments.join("::")
            )
        })
        .collect();
    assert_eq!(
        paths,
        [
            "/a::util",
            "/a::util::b",
            "/lib",
            "react",
            "@scope/pkg::deep::x",
            "/a::m",
            "/a::y",
        ]
    );
}

/// A blank-line-separated leading JSDoc documents the file; an adjacent one, the item.
#[test]
fn module_doc() {
    let f = parse("/**\n * The store.\n */\n\n/** Item. */\nexport const a = 1;\n");
    assert_eq!(f.module_doc, ["The store."]);
    assert_eq!(f.module_doc_line, 1);
    assert_eq!(f.items[0].doc, ["Item."]);
    let g = parse("/** Only item. */\nexport const a = 1;\n");
    assert!(g.module_doc.is_empty());
}

/// `export { a }` marks a local declaration public; `export * from` is a re-export item.
#[test]
fn export_lists() {
    let f = parse(
        "function a() {}\nexport { a };\nexport * from './x';\nexport { p as q } from './p';\n",
    );
    assert_eq!(f.items[0].vis, Vis::Pub);
    let re: Vec<_> = f.items[1..]
        .iter()
        .map(|i| (i.name.as_str(), i.kind))
        .collect();
    assert_eq!(re, [("*", ItemKind::Use), ("q", ItemKind::Use)]);
}

/// CommonJS: `module.exports = { a }` and `exports.b = ...` make items public.
#[test]
fn commonjs_exports() {
    let f = parse(
        "function a() {}\nfunction b() {}\n/** Sets c. */\nexports.c = (x) => x;\nmodule.exports = { a };\n",
    );
    let got: Vec<_> = f.items.iter().map(|i| (i.name.as_str(), i.vis)).collect();
    assert_eq!(got, [("a", Vis::Pub), ("b", Vis::Private), ("c", Vis::Pub)]);
    assert_eq!(f.items[2].signature, "exports.c = (x) =>");
    assert_eq!(f.items[2].doc, ["Sets c."]);
}

/// Variance and `const` modifiers on type parameters parse without syntax errors, and the
/// signature keeps them.
#[test]
fn type_parameter_modifiers() {
    let f = parse(
        "export interface Box<in out T, const U = 1> { x: T }\nexport class C<out V> {}\nexport interface E<\n  /** c */\n  out S extends X = Y,\n  out C = 1,\n> {}\nexport interface D<in T = never, in out U extends X = Y, out W> {}\n",
    );
    assert!(!f.parse_errors);
    assert_eq!(f.items[0].signature, "interface Box<in out T, const U = 1>");
    assert_eq!(f.items[1].name, "C");
    assert_eq!(f.items[2].name, "E");
    assert_eq!(f.items[3].name, "D");
}

/// Express style: `var app = exports = module.exports = {}` exports `app`, and
/// `app.get = function` / `Route.prototype.x = function` are its public methods.
#[test]
fn commonjs_objects_and_prototypes() {
    let f = parse(
        "var app = exports = module.exports = {};\n/** Gets. */\napp.get = function get(path, fn) {};\n\
         function Route() {}\nRoute.prototype.run = (a) => a;\nvar local = {};\nlocal.x = function () {};\n",
    );
    let got: Vec<_> = f
        .items
        .iter()
        .map(|i| (i.qualified_name(), i.vis, i.signature.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (
                "app".to_string(),
                Vis::Pub,
                "var app = exports = module.exports = {}"
            ),
            ("app::get".to_string(), Vis::Pub, "get(path, fn)"),
            ("Route".to_string(), Vis::Private, "function Route()"),
            ("Route::run".to_string(), Vis::Private, "run(a) =>"),
            ("local".to_string(), Vis::Private, "var local = {}"),
            ("local::x".to_string(), Vis::Private, "x()"),
        ]
    );
    assert_eq!(f.items[1].doc, ["Gets."]);
}

/// A class exported by `export { C }` keeps its public methods public and its private ones private.
#[test]
fn listed_class_exports() {
    let f = parse("class C { a() {} private b() {} }\nexport { C };\n");
    let vis: Vec<_> = f.items.iter().map(|i| i.vis).collect();
    assert_eq!(vis, [Vis::Pub, Vis::Pub, Vis::Private]);
}

/// `export type * from` parses without syntax errors.
#[test]
fn export_type_star() {
    let f = parse("export type * from './t.js';\nexport type { A } from './a.js';\n");
    assert!(!f.parse_errors);
    assert_eq!(f.items.len(), 2);
}
