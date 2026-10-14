//! M1 exit criterion: the code in the docs compiles.
//!
//! Every fenced code block in the documents listed in `DOCS` must carry an info
//! string. Blocks whose info string starts with `build` are BuildLang and are
//! run through `buildc`; any other label (`text`, `bash`, `ebnf`, ...) marks a
//! block that is not BuildLang. An unlabeled fence fails the test, so BuildLang
//! cannot hide in an unchecked block.
//!
//! Attributes after `build`, separated by spaces:
//!
//! - `name=ID`: other blocks in the same file can reuse this one.
//! - `uses=ID,ID`: prepend those named blocks, in order.
//! - `in-main`: wrap the block in `fn main() ~ Console { ... }` (statement snippets).
//! - `fail=BL0402`: `buildc check` must reject the program and report that code.
//! - `parse-only`: `buildc parse` must accept it (syntax sketches).
//! - `lex-only`: `buildc lex` must accept it (token listings).
//!
//! Without a mode, `buildc check` must accept the program. A program with no
//! `fn main` gets an empty one appended.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

const DOCS: &[&str] = &["SPECIFICATION.md", "docs/EFFECTS_GUIDE.md", "README.md"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn buildc() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    cmd
}

#[derive(Debug, Default)]
struct Block {
    line: usize,
    lang: String,
    attrs: Vec<String>,
    code: String,
}

impl Block {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find_map(|a| a.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
    }

    fn flag(&self, key: &str) -> bool {
        self.attrs.iter().any(|a| a == key)
    }
}

/// Split a Markdown file into fenced blocks. Indented fences (inside list
/// items) are handled by stripping the fence's indentation from each line.
fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current: Option<(Block, usize, String)> = None;
    for (i, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        if let Some(rest) = trimmed.strip_prefix("```") {
            match current.take() {
                None => {
                    let mut words = rest.split_whitespace().map(str::to_string);
                    let block = Block {
                        line: i + 1,
                        lang: words.next().unwrap_or_default(),
                        attrs: words.collect(),
                        code: String::new(),
                    };
                    current = Some((block, indent, String::new()));
                }
                Some((mut block, _, code)) => {
                    block.code = code;
                    out.push(block);
                }
            }
            continue;
        }
        if let Some((_, fence_indent, code)) = current.as_mut() {
            let cut = raw
                .char_indices()
                .take_while(|(n, c)| *n < *fence_indent && *c == ' ')
                .count();
            code.push_str(&raw[cut..]);
            code.push('\n');
        }
    }
    out
}

fn program(block: &Block, named: &HashMap<String, String>) -> Result<String, String> {
    let mut src = String::new();
    if let Some(uses) = block.attr("uses") {
        for id in uses.split(',') {
            let code = named
                .get(id)
                .ok_or_else(|| format!("uses={id}, but no earlier block has name={id}"))?;
            src.push_str(code);
            src.push('\n');
        }
    }
    if block.flag("in-main") {
        src.push_str("fn main() ~ Console {\n");
        src.push_str(&block.code);
        src.push_str("}\n");
    } else {
        src.push_str(&block.code);
    }
    let lex_or_parse = block.flag("lex-only") || block.flag("parse-only");
    if !lex_or_parse && !src.contains("fn main") {
        src.push_str("\nfn main() {}\n");
    }
    Ok(src)
}

fn run(block: &Block, file: &Path) -> Result<(), String> {
    let (sub, json) = if block.flag("lex-only") {
        ("lex", false)
    } else if block.flag("parse-only") {
        ("parse", false)
    } else {
        ("check", true)
    };
    let mut cmd = buildc();
    cmd.arg(sub).arg(file);
    if json {
        cmd.arg("--error-format=json");
    }
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run buildc: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    match block.attr("fail") {
        None if out.status.success() => Ok(()),
        None => {
            let detail = if json {
                stdout.to_string()
            } else {
                stderr.to_string()
            };
            Err(format!("`buildc {sub}` rejected it:\n{detail}"))
        }
        Some(_) if out.status.success() => {
            Err("expected a rejection; `buildc check` passed".into())
        }
        Some(code) => {
            if stdout.contains(&format!("\"code\": \"{code}\"")) {
                Ok(())
            } else {
                Err(format!("rejected, but not with {code}:\n{stdout}"))
            }
        }
    }
}

#[test]
fn every_build_block_in_the_docs_checks() {
    let root = repo_root();
    let tmp = std::env::temp_dir().join(format!("buildlang_doc_blocks_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create temp dir");
    let mut failures = Vec::new();
    let mut checked = 0;
    for doc in DOCS {
        let text = std::fs::read_to_string(root.join(doc)).expect("doc exists");
        let mut named = HashMap::new();
        for (n, block) in blocks(&text).into_iter().enumerate() {
            let at = format!("{doc}:{}", block.line);
            if block.lang.is_empty() {
                failures.push(format!(
                    "{at}: unlabeled code fence; label it `build` or `text`"
                ));
                continue;
            }
            if block.lang != "build" {
                continue;
            }
            let src = match program(&block, &named) {
                Ok(src) => src,
                Err(e) => {
                    failures.push(format!("{at}: {e}"));
                    continue;
                }
            };
            if let Some(id) = block.attr("name") {
                named.insert(id.to_string(), block.code.clone());
            }
            let file = tmp.join(format!("{}_{n}.bld", doc.replace(['/', '.'], "_")));
            std::fs::write(&file, &src).expect("write block");
            checked += 1;
            if let Err(e) = run(&block, &file) {
                failures.push(format!("{at}: {e}\n--- program ---\n{src}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(
        checked >= 40,
        "only {checked} build blocks found; the docs lost their examples"
    );
    assert!(
        failures.is_empty(),
        "{} doc block(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn block_splitter_reads_labels_attributes_and_indented_fences() {
    let text = "a\n```build name=x uses=y,z fail=BL0402\nfn f() {}\n```\n\n  ```text\n  hi\n  ```\n```\nbare\n```\n";
    let b = blocks(text);
    assert_eq!(b.len(), 3);
    assert_eq!(b[0].lang, "build");
    assert_eq!(b[0].attr("name"), Some("x"));
    assert_eq!(b[0].attr("uses"), Some("y,z"));
    assert_eq!(b[0].attr("fail"), Some("BL0402"));
    assert_eq!(b[0].code, "fn f() {}\n");
    assert_eq!(b[1].lang, "text");
    assert_eq!(b[1].code, "hi\n");
    assert_eq!(b[2].lang, "");
}
