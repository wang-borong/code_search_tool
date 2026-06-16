use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::config::TuiThemeConfig;
use crate::core::{CodeItem, CodeItemKind};

use super::{highlight, query_with_cursor, AppState, SourceMode, StatusLevel, TuiLayoutPreset, HELP_OVERLAY_TEXT};

pub(super) fn render(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    let bottom_height = bottom_panel_height(app.layout_preset);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(bottom_height),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(frame, outer[0], app);
    render_main(frame, outer[1], app);
    render_bottom(frame, outer[2], app);
    render_query(frame, outer[3], app);

    if app.command_active {
        render_command_palette(frame, area, app);
    }

    if app.help_visible {
        render_help_overlay(frame, area);
    }
}

fn theme(app: &AppState) -> &TuiThemeConfig {
    &app.config.tui.theme
}

fn themed(app: &AppState, style: Style) -> Style {
    highlight::theme_style(theme(app), style)
}

fn selected_style(app: &AppState, fg: Color, bg: Color) -> Style {
    highlight::selection_style(theme(app), fg, bg)
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let workspace = app
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| app.root.to_str().unwrap_or("workspace"));
    let title = format!(
        " fcs | {} | {} | {} | trace {}:{} ",
        workspace,
        app.mode.label(),
        app.layout_preset.label(),
        app.active_trace_session,
        app.trace_view.label()
    );
    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled(
            title,
            themed(app, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ),
        Span::raw(" "),
        Span::styled(&app.semantic_status, themed(app, Style::default().fg(Color::Green))),
        Span::raw(" | "),
        Span::styled(&app.status, status_style(app.status_level, theme(app))),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_main(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let constraints = main_constraints(app.layout_preset);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    render_sidebar(frame, chunks[0], app);
    render_results(frame, chunks[1], app);
    render_preview(frame, chunks[2], app);
}

fn render_sidebar(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(5), Constraint::Length(6)])
        .split(area);

    render_sources(frame, chunks[0], app);
    render_pins(frame, chunks[1], app);
    render_navigation(frame, chunks[2], app);
}

