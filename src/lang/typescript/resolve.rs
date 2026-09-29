//! TypeScript / JavaScript import resolution. Relative imports arrive already anchored at the
//! source root; bare ones are aliases (`@/x`, `#x`, `baseUrl`), dependencies or Node builtins.

use super::imports::normalize;
use crate::index::Index;
use crate::model::FileIndex;
use crate::resolve::{Target, internal};

/// Modules Node ships with; they are not dependencies.
const BUILTINS: [&str; 32] = [
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "crypto",
    "dns",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "querystring",
    "readline",
    "stream",
    "string_decoder",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
    "test",
];

/// True for a module Node ships with.
pub fn is_builtin(name: &str) -> bool {
    BUILTINS.contains(&name)
}

/// Resolves `segs`; `anchored` paths are absolute below the source root already.
pub fn resolve(index: &Index, _file: &FileIndex, segs: &[String], anchored: bool) -> Target {
    if anchored {
        return internal(index, segs);
    }
    if let Some(path) = index.project.aliases.expand(segs, &index.modules) {
        return internal(index, &normalize(path));
    }
    match segs.first() {
        Some(name) if is_builtin(name) || index.project.externals.contains(name) => {
            Target::External(name.clone())
        }
        _ => Target::Other,
    }
}
