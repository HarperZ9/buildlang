// ===============================================================================
// LINE MAP: FROM THE JOINED PACKAGE TEXT BACK TO THE FILES IT CAME FROM
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! A package is checked and built as one joined text (see [`super::source`]).
//! A diagnostic position in that text names a file that does not exist, so
//! the join records, for every line it writes, the file and line it copied.
//! [`LineMap::origin`] turns a line of the joined text back into that place.

/// Where one line of the joined text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin<'a> {
    /// The file as the user can open it: `<package dir>/src/<name>.bld`,
    /// with `/` separators on every platform.
    pub file: &'a str,
    /// 1-based line in that file.
    pub line: usize,
    /// Byte offset of the start of that line in that file.
    pub byte_start: u32,
    /// True for a file of the package being checked, false for a file of a
    /// locked dependency.
    pub own: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    file: usize,
    line: usize,
    byte_start: u32,
}

/// The origin of every line of a joined package text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineMap {
    /// `(shown path, belongs to the checked package)`.
    files: Vec<(String, bool)>,
    /// One entry per joined line, in order. `None` for a line the join wrote
    /// itself (a header or a separator).
    rows: Vec<Option<Row>>,
}

impl LineMap {
    /// Start a file; lines pushed with [`LineMap::copied`] belong to it.
    pub fn begin_file(&mut self, shown: String, own: bool) {
        self.files.push((shown, own));
    }

    /// The join wrote a line of its own.
    pub fn synthetic(&mut self) {
        self.rows.push(None);
    }

    /// The join copied line `line` (1-based, starting at `byte_start`) of the
    /// file begun last.
    pub fn copied(&mut self, line: usize, byte_start: usize) {
        let file = self.files.len().saturating_sub(1);
        self.rows.push(Some(Row {
            file,
            line,
            byte_start: byte_start as u32,
        }));
    }

    /// Number of joined lines recorded.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when no line was recorded.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The origin of joined line `joined_line` (1-based). `None` for a line
    /// the join wrote itself or a line past the end.
    pub fn origin(&self, joined_line: usize) -> Option<Origin<'_>> {
        let row = (*self.rows.get(joined_line.checked_sub(1)?)?)?;
        let (file, own) = self.files.get(row.file)?;
        Some(Origin {
            file,
            line: row.line,
            byte_start: row.byte_start,
            own: *own,
        })
    }

    /// Every joined file, dependencies first, in join order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(shown, _)| shown.as_str())
    }

    /// Every file of the checked package, in join order.
    pub fn own_files(&self) -> impl Iterator<Item = &str> {
        self.files
            .iter()
            .filter(|(_, own)| *own)
            .map(|(shown, _)| shown.as_str())
    }
}

/// `dir` and a `/`-separated relative path as one shown path. `\` becomes
/// `/` so a diagnostic reads the same on every platform.
pub fn shown_path(dir: &std::path::Path, rel: &str) -> String {
    let base = dir.to_string_lossy().replace('\\', "/");
    let base = base.trim_end_matches('/');
    if base.is_empty() || base == "." {
        rel.to_string()
    } else {
        format!("{}/{}", base, rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copied_line_maps_back_and_a_written_line_does_not() {
        let mut map = LineMap::default();
        map.synthetic();
        map.begin_file("pkg/src/a.bld".to_string(), true);
        map.copied(1, 0);
        map.copied(3, 20);
        map.synthetic();
        assert_eq!(map.len(), 4);
        assert_eq!(map.origin(1), None);
        let o = map.origin(3).expect("line 3 was copied");
        assert_eq!(
            (o.file, o.line, o.byte_start, o.own),
            ("pkg/src/a.bld", 3, 20, true)
        );
        assert_eq!(map.origin(4), None);
        assert_eq!(map.origin(5), None);
        assert_eq!(map.origin(0), None);
    }

    #[test]
    fn shown_paths_use_forward_slashes() {
        let dir = std::path::Path::new("some\\pkg\\");
        assert_eq!(shown_path(dir, "src/a.bld"), "some/pkg/src/a.bld");
        assert_eq!(
            shown_path(std::path::Path::new("."), "src/a.bld"),
            "src/a.bld"
        );
    }
}
