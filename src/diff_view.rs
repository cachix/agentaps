//! Read-only GPUI Kit adaptation of GPUI Box Kit's MIT-licensed DiffView.
//! Source: https://github.com/fran0220/gpui-box/blob/24afaaf1b2f34827342b5ad04a4efba95e705c45/crates/gpui-kit/src/content/diff_view.rs
//! Original license: licenses/GPUI-Box-Kit-MIT.txt.
//! Diff production and filesystem access belong to the caller.

use crate::theme::Palette;
use crate::theme::{ACCENT, BG, BORDER, MUTED, SURFACE, TEXT};
use gpui_kit::{AnyElement, IntoElement, div, prelude::*, px};

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

#[derive(Clone, Debug)]
pub struct Side {
    pub number: usize,
    pub text: String,
    pub mark: Mark,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub old: Option<Side>,
    pub new: Option<Side>,
}

#[derive(Clone, Debug)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug)]
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

#[derive(Clone)]
pub enum Row {
    File(String),
    Hunk(String),
    Unified {
        side: Side,
        old_number: Option<usize>,
        new_number: Option<usize>,
    },
    Split {
        old: Option<Side>,
        new: Option<Side>,
    },
    Note(String),
}

pub fn flatten(files: &[File], presentation: Presentation) -> Vec<Row> {
    let mut rows = Vec::new();
    for file in files {
        rows.push(Row::File(file.path.clone()));
        if let Some(note) = &file.note {
            rows.push(Row::Note(note.clone()));
        }
        for hunk in &file.hunks {
            rows.push(Row::Hunk(hunk.header.clone()));
            for line in &hunk.lines {
                match presentation {
                    Presentation::Split => rows.push(Row::Split {
                        old: line.old.clone(),
                        new: line.new.clone(),
                    }),
                    Presentation::Unified => match (&line.old, &line.new) {
                        (Some(old), Some(new)) if old.mark == Mark::Context => {
                            rows.push(Row::Unified {
                                side: old.clone(),
                                old_number: Some(old.number),
                                new_number: Some(new.number),
                            });
                        }
                        (Some(old), Some(new)) => {
                            rows.push(Row::Unified {
                                side: old.clone(),
                                old_number: Some(old.number),
                                new_number: None,
                            });
                            rows.push(Row::Unified {
                                side: new.clone(),
                                old_number: None,
                                new_number: Some(new.number),
                            });
                        }
                        (Some(old), None) => rows.push(Row::Unified {
                            side: old.clone(),
                            old_number: Some(old.number),
                            new_number: None,
                        }),
                        (None, Some(new)) => rows.push(Row::Unified {
                            side: new.clone(),
                            old_number: None,
                            new_number: Some(new.number),
                        }),
                        (None, None) => {}
                    },
                }
            }
        }
    }
    rows
}

pub(crate) fn render_row(row: &Row, palette: Palette) -> AnyElement {
    match row {
        Row::File(path) => div()
            .h(px(32.))
            .px_3()
            .flex()
            .items_center()
            .bg(palette.color(SURFACE))
            .text_sm()
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .text_color(palette.color(TEXT))
            .child(path.clone())
            .into_any_element(),
        Row::Hunk(header) => div()
            .h(px(28.))
            .px_3()
            .flex()
            .items_center()
            .bg(palette.color(BORDER))
            .text_xs()
            .font_family("monospace")
            .text_color(palette.color(ACCENT))
            .child(header.clone())
            .into_any_element(),
        Row::Note(note) => div()
            .h(px(30.))
            .px_3()
            .flex()
            .items_center()
            .text_sm()
            .text_color(palette.color(MUTED))
            .child(note.clone())
            .into_any_element(),
        Row::Unified {
            side,
            old_number,
            new_number,
        } => div()
            .h(px(24.))
            .flex()
            .items_center()
            .bg(palette.color(mark_bg(side.mark)))
            .font_family("monospace")
            .text_xs()
            .child(number(*old_number, palette))
            .child(number(*new_number, palette))
            .child(prefix(side.mark, palette))
            .child(code(&side.text, palette))
            .into_any_element(),
        Row::Split { old, new } => div()
            .w_full()
            .h(px(24.))
            .flex()
            .items_center()
            .font_family("monospace")
            .text_xs()
            .child(split_side(old.as_ref(), true, palette))
            .child(split_side(new.as_ref(), false, palette))
            .into_any_element(),
    }
}

fn split_side(side: Option<&Side>, divider: bool, palette: Palette) -> impl IntoElement {
    let mark = side.map_or(Mark::Context, |side| side.mark);
    div()
        .flex()
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .items_center()
        .bg(palette.color(mark_bg(mark)))
        .when(divider, |side| {
            side.border_r_1().border_color(palette.color(BORDER))
        })
        .child(number(side.map(|side| side.number), palette))
        .child(prefix(mark, palette))
        .child(code(side.map_or("", |side| side.text.as_str()), palette))
}

fn number(value: Option<usize>, palette: Palette) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .w(px(42.))
        .pr_2()
        .text_right()
        .text_color(palette.color(MUTED))
        .child(value.map_or_else(String::new, |number| number.to_string()))
}

fn prefix(mark: Mark, palette: Palette) -> impl IntoElement {
    let (symbol, color) = match mark {
        Mark::Context => (" ", MUTED),
        Mark::Added => ("+", ADDED_TEXT),
        Mark::Removed => ("-", REMOVED_TEXT),
    };
    div()
        .flex_shrink_0()
        .w(px(18.))
        .text_color(palette.color(color))
        .child(symbol)
}

fn code(value: &str, palette: Palette) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(palette.color(TEXT))
        .child(value.to_owned())
}

fn mark_bg(mark: Mark) -> u32 {
    match mark {
        Mark::Context => BG,
        Mark::Added => ADDED_BG,
        Mark::Removed => REMOVED_BG,
    }
}
