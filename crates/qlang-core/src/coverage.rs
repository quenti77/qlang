//! Statement coverage for `qlang test --coverage`.
//!
//! Off by default and costs nothing then. When on (per thread), the checker declares every statement
//! it sees and the tree walker records every statement it runs; [`take`] turns both into lines per file.

use crate::span::{SourceMap, Span};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashSet};

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static HITS: RefCell<HashSet<(u32, u32)>> = RefCell::new(HashSet::new());
    static ALL: RefCell<HashSet<(u32, u32)>> = RefCell::new(HashSet::new());
}

/// Turn the collection on or off for this thread.
pub fn enable(on: bool) {
    ON.with(|o| o.set(on));
}

pub fn enabled() -> bool {
    ON.with(|o| o.get())
}

/// A statement exists (seen by the checker).
pub fn declare(span: Span) {
    if enabled() {
        ALL.with(|a| a.borrow_mut().insert((span.file, span.start)));
    }
}

/// A statement ran.
pub fn hit(span: Span) {
    HITS.with(|h| h.borrow_mut().insert((span.file, span.start)));
}

/// The lines of one file: those with a statement, and those where one ran.
#[derive(Default, Debug)]
pub struct FileLines {
    pub lines: BTreeSet<u32>,
    pub hit: BTreeSet<u32>,
}

/// Empty what was collected and give it as lines per file name (files whose name starts with `<` are
/// built in and left out).
pub fn take(sources: &SourceMap) -> BTreeMap<String, FileLines> {
    let all = ALL.with(|a| std::mem::take(&mut *a.borrow_mut()));
    let hits = HITS.with(|h| std::mem::take(&mut *h.borrow_mut()));
    let mut out: BTreeMap<String, FileLines> = BTreeMap::new();
    let line_of = |file: u32, start: u32| {
        sources
            .locate(Span {
                file,
                start,
                end: start,
            })
            .line
    };
    for &(file, start) in all.iter().chain(hits.iter()) {
        let Some(f) = sources.file(file) else {
            continue;
        };
        if f.name.starts_with('<') {
            continue;
        }
        let entry = out.entry(f.name.clone()).or_default();
        let line = line_of(file, start);
        entry.lines.insert(line);
        if hits.contains(&(file, start)) {
            entry.hit.insert(line);
        }
    }
    out
}
