use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::config::TuiThemeConfig;

use super::preview_cache::PreviewWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    CLike,
    Rust,
    Python,
    Shell,
    Toml,
    Json,
    Markdown,
    Generic,
}

pub(super) fn theme_style(theme: &TuiThemeConfig, mut style: Style) -> Style {
    if !theme.color {
        style.fg = None;
        style.bg = None;
        return style;
    }
    if theme.low_color {
        style.fg = style.fg.map(low_color);
        style.bg = style.bg.map(low_color);
    }
    style
}

pub(super) fn selection_style(theme: &TuiThemeConfig, fg: Color, bg: Color) -> Style {
    if !theme.color {
        return Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED);
    }
    theme_style(theme, Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD))
}

pub(super) fn preview_lines(window: &PreviewWindow, theme: &TuiThemeConfig) -> Vec<Line<'static>> {
    if let Some(message) = &window.message {
        return vec![Line::from(Span::styled(
            message.clone(),
            theme_style(theme, Style::default().fg(Color::LightRed)),
        ))];
    }

    window
        .lines
        .iter()
        .map(|line| {
            let line_style = if line.is_target {
                theme_style(theme, Style::default().bg(Color::Rgb(36, 42, 54)))
            } else {
                Style::default()
            };
            let marker_style = if line.is_target {
                theme_style(theme, line_style.fg(Color::Yellow).add_modifier(Modifier::BOLD))
            } else {
                theme_style(theme, line_style.fg(Color::DarkGray))
            };
            let number_style = if line.is_target {
                theme_style(theme, line_style.fg(Color::LightYellow).add_modifier(Modifier::BOLD))
            } else {
                theme_style(theme, line_style.fg(Color::DarkGray))
            };
            let mut spans = vec![
                Span::styled(if line.is_target { ">" } else { " " }.to_string(), marker_style),
                Span::styled(format!(" {:>5} ", line.number), number_style),
                Span::styled("| ".to_string(), theme_style(theme, line_style.fg(Color::DarkGray))),
            ];
            spans.extend(highlight_code(&window.path, &line.text, line_style, theme));
            Line::from(spans)
        })
        .collect()
}

pub(super) fn highlight_code(path: &Path, text: &str, base_style: Style, theme: &TuiThemeConfig) -> Vec<Span<'static>> {
    let base_style = theme_style(theme, base_style);
    if !theme.syntax_highlight {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    let syntax = detect_syntax(path);
    if syntax == Syntax::Markdown {
        return highlight_markdown(text, base_style, theme);
    }

    let mut spans = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        if let Some(comment_len) = line_comment_len(syntax, rest, index, text) {
            push_span(&mut spans, rest, token_style(base_style, TokenKind::Comment, theme));
            index += comment_len;
            continue;
        }
        if rest.starts_with("/*") {
            let len = rest.find("*/").map_or(rest.len(), |offset| offset + 2);
            push_span(
                &mut spans,
                &rest[..len],
                token_style(base_style, TokenKind::Comment, theme),
            );
            index += len;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            let len = consume_while(rest, |value| value.is_whitespace());
            push_span(&mut spans, &rest[..len], base_style);
            index += len;
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            let len = consume_string(rest, ch);
            push_span(
                &mut spans,
                &rest[..len],
                token_style(base_style, TokenKind::String, theme),
            );
            index += len;
            continue;
        }
        if ch.is_ascii_digit() {
            let len = consume_while(rest, |value| {
                value.is_ascii_alphanumeric() || matches!(value, '.' | '_')
            });
            push_span(
                &mut spans,
                &rest[..len],
                token_style(base_style, TokenKind::Number, theme),
            );
            index += len;
            continue;
        }
        if is_identifier_start(ch) {
            let len = consume_while(rest, is_identifier_continue);
            let word = &rest[..len];
            let style = if is_keyword(syntax, word) {
                token_style(base_style, TokenKind::Keyword, theme)
            } else if is_builtin_constant(word) {
                token_style(base_style, TokenKind::Constant, theme)
            } else if next_non_space(&rest[len..]) == Some('(') {
                token_style(base_style, TokenKind::Function, theme)
            } else {
                base_style
            };
            push_span(&mut spans, word, style);
            index += len;
            continue;
        }

        let len = ch.len_utf8();
        let style = if is_operator_or_punctuation(ch) {
            token_style(base_style, TokenKind::Punctuation, theme)
        } else {
            base_style
        };
        push_span(&mut spans, &rest[..len], style);
        index += len;
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

pub(super) fn code_item_kind_style(kind: &crate::core::CodeItemKind, theme: &TuiThemeConfig) -> Style {
    match kind {
        crate::core::CodeItemKind::File => theme_style(theme, Style::default().fg(Color::LightBlue)),
        crate::core::CodeItemKind::Symbol => theme_style(theme, Style::default().fg(Color::LightYellow)),
        crate::core::CodeItemKind::TextMatch => theme_style(theme, Style::default().fg(Color::LightGreen)),
    }
}

pub(super) fn debug_line(value: &str, theme: &TuiThemeConfig) -> Line<'static> {
    let lower = value.to_ascii_lowercase();
    if value.starts_with("-- ") {
        return Line::from(Span::styled(
            value.to_string(),
            theme_style(
                theme,
                Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
            ),
        ));
    }
    if lower.contains("error") || lower.contains("failed") {
        return prefixed_line(value, Color::LightRed, theme);
    }
    if lower.contains("stopped") || lower.starts_with("stop:") {
        return prefixed_line(value, Color::Yellow, theme);
    }
    if lower.contains("running") || lower.contains("continued") {
        return prefixed_line(value, Color::LightGreen, theme);
    }
    if lower.starts_with("eval:") || lower.starts_with("watch") {
        return prefixed_line(value, Color::LightCyan, theme);
    }
    if lower.starts_with("caps:") || lower.starts_with("req/res") {
        return prefixed_line(value, Color::DarkGray, theme);
    }
    Line::from(Span::raw(value.to_string()))
}

