//! Checkout diff data and adaptation to GPUI Kit’s patch viewer.

use gpui_kit::component::diff::{DiffFile, DiffMode};

pub(crate) const ADDED_BG: u32 = 0x19382e;
pub(crate) const REMOVED_BG: u32 = 0x422a31;
pub(crate) const ADDED_TEXT: u32 = 0x9cdbb5;
pub(crate) const REMOVED_TEXT: u32 = 0xf0aaaa;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    #[default]
    Unified,
    Split,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Side {
    pub number: usize,
    pub text: String,
    pub mark: Mark,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub old: Option<Side>,
    pub new: Option<Side>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub hunks: Vec<Hunk>,
    pub note: Option<String>,
}

pub fn stats(file: &File) -> (usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    for line in file.hunks.iter().flat_map(|hunk| &hunk.lines) {
        if line
            .new
            .as_ref()
            .is_some_and(|side| side.mark == Mark::Added)
        {
            added += 1;
        }
        if line
            .old
            .as_ref()
            .is_some_and(|side| side.mark == Mark::Removed)
        {
            removed += 1;
        }
    }
    (added, removed)
}

impl Presentation {
    pub(crate) fn mode(self) -> DiffMode {
        match self {
            Self::Unified => DiffMode::Unified,
            Self::Split => DiffMode::Split,
        }
    }
}

/// Keep patch parsing off the UI thread along with checkout loading.
pub(crate) fn component_files(files: &[File]) -> Result<Vec<DiffFile>, String> {
    let mut patch = String::new();
    for file in files {
        // Git uses C-quoted paths, including octal escapes for UTF-8 bytes.
        let path = |prefix: &str| {
            let mut quoted = String::from("\"");
            for byte in format!("{prefix}{}", file.path).bytes() {
                match byte {
                    b'"' => quoted.push_str("\\\""),
                    b'\\' => quoted.push_str("\\\\"),
                    32..=126 => quoted.push(char::from(byte)),
                    _ => quoted.push_str(&format!("\\{byte:03o}")),
                }
            }
            quoted.push('"');
            quoted
        };
        let old = path("a/");
        let new = path("b/");
        patch.push_str(&format!("diff --git {old} {new}\n--- {old}\n+++ {new}\n"));
        for hunk in &file.hunks {
            patch.push_str(&hunk.header);
            patch.push('\n');
            let mut removed = Vec::new();
            let mut added = Vec::new();
            for line in &hunk.lines {
                if let Some(side) = &line.old
                    && side.mark == Mark::Context
                {
                    flush_changes(&mut patch, &mut removed, &mut added);
                    patch.push(' ');
                    patch.push_str(&side.text);
                    patch.push('\n');
                } else {
                    removed.extend(line.old.as_ref());
                    added.extend(line.new.as_ref());
                }
            }
            flush_changes(&mut patch, &mut removed, &mut added);
        }
    }
    DiffFile::parse(&patch).map_err(|error| error.to_string())
}

fn flush_changes<'a>(patch: &mut String, removed: &mut Vec<&'a Side>, added: &mut Vec<&'a Side>) {
    for (prefix, sides) in [('-', removed), ('+', added)] {
        for side in sides.drain(..) {
            debug_assert!(side.number > 0);
            patch.push(prefix);
            patch.push_str(&side.text);
            patch.push('\n');
        }
    }
}
