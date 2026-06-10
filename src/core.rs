use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use skim::prelude::{DisplayContext, ItemPreview, Matches, PreviewContext, SkimItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeItemKind {
    File,
    Symbol,
    TextMatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl Location {
    pub fn new(path: impl Into<PathBuf>, line: Option<usize>, column: Option<usize>) -> Self {
        Self {
            path: path.into(),
            line,
            column,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn display_path(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
}

#[derive(Debug, Clone)]
pub struct CodeItem {
    pub kind: CodeItemKind,
    pub label: String,
    pub detail: String,
    pub location: Location,
    display: String,
}

impl CodeItem {
    pub fn from_parts(
        kind: CodeItemKind,
        label: impl Into<String>,
        detail: impl Into<String>,
        location: Location,
        display: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            label: label.into(),
            detail: detail.into(),
            location,
            display: display.into(),
        }
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let display = path.to_string_lossy().replace('\\', "/");

        Self::file_with_display(path, display)
    }

    pub fn file_with_display(path: impl Into<PathBuf>, display: impl Into<String>) -> Self {
        let path = path.into();
        let display = display.into();

        Self {
            kind: CodeItemKind::File,
            label: display.clone(),
            detail: String::new(),
            location: Location::new(path, Some(1), None),
            display,
        }
    }

    pub fn text_match(path: impl Into<PathBuf>, line: usize, column: Option<usize>, text: impl Into<String>) -> Self {
        let path = path.into();
        let normalized_path = path.to_string_lossy().replace('\\', "/");
        let text = text.into();
        let display = format!("{normalized_path}:{line}:{text}");

        Self {
            kind: CodeItemKind::TextMatch,
            label: normalized_path,
            detail: text,
            location: Location::new(path, Some(line), column),
            display,
        }
    }

    pub fn symbol(
        path: impl Into<PathBuf>,
        display_path: impl Into<String>,
        line: usize,
        column: Option<usize>,
        name: impl Into<String>,
        symbol_kind: impl Into<String>,
    ) -> Self {
        let path = path.into();
        let display_path = display_path.into();
        let name = name.into();
        let symbol_kind = symbol_kind.into();
        let detail = format!("{name} [{symbol_kind}]");
        let display = format!("{display_path}:{line}:{detail}");

        Self {
            kind: CodeItemKind::Symbol,
            label: display_path,
            detail,
            location: Location::new(path, Some(line), column),
            display,
        }
    }

    pub fn display_text(&self) -> &str {
        &self.display
    }
}

impl SkimItem for CodeItem {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.display)
    }

    fn display(&self, context: DisplayContext) -> Line<'_> {
        let path_style = Style::default().fg(Color::Cyan);
        let detail_style = Style::default();
        let separator_style = Style::default().fg(Color::DarkGray);
        let matched_style = context.matched_style;
        let base_style = context.base_style;
        let highlight_positions = highlight_positions(&self.display, context.matches);

        let mut spans = Vec::new();
        match self.kind {
            CodeItemKind::File => {
                push_styled_segment(
                    &mut spans,
                    &self.display,
                    0,
                    path_style.patch(base_style),
                    &highlight_positions,
                    matched_style,
                );
            }
            CodeItemKind::Symbol | CodeItemKind::TextMatch => {
                let line = self.location.line.unwrap_or(1).to_string();
                let line_style = Style::default().fg(Color::Green);
                let mut offset = 0;

                push_styled_segment(
                    &mut spans,
                    &self.label,
                    offset,
                    path_style.patch(base_style),
                    &highlight_positions,
                    matched_style,
                );
                offset += self.label.chars().count();

                push_styled_segment(
                    &mut spans,
                    ":",
                    offset,
                    separator_style.patch(base_style),
                    &highlight_positions,
                    matched_style,
                );
                offset += 1;

                push_styled_segment(
                    &mut spans,
                    &line,
                    offset,
                    line_style.patch(base_style),
                    &highlight_positions,
                    matched_style,
                );
                offset += line.chars().count();

                push_styled_segment(
                    &mut spans,
                    ":",
                    offset,
                    separator_style.patch(base_style),
                    &highlight_positions,
                    matched_style,
                );
                offset += 1;

                push_styled_segment(
                    &mut spans,
                    &self.detail,
                    offset,
                    detail_style.patch(base_style),
                    &highlight_positions,
                    matched_style,
                );
            }
        }

        Line::from(spans)
    }

    fn preview(&self, context: PreviewContext) -> ItemPreview {
        let line = self.location.line.unwrap_or(1);
        let path = self.location.display_path();

        match crate::preview::preview_path(&path, line, context.height) {
            Ok(ansi_text) => ItemPreview::AnsiText(ansi_text),
            Err(e) => ItemPreview::Text(format!("Failed to generate preview: {e}")),
        }
    }
}

fn highlight_positions(display: &str, matches: Matches) -> HashSet<usize> {
    match matches {
        Matches::CharIndices(indices) => indices.iter().copied().collect(),
        Matches::CharRange(start, end) => (start..end).collect(),
        Matches::ByteRange(start, end) => {
            let char_start = display.get(0..start).map_or(0, |value| value.chars().count());
            let char_end = display
                .get(0..end)
                .map_or(display.chars().count(), |value| value.chars().count());
            (char_start..char_end).collect()
        }
        Matches::None => HashSet::new(),
    }
}

fn push_styled_segment(
    spans: &mut Vec<Span<'static>>,
    segment: &str,
    offset: usize,
    base_style: Style,
    highlight_positions: &HashSet<usize>,
    matched_style: Style,
) {
    let mut current_text = String::new();
    let mut current_style = base_style;
    let mut style_initialized = false;

    for (index, ch) in segment.chars().enumerate() {
        let absolute_index = offset + index;
        let char_style = if highlight_positions.contains(&absolute_index) {
            base_style.patch(matched_style)
        } else {
            base_style
        };

        if !style_initialized {
            current_style = char_style;
            style_initialized = true;
        }

        if char_style != current_style {
            if !current_text.is_empty() {
                spans.push(Span::styled(current_text.clone(), current_style));
                current_text.clear();
            }
            current_style = char_style;
        }

        current_text.push(ch);
    }

    if !current_text.is_empty() {
        spans.push(Span::styled(current_text, current_style));
    }
}