pub(super) fn activity_line(label: &str, value: &str, color: Color, theme: &TuiThemeConfig) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            theme_style(theme, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ),
        Span::styled(value.to_string(), theme_style(theme, Style::default().fg(Color::Gray))),
    ])
}

fn prefixed_line(value: &str, color: Color, theme: &TuiThemeConfig) -> Line<'static> {
    let Some(index) = value.find(':') else {
        return Line::from(Span::styled(
            value.to_string(),
            theme_style(theme, Style::default().fg(color)),
        ));
    };
    let (prefix, rest) = value.split_at(index + 1);
    Line::from(vec![
        Span::styled(
            prefix.to_string(),
            theme_style(theme, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ),
        Span::styled(rest.to_string(), theme_style(theme, Style::default().fg(Color::Gray))),
    ])
}

#[derive(Debug, Clone, Copy)]
enum TokenKind {
    Keyword,
    Function,
    String,
    Number,
    Comment,
    Constant,
    Punctuation,
}

fn token_style(base_style: Style, kind: TokenKind, theme: &TuiThemeConfig) -> Style {
    let style = match kind {
        TokenKind::Keyword => Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
        TokenKind::Function => Style::default().fg(Color::LightCyan),
        TokenKind::String => Style::default().fg(Color::LightGreen),
        TokenKind::Number => Style::default().fg(Color::LightYellow),
        TokenKind::Comment => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        TokenKind::Constant => Style::default().fg(Color::Yellow),
        TokenKind::Punctuation => Style::default().fg(Color::Blue),
    };
    base_style.patch(theme_style(theme, style))
}

fn low_color(color: Color) -> Color {
    match color {
        Color::LightRed => Color::Red,
        Color::LightGreen => Color::Green,
        Color::LightYellow => Color::Yellow,
        Color::LightBlue => Color::Blue,
        Color::LightMagenta => Color::Magenta,
        Color::LightCyan => Color::Cyan,
        Color::Gray | Color::DarkGray => Color::White,
        Color::Rgb(_, _, _) | Color::Indexed(_) => Color::Blue,
        other => other,
    }
}

fn detect_syntax(path: &Path) -> Syntax {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => Syntax::Rust,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "java" | "js" | "ts" | "go" => Syntax::CLike,
        "py" | "pyw" => Syntax::Python,
        "sh" | "bash" | "zsh" | "fish" => Syntax::Shell,
        "toml" => Syntax::Toml,
        "json" | "jsonc" => Syntax::Json,
        "md" | "markdown" => Syntax::Markdown,
        _ => Syntax::Generic,
    }
}

fn line_comment_len(syntax: Syntax, rest: &str, index: usize, line: &str) -> Option<usize> {
    if matches!(syntax, Syntax::CLike | Syntax::Rust | Syntax::Json | Syntax::Generic) && rest.starts_with("//") {
        return Some(rest.len());
    }
    if matches!(syntax, Syntax::Python | Syntax::Shell | Syntax::Toml) && rest.starts_with('#') {
        return Some(rest.len());
    }
    if matches!(syntax, Syntax::CLike | Syntax::Rust) && rest.starts_with('#') && line[..index].trim().is_empty() {
        return Some(rest.len());
    }
    None
}

