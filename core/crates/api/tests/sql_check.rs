//! Every SQL statement in the repo layer, prepared by MySQL against the migrated schema (Q-arch-3:
//! the CI check that stands in for compile-time checked queries). A typo, an unknown table or
//! column, or a wrong number of binds fails here, without running the query.
//!
//! The SQL is read from `crates/repo/src/*.rs`: every string literal that starts like a statement,
//! with the file's `const …: &str` SELECT prefixes filled in. List placeholders (`{}`, `{ids}`, …)
//! become one `?`; optional clauses (`{filter}`, …) become nothing. Statements built from names at
//! runtime can't be checked and are listed in `UNCHECKABLE`, so a new one fails until it's looked at.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use sqlx::{Either, Executor, Statement};

/// Placeholders that stand for a list of `?` marks.
const LISTS: [&str; 5] = ["", "ids", "marks", "list", "from"];
/// Placeholders that stand for an optional clause.
const CLAUSES: [&str; 4] = ["filter", "comp", "comp_filter", "not_in"];
/// (file, start of the statement): columns or tables chosen at runtime.
const UNCHECKABLE: [(&str, &str); 3] = [
    ("account.rs", "SELECT JSON_OBJECT({obj}) FROM `{table}`"),
    ("account.rs", "UPDATE AccountDeletion SET Status = ?, {col} = ?"),
    ("courses.rs", "UPDATE CourseShareInvite SET {col} = ?"),
];

/// A string literal: its value and the byte range of the literal in the source.
struct Lit {
    value: String,
    end: usize,
}

/// The string literals of a Rust file, outside comments; escapes and `\`-newline continuations
/// resolved. (The repo crate has no raw strings.)
fn literals(src: &str) -> Vec<Lit> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            // A char literal ('"', '\'', 'x'); a lifetime has no closing quote right after.
            b'\'' if b.get(i + 1) == Some(&b'\\') => {
                i += 2;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
                i += 1;
            }
            b'\'' if b.get(i + 2) == Some(&b'\'') => i += 3,
            b'"' => {
                let mut v = String::new();
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        match b.get(i + 1) {
                            Some(b'\n') | Some(b'\r') => {
                                i += 1;
                                while i < b.len() && b[i].is_ascii_whitespace() {
                                    i += 1;
                                }
                                continue;
                            }
                            Some(b'n') => v.push('\n'),
                            Some(b't') => v.push('\t'),
                            Some(&c) => v.push(c as char),
                            None => {}
                        }
                        i += 2;
                        continue;
                    }
                    let ch = src[i..].chars().next().expect("in bounds");
                    v.push(ch);
                    i += ch.len_utf8();
                }
                i += 1;
                out.push(Lit { value: v, end: i });
            }
            _ => i += 1,
        }
    }
    out
}

fn is_statement(s: &str) -> bool {
    let up = s.trim_start().to_ascii_uppercase();
    ["SELECT ", "INSERT ", "UPDATE ", "DELETE ", "WITH ", "REPLACE "].iter().any(|k| up.starts_with(k)) || up.starts_with("{")
}

/// Fills `{NAME}` placeholders; the flag says binds can't be counted (a list or clause was
/// filled in). `None` when one is unknown (built from names at runtime).
fn resolve(sql: &str, consts: &HashMap<String, String>) -> Option<(String, bool)> {
    let mut out = String::new();
    let mut lists = false;
    let mut rest = sql;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let name = &rest[open + 1..close];
        if let Some(v) = consts.get(name) {
            out.push_str(v);
        } else if name.is_empty() && !out.ends_with('(') {
            // A positional `{}` outside parentheses adds an optional clause (` FOR UPDATE`).
        } else if LISTS.contains(&name) {
            out.push('?');
            lists = true;
        } else if CLAUSES.contains(&name) {
            lists = true; // the clause may carry binds of its own
        } else {
            return None;
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Some((out, lists))
}

/// The `.bind(` calls chained on this literal's query, when the chain is straight-line: they all
/// come before its `.fetch…`/`.execute(` with no `;` (a loop or a `let`) in between. `None` otherwise.
fn binds_after(src: &str, end: usize) -> Option<usize> {
    let tail = &src[end..];
    let runs = [".fetch_one(", ".fetch_optional(", ".fetch_all(", ".fetch(", ".execute("];
    let run = runs.iter().filter_map(|r| tail.find(r)).min()?;
    let stmt = &tail[..run];
    if stmt.contains(';') || stmt.contains('{') {
        return None;
    }
    // Only when this literal is the query's own SQL: the statement starts the query call.
    let head = &src[..end];
    let start = head.rfind(['\n', ';', '{']).map_or(0, |p| p + 1);
    let line_start = head[..head.len().saturating_sub(1)].rfind(';').unwrap_or(0);
    if !src[line_start..end].contains("sqlx::query") || src[start..end].contains("let sql") {
        return None;
    }
    Some(stmt.matches(".bind(").count())
}

#[tokio::test]
async fn every_repo_statement_prepares_against_the_schema() {
    let Some(h) = common::harness().await else { return };
    let _ = (&h.t, &h.app);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../repo/src");
    let mut checked = 0;
    let mut skipped = Vec::new();
    let mut failures = Vec::new();
    let mut conn = h.db.pool.acquire().await.unwrap();
    let mut files: Vec<_> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension() == Some("rs".as_ref())).collect();
    files.sort();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let lits = literals(&src);
        // `const NAME: &str = "…";` prefixes, by the literal that follows the declaration.
        let mut consts = HashMap::new();
        for (i, l) in lits.iter().enumerate() {
            let before = &src[..l.end - l.value.len().min(l.end)];
            let decl = before.rfind("const ").map(|p| &before[p..]);
            if let Some(d) = decl.filter(|d| d.contains(": &str =") && !d.contains(';')) {
                let n = d["const ".len()..d.find(':').unwrap()].trim().to_string();
                if lits.get(i).is_some() {
                    consts.insert(n, l.value.clone());
                }
            }
        }
        for l in &lits {
            if !is_statement(&l.value) || consts.values().any(|v| v == &l.value) {
                continue;
            }
            let Some((sql, lists)) = resolve(&l.value, &consts) else {
                skipped.push((name.clone(), l.value.clone()));
                continue;
            };
            match conn.prepare(sql.as_str()).await {
                Ok(stmt) => {
                    checked += 1;
                    let params = match stmt.parameters() {
                        Some(Either::Right(n)) => n,
                        Some(Either::Left(p)) => p.len(),
                        None => 0,
                    };
                    if let (false, Some(binds)) = (lists, binds_after(&src, l.end)) {
                        if binds != params {
                            failures.push(format!("{name}: {params} placeholders but {binds} binds: {sql}"));
                        }
                    }
                }
                Err(e) => failures.push(format!("{name}: {e}\n    {sql}")),
            }
        }
    }
    for (file, sql) in &skipped {
        assert!(
            UNCHECKABLE.iter().any(|(f, start)| f == file && sql.starts_with(start)),
            "{file}: SQL built from runtime names isn't checked; add it to UNCHECKABLE if that's intended: {sql}"
        );
    }
    assert!(failures.is_empty(), "{} of {} statements failed:\n{}", failures.len(), checked + failures.len(), failures.join("\n"));
    assert!(checked > 250, "only {checked} statements found: is the extractor still reading the repo crate?");
    println!("{checked} statements prepared, {} skipped as runtime-built", skipped.len());
    h.db.drop_db().await;
}