fn render_sources(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let items = SourceMode::all()
        .iter()
        .map(|mode| {
            let style = if *mode == app.mode {
                selected_style(app, Color::Black, Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(mode.short_label(), style),
                Span::raw(" "),
                Span::styled(mode.label(), themed(app, Style::default().fg(Color::DarkGray))),
            ]))
        })
        .collect::<Vec<ListItem>>();
    let list = List::new(items).block(Block::default().title("Sources").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_pins(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let visible_height = area.height.saturating_sub(2).max(1) as usize;
    let items = app
        .pinned_items
        .iter()
        .take(visible_height)
        .map(|item| ListItem::new(code_item_line(app, item, false, Some("P "))))
        .collect::<Vec<ListItem>>();
    let title = format!("Pins {}", app.pinned_items.len());
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_navigation(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let visible_height = area.height.saturating_sub(2).max(1) as usize;
    let start = app
        .navigation_index
        .unwrap_or(0)
        .saturating_sub(visible_height.saturating_sub(1));
    let end = (start + visible_height).min(app.navigation.len());
    let items = app.navigation[start..end]
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let absolute_index = start + index;
            let style = if Some(absolute_index) == app.navigation_index {
                selected_style(app, Color::Black, Color::Yellow)
            } else {
                themed(app, Style::default().fg(Color::DarkGray))
            };
            ListItem::new(apply_line_style(code_item_line(app, item, false, None), style))
        })
        .collect::<Vec<ListItem>>();
    let title = match app.navigation_index {
        Some(index) => format!("Jumps {}/{}", index + 1, app.navigation.len()),
        None => "Jumps 0/0".to_string(),
    };
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_results(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let visible_height = area.height.saturating_sub(2).max(1) as usize;
    let start = app.selected.saturating_sub(visible_height / 2);
    let end = (start + visible_height).min(app.results.len());
    let selected = if app.results.is_empty() {
        0
    } else {
        app.selected.saturating_add(1)
    };
    let suffix = result_projection_suffix(app);
    let title = if app.mode == SourceMode::Trace {
        format!(
            "Results {selected}/{} trace:{}:{}{}",
            app.results.len(),
            app.active_trace_session,
            app.trace_view.label(),
            suffix
        )
    } else {
        format!("Results {selected}/{}{}", app.results.len(), suffix)
    };
    let items = result_list_items(app, start, end);
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn result_list_items(app: &AppState, start: usize, end: usize) -> Vec<ListItem<'static>> {
    let mut items = Vec::new();
    let mut current_group = start
        .checked_sub(1)
        .and_then(|index| app.results.get(index))
        .map(|item| result_group_label(app.result_group, item));

    for (index, item) in app.results[start..end].iter().enumerate() {
        let absolute_index = start + index;
        let group = result_group_label(app.result_group, item);
        if app.result_group != super::TuiResultGroup::None && current_group.as_deref() != Some(group.as_str()) {
            current_group = Some(group.clone());
            items.push(ListItem::new(result_group_header(app, &group)));
        }

        let pin = if app.is_pinned(item) { "P " } else { "  " };
        items.push(ListItem::new(code_item_line(
            app,
            item,
            absolute_index == app.selected,
            Some(pin),
        )));
    }
    items
}

fn result_group_label(group: super::TuiResultGroup, item: &CodeItem) -> String {
    match group {
        super::TuiResultGroup::None => String::new(),
        super::TuiResultGroup::Kind => match item.kind {
            CodeItemKind::File => "file".to_string(),
            CodeItemKind::Symbol => item
                .detail
                .rsplit_once('[')
                .and_then(|(_, rest)| rest.strip_suffix(']'))
                .unwrap_or("symbol")
                .to_string(),
            CodeItemKind::TextMatch => "text-match".to_string(),
        },
        super::TuiResultGroup::Path => item.location.path.to_string_lossy().replace('\\', "/"),
    }
}

fn result_group_header(app: &AppState, group: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("== ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(
            group.to_string(),
            themed(
                app,
                Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
            ),
        ),
        Span::styled(" ==".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
    ])
}

fn code_item_line(app: &AppState, item: &CodeItem, selected: bool, prefix: Option<&str>) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(prefix) = prefix {
        spans.push(Span::styled(
            prefix.to_string(),
            themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        ));
    }

    match item.kind {
        CodeItemKind::File => {
            spans.push(Span::styled(
                item.display_text().to_string(),
                highlight::code_item_kind_style(&item.kind, theme(app)),
            ));
        }
        CodeItemKind::Symbol | CodeItemKind::TextMatch => {
            spans.push(Span::styled(
                item.label.clone(),
                themed(app, Style::default().fg(Color::LightBlue)),
            ));
            if let Some(line) = item.location.line {
                spans.push(Span::styled(
                    ":".to_string(),
                    themed(app, Style::default().fg(Color::DarkGray)),
                ));
                spans.push(Span::styled(
                    line.to_string(),
                    themed(app, Style::default().fg(Color::LightGreen)),
                ));
            }
            spans.push(Span::styled(
                ":".to_string(),
                themed(app, Style::default().fg(Color::DarkGray)),
            ));
            let detail_style = match item.kind {
                CodeItemKind::Symbol => themed(app, Style::default().fg(Color::LightYellow)),
                CodeItemKind::TextMatch => Style::default(),
                CodeItemKind::File => Style::default(),
            };
            spans.extend(highlight::highlight_code(
                &item.location.path,
                &item.detail,
                detail_style,
                theme(app),
            ));
        }
    }

    let line = Line::from(spans);
    if selected {
        apply_line_style(line, selected_style(app, Color::Black, Color::Green))
    } else {
        line
    }
}

fn apply_line_style(line: Line<'static>, style: Style) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style.patch(style)))
            .collect::<Vec<Span<'static>>>(),
    )
}