fn highlight_markdown(text: &str, base_style: Style, theme: &TuiThemeConfig) -> Vec<Span<'static>> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('#') {
        return vec![Span::styled(
            text.to_string(),
            theme_style(theme, base_style.fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        )];
    }
    if trimmed.starts_with("```") {
        return vec![Span::styled(
            text.to_string(),
            token_style(base_style, TokenKind::String, theme),
        )];
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("> ") {
        return vec![Span::styled(
            text.to_string(),
            theme_style(theme, base_style.fg(Color::LightYellow)),
        )];
    }
    highlight_inline_markup(text, base_style, theme)
}

fn highlight_inline_markup(text: &str, base_style: Style, theme: &TuiThemeConfig) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        if let Some(stripped) = rest.strip_prefix('`') {
            let len = stripped.find('`').map_or(1, |offset| offset + 2);
            push_span(
                &mut spans,
                &rest[..len],
                token_style(base_style, TokenKind::String, theme),
            );
            index += len;
            continue;
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        let len = ch.len_utf8();
        push_span(&mut spans, &rest[..len], base_style);
        index += len;
    }
    spans
}

fn is_keyword(syntax: Syntax, word: &str) -> bool {
    let keywords = match syntax {
        Syntax::Rust => RUST_KEYWORDS,
        Syntax::Python => PYTHON_KEYWORDS,
        Syntax::Shell => SHELL_KEYWORDS,
        Syntax::Toml => TOML_KEYWORDS,
        Syntax::Json => JSON_KEYWORDS,
        Syntax::CLike | Syntax::Generic | Syntax::Markdown => C_LIKE_KEYWORDS,
    };
    keywords.contains(&word)
}

fn is_builtin_constant(word: &str) -> bool {
    matches!(
        word,
        "true" | "false" | "True" | "False" | "NULL" | "null" | "None" | "Some" | "Ok" | "Err"
    )
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_operator_or_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}'
            | '('
            | ')'
            | '['
            | ']'
            | ':'
            | ';'
            | ','
            | '.'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '<'
            | '>'
            | '!'
            | '&'
            | '|'
            | '%'
            | '^'
    )
}

fn consume_while(input: &str, predicate: impl Fn(char) -> bool) -> usize {
    let mut end = 0;
    for (index, ch) in input.char_indices() {
        if !predicate(ch) {
            break;
        }
        end = index + ch.len_utf8();
    }
    end
}

fn consume_string(input: &str, quote: char) -> usize {
    let mut escaped = false;
    for (index, ch) in input.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return index + ch.len_utf8();
        }
    }
    input.len()
}

fn next_non_space(input: &str) -> Option<char> {
    input.chars().find(|value| !value.is_whitespace())
}

fn push_span(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.style == style {
            last.content.to_mut().push_str(text);
            return;
        }
    }
    spans.push(Span::styled(text.to_string(), style));
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static",
    "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
];

const C_LIKE_KEYWORDS: &[&str] = &[
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "explicit",
    "extern",
    "false",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "namespace",
    "new",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

const PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
    "return", "try", "while", "with", "yield",
];

const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in", "select", "then", "until",
    "while",
];

const TOML_KEYWORDS: &[&str] = &["true", "false"];
const JSON_KEYWORDS: &[&str] = &["true", "false", "null"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_keywords_and_comments() {
        let theme = TuiThemeConfig::default();
        let spans = highlight_code(
            Path::new("src/main.rs"),
            "pub fn main() { // run",
            Style::default(),
            &theme,
        );

        assert!(spans.iter().any(|span| span.content == "pub"));
        assert!(spans.iter().any(|span| span.content == "fn"));
        assert!(spans.iter().any(|span| span.content == "// run"));
    }

    #[test]
    fn renders_preview_with_line_metadata() {
        let window = PreviewWindow {
            path: Path::new("src/main.rs").to_path_buf(),
            target_line: 2,
            target_column: None,
            lines: vec![
                super::super::preview_cache::PreviewLine {
                    number: 1,
                    text: "fn before() {}".to_string(),
                    is_target: false,
                },
                super::super::preview_cache::PreviewLine {
                    number: 2,
                    text: "pub fn main() {}".to_string(),
                    is_target: true,
                },
            ],
            message: None,
        };

        let theme = TuiThemeConfig::default();
        let lines = preview_lines(&window, &theme);

        assert_eq!(lines.len(), 2);
        assert!(lines[1].spans.iter().any(|span| span.content.contains("pub")));
    }

    #[test]
    fn color_can_be_disabled_without_dropping_text() {
        let theme = TuiThemeConfig {
            color: false,
            ..TuiThemeConfig::default()
        };
        let spans = highlight_code(Path::new("src/main.rs"), "pub fn main()", Style::default(), &theme);

        assert!(spans.iter().any(|span| span.content.contains("pub")));
        assert!(spans.iter().all(|span| span.style.fg.is_none()));
        assert!(spans.iter().all(|span| span.style.bg.is_none()));
    }
}
