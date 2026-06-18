use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::document::Document;
use super::symbols::SymbolProvider;
use super::types::DocumentSymbol;

pub(crate) const MAX_INDEXED_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_RECURSION_DEPTH: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceSymbolIndexStats {
    pub(crate) indexed_files: usize,
    pub(crate) skipped_file_cap: usize,
    pub(crate) skipped_large_files: usize,
    pub(crate) skipped_non_utf8: usize,
    pub(crate) skipped_read_errors: usize,
    pub(crate) unsupported_root: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WorkspaceSymbolIndex {
    symbols_by_uri: BTreeMap<String, Vec<DocumentSymbol>>,
    stats: WorkspaceSymbolIndexStats,
}

impl WorkspaceSymbolIndex {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn stats(&self) -> &WorkspaceSymbolIndexStats {
        &self.stats
    }

    pub(crate) fn symbols(&self) -> &BTreeMap<String, Vec<DocumentSymbol>> {
        &self.symbols_by_uri
    }

    pub(crate) fn rebuild_from_uri(
        &mut self,
        root_uri: Option<&str>,
        symbols: &SymbolProvider,
    ) -> WorkspaceSymbolIndexStats {
        let Some(root_uri) = root_uri else {
            return self.clear_unsupported();
        };
        let Some(root_path) = file_uri_to_path(root_uri) else {
            return self.clear_unsupported();
        };
        if !root_path.is_dir() {
            return self.clear_unsupported();
        }
        self.rebuild_from_path(root_uri, &root_path, symbols)
    }

    pub(crate) fn rebuild_from_path(
        &mut self,
        root_uri: &str,
        root: &Path,
        symbols: &SymbolProvider,
    ) -> WorkspaceSymbolIndexStats {
        self.symbols_by_uri.clear();
        self.stats = WorkspaceSymbolIndexStats::default();
        let Ok(canonical_root) = root.canonicalize() else {
            return self.clear_unsupported();
        };

        let base_uri = root_uri.trim_end_matches('/');
        self.scan_dir(base_uri, &canonical_root, &canonical_root, 0, symbols);
        self.stats.clone()
    }

    fn clear_unsupported(&mut self) -> WorkspaceSymbolIndexStats {
        self.symbols_by_uri.clear();
        self.stats = WorkspaceSymbolIndexStats {
            unsupported_root: 1,
            ..WorkspaceSymbolIndexStats::default()
        };
        self.stats.clone()
    }

    fn scan_dir(
        &mut self,
        base_uri: &str,
        root: &Path,
        dir: &Path,
        depth: usize,
        symbols: &SymbolProvider,
    ) {
        if depth > MAX_RECURSION_DEPTH {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            self.stats.skipped_read_errors += 1;
            return;
        };
        let mut paths = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        paths.sort();

        for path in paths {
            if path.is_dir() {
                if !is_excluded_dir(&path) {
                    self.scan_dir(base_uri, root, &path, depth + 1, symbols);
                }
                continue;
            }
            if !is_quanta_file(&path) {
                continue;
            }
            if self.stats.indexed_files >= MAX_INDEXED_FILES {
                self.stats.skipped_file_cap += 1;
                continue;
            }
            self.index_file(base_uri, root, &path, symbols);
        }
    }

    fn index_file(&mut self, base_uri: &str, root: &Path, path: &Path, symbols: &SymbolProvider) {
        let Ok(metadata) = fs::metadata(path) else {
            self.stats.skipped_read_errors += 1;
            return;
        };
        if metadata.len() > MAX_FILE_BYTES {
            self.stats.skipped_large_files += 1;
            return;
        }
        let Ok(bytes) = fs::read(path) else {
            self.stats.skipped_read_errors += 1;
            return;
        };
        let Ok(content) = String::from_utf8(bytes) else {
            self.stats.skipped_non_utf8 += 1;
            return;
        };
        let Ok(canonical_path) = path.canonicalize() else {
            self.stats.skipped_read_errors += 1;
            return;
        };
        let Some(uri) = indexed_file_uri(base_uri, root, &canonical_path) else {
            self.stats.skipped_read_errors += 1;
            return;
        };
        let doc = Document::new(uri.clone(), "quanta".to_string(), 0, content);
        self.symbols_by_uri
            .insert(uri, symbols.document_symbols(&doc));
        self.stats.indexed_files += 1;
    }
}

fn is_quanta_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("quanta")
}

fn is_excluded_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".worktrees" | "target" | "node_modules" | "dist" | "build" | ".Codex")
    )
}

fn indexed_file_uri(base_uri: &str, root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut uri = base_uri.to_string();
    for component in relative.components() {
        let part = component.as_os_str().to_str()?;
        uri.push('/');
        uri.push_str(&encode_uri_path_segment(part));
    }
    Some(uri)
}

fn encode_uri_path_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => {
                use std::fmt::Write as _;
                write!(&mut encoded, "%{byte:02X}").expect("encode uri segment");
            }
        }
    }
    encoded
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest)?;
    if cfg!(windows) {
        let trimmed = decoded.trim_start_matches('/');
        Some(PathBuf::from(trimmed))
    } else {
        Some(PathBuf::from(decoded))
    }
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = *bytes.get(i + 1)?;
            let lo = *bytes.get(i + 2)?;
            decoded.push(hex_value(hi)? * 16 + hex_value(lo)?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::document::DocumentStore;
    use crate::lsp::symbols::SymbolProvider;
    use std::sync::Arc;

    fn provider() -> SymbolProvider {
        SymbolProvider::new(Arc::new(DocumentStore::new()))
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "quantalang_lsp_index_{label}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn indexes_quanta_files_in_sorted_order_and_skips_excluded_dirs() {
        let root = temp_root("sorted");
        std::fs::write(root.join("b.quanta"), "fn beta() -> i32 { 2 }\n").expect("write b");
        std::fs::write(root.join("a.quanta"), "fn alpha() -> i32 { 1 }\n").expect("write a");
        std::fs::create_dir_all(root.join("target")).expect("create target");
        std::fs::write(
            root.join("target").join("hidden.quanta"),
            "fn hidden() {}\n",
        )
        .expect("write hidden");

        let mut index = WorkspaceSymbolIndex::new();
        let stats = index.rebuild_from_path("file:///workspace", &root, &provider());

        assert_eq!(stats.indexed_files, 2);
        assert_eq!(
            index.symbols().keys().cloned().collect::<Vec<_>>(),
            vec![
                "file:///workspace/a.quanta".to_string(),
                "file:///workspace/b.quanta".to_string(),
            ]
        );
        assert!(!index
            .symbols()
            .contains_key("file:///workspace/target/hidden.quanta"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn caps_indexed_files_and_records_skips() {
        let root = temp_root("cap");
        for i in 0..(MAX_INDEXED_FILES + 1) {
            std::fs::write(
                root.join(format!("f{i:03}.quanta")),
                format!("fn f{i}() {{}}\n"),
            )
            .expect("write file");
        }

        let mut index = WorkspaceSymbolIndex::new();
        let stats = index.rebuild_from_path("file:///workspace", &root, &provider());

        assert_eq!(stats.indexed_files, MAX_INDEXED_FILES);
        assert_eq!(stats.skipped_file_cap, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unsupported_root_uri_without_panicking() {
        let mut index = WorkspaceSymbolIndex::new();
        let stats = index.rebuild_from_uri(Some("memfs:///workspace"), &provider());

        assert_eq!(stats.unsupported_root, 1);
        assert!(index.symbols().is_empty());
    }
}