fn render_preview(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let window = app.preview_window_for_current(area.height);
    let paragraph = Paragraph::new(highlight::preview_lines_with_matches(
        &window,
        theme(app),
        &preview_match_terms(app),
    ))
    .block(Block::default().title(app.preview_title()).borders(Borders::ALL))
    .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn preview_match_terms(app: &AppState) -> Vec<String> {
    let mut terms = Vec::new();
    if !app.query.trim().is_empty() {
        terms.push(app.query.clone());
    }
    if let Some(item) = app.current_item() {
        if !item.label.trim().is_empty() {
            terms.push(item.label.clone());
        }
        if matches!(item.kind, CodeItemKind::Symbol) {
            terms.push(item.detail.clone());
        }
    }
    terms
}

fn status_style(level: StatusLevel, theme: &TuiThemeConfig) -> Style {
    match level {
        StatusLevel::Info => highlight::theme_style(theme, Style::default().fg(Color::Yellow)),
        StatusLevel::Warning => highlight::theme_style(theme, Style::default().fg(Color::LightMagenta)),
        StatusLevel::Error => {
            highlight::theme_style(theme, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        }
    }
}

fn render_bottom(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let constraints = bottom_constraints(app.layout_preset);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let trace_lines = app
        .trace_items
        .iter()
        .take(area.height.saturating_sub(2).max(1) as usize)
        .map(|item| ListItem::new(code_item_line(app, item, false, None)))
        .collect::<Vec<ListItem>>();
    let trace = List::new(trace_lines).block(
        Block::default()
            .title(format!(
                "Trace {}:{} {}",
                app.active_trace_session,
                app.trace_view.label(),
                app.trace_items.len()
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(trace, chunks[0]);

    render_debug_panel(frame, chunks[1], app);

    render_activity(frame, chunks[2], app);
}

fn render_debug_panel(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    if area.width < 48 || area.height < 7 {
        let lines = app
            .debug_panel_text()
            .lines()
            .map(|line| highlight::debug_line(line, theme(app)))
            .collect::<Vec<Line<'static>>>();
        let paragraph = Paragraph::new(lines)
            .block(Block::default().title("Debug").borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(3)])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(columns[1]);

    let snapshot = &app.dap_snapshot;
    let mut session = vec![
        highlight::debug_line(&snapshot.status, theme(app)),
        Line::from(vec![
            Span::styled(
                "adapter: ",
                themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
            ),
            Span::styled(snapshot.adapter.clone(), themed(app, Style::default().fg(Color::Gray))),
            Span::styled(
                " profile: ",
                themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
            ),
            Span::styled(snapshot.profile.clone(), themed(app, Style::default().fg(Color::Gray))),
        ]),
        highlight::debug_line(
            &format!(
                "req/res {}:{}  thread {:?} frame {:?}",
                snapshot.request_count,
                snapshot.response_count,
                snapshot.selected_thread_id,
                snapshot.selected_frame_id
            ),
            theme(app),
        ),
    ];
    if let Some(reason) = &snapshot.stop_reason {
        session.push(highlight::debug_line(&format!("stop: {reason}"), theme(app)));
    }
    if !snapshot.capabilities.is_empty() {
        session.push(highlight::debug_line(
            &format!(
                "caps: {}",
                snapshot
                    .capabilities
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            theme(app),
        ));
    }
    let session = Paragraph::new(session)
        .block(Block::default().title("Session").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(session, left[0]);

    let stack = list_from_strings(&snapshot.stack, left[1].height, "Stack", theme(app));
    frame.render_widget(stack, left[1]);

    let mut variable_lines = snapshot.variables.clone();
    if !snapshot.watches.is_empty() {
        variable_lines.push("-- watches --".to_string());
        variable_lines.extend(snapshot.watches.clone());
    }
    if let Some(evaluation) = &snapshot.last_evaluation {
        variable_lines.push(format!("eval: {evaluation}"));
    }
    if let Some(error) = &snapshot.error {
        variable_lines.push(format!("error: {error}"));
    }
    let variables = list_from_strings(&variable_lines, right[0].height, "Variables", theme(app));
    frame.render_widget(variables, right[0]);

    let mut event_lines = snapshot.events.clone();
    if !snapshot.breakpoints.is_empty() {
        event_lines.push("-- breakpoints --".to_string());
        event_lines.extend(snapshot.breakpoints.clone());
    }
    if let Some(location) = &snapshot.stopped_location {
        let column = location.column.map(|column| format!(":{column}")).unwrap_or_default();
        event_lines.push(format!(
            "stopped {}:{}{}",
            location.path.display(),
            location.line,
            column
        ));
    }
    let events = list_from_strings(&event_lines, right[1].height, "Events", theme(app));
    frame.render_widget(events, right[1]);
}

fn list_from_strings<'a>(values: &[String], area_height: u16, title: &'a str, theme: &TuiThemeConfig) -> List<'a> {
    let visible_height = area_height.saturating_sub(2).max(1) as usize;
    let items = values
        .iter()
        .take(visible_height)
        .map(|value| ListItem::new(highlight::debug_line(value, theme)))
        .collect::<Vec<ListItem>>();
    List::new(items).block(Block::default().title(title).borders(Borders::ALL))
}

fn render_activity(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let source = app
        .pending_source
        .as_ref()
        .map(|(_, mode, query)| format!("{} '{}'", mode.short_label(), query))
        .unwrap_or_else(|| "idle".to_string());
    let lsp = app
        .pending_lsp
        .as_ref()
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| "idle".to_string());
    let dap = app
        .pending_dap
        .as_ref()
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| "idle".to_string());
    let preview = app.preview_title();
    let counts = format!(
        "pins: {}  jumps: {}  trace: {}:{}  breakpoints: {}",
        app.pinned_items.len(),
        app.navigation.len(),
        app.active_trace_session,
        app.trace_view.label(),
        app.breakpoints.len()
    );
    let mut text = vec![
        highlight::activity_line("source", &source, Color::LightGreen, theme(app)),
        highlight::activity_line("lsp", &lsp, Color::LightBlue, theme(app)),
        highlight::activity_line("dap", &dap, Color::LightMagenta, theme(app)),
        highlight::activity_line("preview", &preview, Color::LightCyan, theme(app)),
        highlight::activity_line("status", &app.status, status_color(app.status_level), theme(app)),
        highlight::activity_line("health", &app.health_summary(), Color::LightYellow, theme(app)),
        Line::from(vec![
            Span::styled(
                "counts: ",
                themed(app, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ),
            Span::styled(counts, themed(app, Style::default().fg(Color::Gray))),
        ]),
    ];
    if let Some(plan) = &app.startup_plan {
        let available = area.height.saturating_sub(2).saturating_sub(text.len() as u16) as usize;
        for line in crate::workspace::startup_plan_lines(plan).into_iter().take(available) {
            text.push(highlight::debug_line(&line, theme(app)));
        }
    }
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Activity").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn bottom_panel_height(preset: TuiLayoutPreset) -> u16 {
    match preset {
        TuiLayoutPreset::Balanced | TuiLayoutPreset::Search | TuiLayoutPreset::Semantic => 8,
        TuiLayoutPreset::Debug => 12,
        TuiLayoutPreset::Trace => 10,
    }
}

fn main_constraints(preset: TuiLayoutPreset) -> [Constraint; 3] {
    match preset {
        TuiLayoutPreset::Balanced => [
            Constraint::Length(28),
            Constraint::Percentage(42),
            Constraint::Percentage(58),
        ],
        TuiLayoutPreset::Search => [
            Constraint::Length(24),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ],
        TuiLayoutPreset::Debug => [
            Constraint::Length(22),
            Constraint::Percentage(34),
            Constraint::Percentage(66),
        ],
        TuiLayoutPreset::Trace => [
            Constraint::Length(26),
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ],
        TuiLayoutPreset::Semantic => [
            Constraint::Length(24),
            Constraint::Percentage(46),
            Constraint::Percentage(54),
        ],
    }
}

fn bottom_constraints(preset: TuiLayoutPreset) -> [Constraint; 3] {
    match preset {
        TuiLayoutPreset::Balanced | TuiLayoutPreset::Search | TuiLayoutPreset::Semantic => [
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ],
        TuiLayoutPreset::Debug => [
            Constraint::Percentage(24),
            Constraint::Percentage(52),
            Constraint::Percentage(24),
        ],
        TuiLayoutPreset::Trace => [
            Constraint::Percentage(48),
            Constraint::Percentage(28),
            Constraint::Percentage(24),
        ],
    }
}

fn result_projection_suffix(app: &AppState) -> String {
    let filter = app.result_filter_label();
    let group = app.result_group.label();
    if filter == "none" && group == "none" {
        return String::new();
    }
    format!(" filter={filter} group={group}")
}

fn render_query(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let (prompt, input, active) = if app.command_active {
        ("CMD*", app.command.as_str(), true)
    } else if app.input_active {
        ("QUERY*", app.query.as_str(), true)
    } else {
        ("QUERY", app.query.as_str(), false)
    };
    let cursor = if app.command_active {
        app.command_cursor
    } else {
        app.query_cursor
    };
    let query = query_with_cursor(input, cursor, active);
    let mut spans = vec![
        Span::styled(format!("{prompt}: "), themed(app, Style::default().fg(Color::Yellow))),
        Span::raw(query),
        Span::raw("    "),
    ];
    spans.extend(shortcut_hint_spans(app));
    let text = Line::from(spans);
    let paragraph = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_command_palette(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let popup = anchored_popup(area, 76, 11);
    frame.render_widget(Clear, popup);

    let matches = app.command_matches();
    let items = matches
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let style = if index == 0 {
                selected_style(app, Color::Black, Color::Cyan)
            } else {
                Style::default()
            };
            let category = command_category(command);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>2}. ", index + 1),
                    themed(app, Style::default().fg(Color::DarkGray)),
                ),
                Span::styled(
                    format!("{category:<8} "),
                    themed(app, Style::default().fg(command_category_color(category))),
                ),
                Span::styled(*command, style),
            ]))
        })
        .collect::<Vec<ListItem>>();
    let title = format!("Command Palette '{}'", app.command);
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, popup);
}

fn shortcut_hint_spans(app: &AppState) -> Vec<Span<'static>> {
    let hints = [
        ("?", "help"),
        ("/", "query"),
        (":", "cmd"),
        ("Tab", "source"),
        ("gd/gr", "nav"),
        ("F5/F10/F11", "dap"),
        ("P", "preview"),
    ];
    let mut spans = Vec::new();
    for (index, (key, label)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                "  ".to_string(),
                themed(app, Style::default().fg(Color::DarkGray)),
            ));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        ));
        spans.push(Span::styled(
            format!(" {label}"),
            themed(app, Style::default().fg(Color::DarkGray)),
        ));
    }
    spans
}

fn status_color(level: StatusLevel) -> Color {
    match level {
        StatusLevel::Info => Color::LightGreen,
        StatusLevel::Warning => Color::LightYellow,
        StatusLevel::Error => Color::LightRed,
    }
}

fn command_category(command: &str) -> &'static str {
    match command.split_whitespace().next().unwrap_or(command) {
        "dap" | "dap-smoke" | "dap-sync" | "watch" | "eval" | "var" | "break" => "debug",
        "trace" => "trace",
        "layout" | "filter" | "group" | "preview" | "source" | "query" => "view",
        "def" | "refs" | "type" | "impl" | "symbols" | "diag" | "incoming" | "outgoing" | "hover" => "semantic",
        "pin" | "unpin" | "pins" | "back" | "forward" | "cycle" | "open" | "refresh" | "delete" => "nav",
        _ => "core",
    }
}

fn command_category_color(category: &str) -> Color {
    match category {
        "debug" => Color::LightMagenta,
        "trace" => Color::LightYellow,
        "view" => Color::LightCyan,
        "semantic" => Color::LightBlue,
        "nav" => Color::LightGreen,
        _ => Color::Gray,
    }
}

fn render_help_overlay(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let popup = centered_rect(area, 78, 72);
    frame.render_widget(Clear, popup);
    let paragraph = Paragraph::new(HELP_OVERLAY_TEXT)
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn anchored_popup(area: Rect, percent_x: u16, height: u16) -> Rect {
    let width = area.width.saturating_mul(percent_x).saturating_div(100).max(40);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height.saturating_add(4));
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
