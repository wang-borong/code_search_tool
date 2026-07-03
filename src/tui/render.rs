use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::config::TuiThemeConfig;
use crate::core::{CodeItem, CodeItemKind};

use super::render_model::*;
use super::{highlight, AppState, SourceMode, StatusLevel, TuiLayoutPreset, HELP_OVERLAY_TEXT};

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
    if bottom_height > 0 {
        render_bottom(frame, outer[2], app);
    }
    render_query(frame, outer[3], app);

    if app.command_active {
        render_command_palette(frame, area, app);
    }

    if app.help_visible {
        render_help_overlay(frame, area, app);
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
    let title = header_title(
        workspace,
        app.mode,
        app.layout_preset,
        &app.active_trace_session,
        app.trace_view.label(),
        area.width,
    );
    let pending = header_pending_labels(app);
    let status = header_status_for(&app.semantic_status, &app.status, &pending, area.width);
    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled(
            title,
            themed(app, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ),
        Span::raw(" "),
        Span::styled(status, status_style(app.status_level, theme(app))),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn header_pending_labels(app: &AppState) -> Vec<&'static str> {
    let mut pending = Vec::new();
    if app.pending_source.is_some() {
        pending.push("source");
    }
    if app.pending_lsp.is_some() {
        pending.push("lsp");
    }
    if app.pending_dap.is_some() {
        pending.push("dap");
    }
    if app.pending_editor_open.is_some() {
        pending.push("editor");
    }
    pending
}

fn render_main(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    if app.layout_preset == TuiLayoutPreset::Search {
        let direction = search_main_direction(area);
        let chunks = Layout::default()
            .direction(direction)
            .constraints(search_main_constraints(direction))
            .split(area);

        render_results(frame, chunks[0], app);
        render_preview(frame, chunks[1], app);
        return;
    }

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
    if matches!(
        app.layout_preset,
        TuiLayoutPreset::Debug | TuiLayoutPreset::Trace | TuiLayoutPreset::Semantic
    ) {
        render_task_sidebar(frame, area, app);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(5), Constraint::Length(6)])
        .split(area);

    render_sources(frame, chunks[0], app);
    render_pins(frame, chunks[1], app);
    render_navigation(frame, chunks[2], app);
}

fn render_task_sidebar(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(7), Constraint::Length(6)])
        .split(area);

    render_sources(frame, chunks[0], app);
    render_task_panel(frame, chunks[1], app);
    render_navigation(frame, chunks[2], app);
}

fn render_task_panel(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let lines = task_sidebar_lines(app)
        .into_iter()
        .take(area.height.saturating_sub(2).max(1) as usize)
        .map(|line| task_sidebar_line(app, &line))
        .collect::<Vec<Line<'static>>>();
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(task_sidebar_title(app.layout_preset))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn task_sidebar_line(app: &AppState, line: &str) -> Line<'static> {
    if let Some((label, value)) = line.split_once(':') {
        Line::from(vec![
            Span::styled(
                format!("{label}:"),
                themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
            ),
            Span::styled(value.to_string(), themed(app, Style::default().fg(Color::Gray))),
        ])
    } else {
        Line::from(Span::styled(
            line.to_string(),
            themed(app, Style::default().fg(Color::Gray)),
        ))
    }
}

fn task_sidebar_lines(app: &AppState) -> Vec<String> {
    task_sidebar_lines_for(TaskSidebarContext {
        layout: app.layout_preset,
        mode: app.mode,
        trace_view: app.trace_view.label(),
        trace_count: app.trace_items.len(),
        breakpoints: app.breakpoints.len(),
        pins: app.pinned_items.len(),
        dap_state: app.dap_snapshot.state.as_str(),
        dap_profile: &app.dap_snapshot.profile,
        selected_label: app.current_item().map(|item| item.display_text()),
        pending: app.pending_source.is_some(),
    })
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
            let (badge, badge_color) = source_mode_badge(app, *mode);
            ListItem::new(Line::from(vec![
                Span::styled(mode.short_label(), style),
                Span::raw(" "),
                Span::styled(mode.label(), themed(app, Style::default().fg(Color::DarkGray))),
                Span::raw(" "),
                Span::styled(
                    badge,
                    themed(app, Style::default().fg(badge_color).add_modifier(Modifier::BOLD)),
                ),
            ]))
        })
        .collect::<Vec<ListItem>>();
    let list = List::new(items).block(Block::default().title("Sources").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn source_mode_badge(app: &AppState, mode: SourceMode) -> (String, Color) {
    if source_mode_pending(app, mode) {
        return ("loading".to_string(), Color::LightYellow);
    }

    match mode {
        SourceMode::Trace => count_badge(app.trace_items.len()),
        SourceMode::Pinned => count_badge(app.pinned_items.len()),
        SourceMode::Debug => {
            let value = format!("{}p/{}b", app.debug_profiles.len(), app.breakpoints.len());
            let color = if app.debug_profiles.is_empty() && app.breakpoints.is_empty() {
                Color::DarkGray
            } else {
                Color::LightGreen
            };
            (value, color)
        }
        _ if mode == app.mode => count_badge(app.results.len()),
        _ => ("-".to_string(), Color::DarkGray),
    }
}

fn source_mode_pending(app: &AppState, mode: SourceMode) -> bool {
    app.pending_source
        .as_ref()
        .is_some_and(|(_, pending_mode, _)| *pending_mode == mode)
        || (mode == app.mode && app.pending_lsp.is_some())
}

fn count_badge(count: usize) -> (String, Color) {
    let color = if count == 0 { Color::DarkGray } else { Color::LightGreen };
    (count.to_string(), color)
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
    let result_budget = if app.results.is_empty() {
        visible_height
    } else {
        visible_height.saturating_sub(2).max(1)
    };
    let start = app.selected.saturating_sub(result_budget / 2);
    let end = (start + result_budget).min(app.results.len());
    let selected = if app.results.is_empty() {
        0
    } else {
        app.selected.saturating_add(1)
    };
    let title = result_title(app, selected, start, end, area.width);
    let items = if app.results.is_empty() {
        result_empty_items(app, visible_height)
    } else {
        result_list_items(app, start, end)
    };
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn result_title(app: &AppState, selected: usize, start: usize, end: usize, width: u16) -> String {
    let title = result_title_for(ResultTitleContext {
        mode: app.mode,
        selected,
        total: app.results.len(),
        visible_start: start,
        visible_end: end,
        pending: result_source_pending(app),
        filter: app.result_filter_label(),
        group: app.result_group.label().to_string(),
        trace_session: app.active_trace_session.clone(),
        trace_view: app.trace_view.label().to_string(),
    });
    compact_middle(&title, width.saturating_sub(2) as usize)
}

fn result_empty_items(app: &AppState, visible_height: usize) -> Vec<ListItem<'static>> {
    result_empty_messages(app)
        .into_iter()
        .take(visible_height)
        .map(|message| ListItem::new(empty_state_line(app, &message)))
        .collect()
}

fn empty_state_line(app: &AppState, message: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(message.to_string(), themed(app, Style::default().fg(Color::Gray))),
    ])
}

fn result_empty_messages(app: &AppState) -> Vec<String> {
    result_empty_messages_for(ResultEmptyContext {
        mode: app.mode,
        query: &app.query,
        pending: result_source_pending(app),
        filter: app.result_filter.as_ref().map(TuiFilterSummary::from),
        group: app.result_group.label(),
        trace_count: app.trace_items.len(),
        pinned_count: app.pinned_items.len(),
        breakpoint_count: app.breakpoints.len(),
    })
}

fn result_source_pending(app: &AppState) -> bool {
    app.pending_source
        .as_ref()
        .is_some_and(|(_, mode, _)| *mode == app.mode)
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
            items.push(ListItem::new(result_group_header(
                app,
                &group,
                result_group_count(app, &group),
            )));
        }

        let pin = if app.is_pinned(item) { "P " } else { "  " };
        items.push(ListItem::new(code_item_line(
            app,
            item,
            absolute_index == app.selected,
            Some(pin),
        )));
        if absolute_index == app.selected {
            items.push(ListItem::new(result_metadata_line(app, item)));
            items.push(ListItem::new(result_action_line(app, item)));
        }
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

fn result_group_count(app: &AppState, group: &str) -> usize {
    app.results
        .iter()
        .filter(|item| result_group_label(app.result_group, item) == group)
        .count()
}

fn result_group_header(app: &AppState, group: &str, count: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled("== ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(
            group.to_string(),
            themed(
                app,
                Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
            ),
        ),
        Span::styled(format!(" ({count})"), themed(app, Style::default().fg(Color::DarkGray))),
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
            spans.extend(path_spans(app, item.display_text()));
        }
        CodeItemKind::Symbol | CodeItemKind::TextMatch => {
            let kind = match item.kind {
                CodeItemKind::Symbol => "sym",
                CodeItemKind::TextMatch => "txt",
                CodeItemKind::File => "file",
            };
            spans.push(Span::styled(
                format!("{kind:<4}"),
                highlight::code_item_kind_style(&item.kind, theme(app)).add_modifier(Modifier::BOLD),
            ));
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

fn path_spans(app: &AppState, path: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let parts = path.split('/').collect::<Vec<&str>>();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                "/".to_string(),
                themed(app, Style::default().fg(Color::DarkGray)),
            ));
        }
        let is_leaf = index + 1 == parts.len();
        let style = if is_leaf {
            highlight::code_item_kind_style(&CodeItemKind::File, theme(app)).add_modifier(Modifier::BOLD)
        } else {
            themed(app, Style::default().fg(Color::Gray))
        };
        spans.push(Span::styled((*part).to_string(), style));
    }
    spans
}

fn result_metadata_line(app: &AppState, item: &CodeItem) -> Line<'static> {
    let kind = match item.kind {
        CodeItemKind::File => "file",
        CodeItemKind::Symbol => "symbol",
        CodeItemKind::TextMatch => "text",
    };
    let location = match (item.location.line, item.location.column) {
        (Some(line), Some(column)) => format!("{line}:{column}"),
        (Some(line), None) => line.to_string(),
        (None, _) => "-".to_string(),
    };
    let path = item.location.path.to_string_lossy().replace('\\', "/");
    Line::from(vec![
        Span::styled("   ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled("kind=", themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(kind.to_string(), themed(app, Style::default().fg(Color::LightCyan))),
        Span::styled(" loc=", themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(location, themed(app, Style::default().fg(Color::LightGreen))),
        Span::styled(" path=", themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(path, themed(app, Style::default().fg(Color::Gray))),
    ])
}

fn result_action_line(app: &AppState, item: &CodeItem) -> Line<'static> {
    let actions = result_action_hints(app, item);
    Line::from(vec![
        Span::styled("   ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled("actions=", themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(
            actions.join(" | "),
            themed(app, Style::default().fg(Color::LightYellow)),
        ),
    ])
}

fn result_action_hints(app: &AppState, item: &CodeItem) -> Vec<&'static str> {
    match app.mode {
        SourceMode::Pinned => vec!["Enter open", "u unpin", "x delete", "P preview"],
        SourceMode::Trace => vec!["Enter open", "B to breakpoints", "trace view", "P preview"],
        SourceMode::Debug => vec!["Enter open", "x delete", ":dap start/sync", "b break"],
        SourceMode::References | SourceMode::Diagnostics | SourceMode::Symbols => {
            vec!["Enter open", "p pin", "a trace", "gd/gr next"]
        }
        SourceMode::Files | SourceMode::Search => {
            if app.is_pinned(item) {
                vec!["Enter open", "u unpin", "a trace", ":filter path"]
            } else {
                vec!["Enter open", "p pin", "a trace", ":filter path"]
            }
        }
    }
}

fn trace_item_line(app: &AppState, item: &CodeItem) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "T ".to_string(),
        themed(
            app,
            Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD),
        ),
    )];

    for badge in trace_item_badges(item).into_iter().take(2) {
        spans.push(Span::styled(format!("[{badge}] "), trace_badge_style(app, &badge)));
    }

    spans.extend(code_item_line(app, item, false, None).spans);
    Line::from(spans)
}

fn trace_item_badges(item: &CodeItem) -> Vec<String> {
    let mut badges = Vec::new();
    if let Some(status) = trace_metadata_value(&item.detail, "status") {
        badges.push(compact_middle(&status, 14));
    }
    if let Some(kind) = trace_detail_kind(&item.detail) {
        if !badges.iter().any(|badge| badge == &kind) {
            badges.push(compact_middle(&kind, 18));
        }
    }
    badges
}

fn trace_metadata_value(text: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = text.find(&needle)? + needle.len();
    let value = text[start..]
        .chars()
        .take_while(|ch| !matches!(*ch, ' ' | '}' | ']'))
        .collect::<String>();
    (!value.trim().is_empty()).then_some(value)
}

fn trace_detail_kind(detail: &str) -> Option<String> {
    let (_, kind) = detail.rsplit_once('[')?;
    let kind = kind.strip_suffix(']')?.trim();
    if let Some(rest) = kind.strip_prefix("trace-timeline:") {
        return Some(format!("timeline:{rest}"));
    }
    if let Some(rest) = kind.strip_prefix("trace-graph-") {
        return Some(format!("graph:{rest}"));
    }
    (!kind.is_empty()).then_some(kind.to_string())
}

fn trace_badge_style(app: &AppState, badge: &str) -> Style {
    let color = match badge {
        "observed" | "ok" | "done" => Color::LightGreen,
        "pending" | "running" | "active" => Color::LightYellow,
        "failed" | "error" | "errored" => Color::LightRed,
        _ => Color::LightCyan,
    };
    themed(app, Style::default().fg(color).add_modifier(Modifier::BOLD))
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
    if app.current_item().is_none() {
        let paragraph = Paragraph::new(preview_empty_lines(app))
            .block(
                Block::default()
                    .title(preview_block_title(app, None, &[], area.width))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
        return;
    }

    let window = app.preview_window_for_current(area.height);
    let match_terms = preview_match_terms(app);
    let inner_width = area.width.saturating_sub(2);
    let paragraph = Paragraph::new(highlight::preview_lines_with_matches_wrapped(
        &window,
        theme(app),
        &match_terms,
        inner_width,
    ))
    .block(
        Block::default()
            .title(preview_block_title(app, Some(&window), &match_terms, area.width))
            .borders(Borders::ALL),
    );
    frame.render_widget(paragraph, area);
}

fn preview_empty_lines(app: &AppState) -> Vec<Line<'static>> {
    preview_empty_messages_for(app.mode, &app.query, result_source_pending(app))
        .into_iter()
        .map(|message| {
            Line::from(vec![
                Span::styled("  ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
                Span::styled(message, themed(app, Style::default().fg(Color::Gray))),
            ])
        })
        .collect()
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

fn preview_block_title(
    app: &AppState,
    window: Option<&super::preview_cache::PreviewWindow>,
    match_terms: &[String],
    width: u16,
) -> String {
    let mut title = app.preview_title();
    if let Some(window) = window {
        let hits = preview_match_hit_count(window, match_terms);
        if hits > 0 {
            title.push_str(&format!(" hits={hits}"));
        }
    }
    compact_middle(&title, width.saturating_sub(2) as usize)
}

fn preview_match_hit_count(window: &super::preview_cache::PreviewWindow, match_terms: &[String]) -> usize {
    let terms = normalized_preview_match_terms(match_terms);
    if terms.is_empty() {
        return 0;
    }

    window
        .lines
        .iter()
        .map(|line| preview_line_hit_count(&line.text, &terms))
        .sum()
}

fn normalized_preview_match_terms(match_terms: &[String]) -> Vec<String> {
    let mut terms = match_terms
        .iter()
        .flat_map(|term| term.split_whitespace())
        .map(|term| {
            term.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
                .to_ascii_lowercase()
        })
        .filter(|term| term.len() >= 2)
        .collect::<Vec<String>>();
    terms.sort();
    terms.dedup();
    terms
}

fn preview_line_hit_count(text: &str, terms: &[String]) -> usize {
    if !text.is_ascii() {
        return 0;
    }

    let lower = text.to_ascii_lowercase();
    let mut count = 0;
    for term in terms {
        let mut offset = 0;
        while let Some(index) = lower[offset..].find(term) {
            count += 1;
            offset += index + term.len();
        }
    }
    count
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

    let visible_trace_height = chunks[0].height.saturating_sub(2).max(1) as usize;
    let trace_lines = if app.trace_items.is_empty() {
        trace_empty_messages()
            .into_iter()
            .take(visible_trace_height)
            .map(|message| ListItem::new(empty_state_line(app, &message)))
            .collect::<Vec<ListItem>>()
    } else {
        app.trace_items
            .iter()
            .take(visible_trace_height)
            .map(|item| ListItem::new(trace_item_line(app, item)))
            .collect::<Vec<ListItem>>()
    };
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
        .constraints([Constraint::Length(8), Constraint::Min(3)])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(columns[1]);

    let snapshot = &app.dap_snapshot;
    let mut session = vec![
        highlight::debug_line(&snapshot.status, theme(app)),
        debug_overview_line(app),
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
        highlight::debug_line(&format!("next: {}", debug_next_step_text(app)), theme(app)),
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
        .block(
            Block::default()
                .title(format!("Session {}", snapshot.state.as_str()))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(session, left[0]);

    let stack = list_from_strings(
        &snapshot.stack,
        left[1].height,
        "Stack",
        debug_empty_messages(DebugEmptyPanel::Stack),
        theme(app),
    );
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
    let variables = list_from_strings(
        &variable_lines,
        right[0].height,
        "Variables",
        debug_empty_messages(DebugEmptyPanel::Variables),
        theme(app),
    );
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
    let events = list_from_strings(
        &event_lines,
        right[1].height,
        "Events",
        debug_empty_messages(DebugEmptyPanel::Events),
        theme(app),
    );
    frame.render_widget(events, right[1]);
}

fn debug_overview_line(app: &AppState) -> Line<'static> {
    let snapshot = &app.dap_snapshot;
    Line::from(vec![
        Span::styled(
            "profiles: ",
            themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        ),
        Span::styled(
            app.debug_profiles.len().to_string(),
            themed(app, Style::default().fg(Color::LightGreen)),
        ),
        Span::styled(
            " tui-bps: ",
            themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        ),
        Span::styled(
            app.breakpoints.len().to_string(),
            themed(app, Style::default().fg(Color::LightGreen)),
        ),
        Span::styled(
            " dap-bps: ",
            themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        ),
        Span::styled(
            snapshot.breakpoints.len().to_string(),
            themed(app, Style::default().fg(Color::LightGreen)),
        ),
        Span::styled(
            " watches: ",
            themed(app, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        ),
        Span::styled(
            snapshot.watches.len().to_string(),
            themed(app, Style::default().fg(Color::LightGreen)),
        ),
    ])
}

fn debug_next_step_text(app: &AppState) -> &'static str {
    match app.dap_snapshot.state {
        crate::dap::DapSessionState::Idle if app.debug_profiles.is_empty() && app.breakpoints.is_empty() => {
            "add b or create a debug profile, then run :dap start"
        }
        crate::dap::DapSessionState::Idle => "run :dap start, or :dap sync before launching",
        crate::dap::DapSessionState::Starting | crate::dap::DapSessionState::Initialized => {
            "wait for launch, then use F5/F10/F11"
        }
        crate::dap::DapSessionState::Running => "wait for a stop, or press F6 to pause",
        crate::dap::DapSessionState::Stopped => "inspect variables, :eval/:watch, F10/F11 step",
        crate::dap::DapSessionState::Terminated | crate::dap::DapSessionState::Disconnected => {
            "run :dap start to relaunch"
        }
        crate::dap::DapSessionState::Errored => "check Events, :dap adapters, then retry",
    }
}

fn list_from_strings<'a>(
    values: &[String],
    area_height: u16,
    title: &'a str,
    empty_messages: Vec<String>,
    theme: &TuiThemeConfig,
) -> List<'a> {
    let visible_height = area_height.saturating_sub(2).max(1) as usize;
    let items = if values.is_empty() {
        empty_messages
            .into_iter()
            .take(visible_height)
            .map(|message| ListItem::new(highlight::debug_line(&message, theme)))
            .collect::<Vec<ListItem>>()
    } else {
        values
            .iter()
            .take(visible_height)
            .map(|value| ListItem::new(highlight::debug_line(value, theme)))
            .collect::<Vec<ListItem>>()
    };
    List::new(items).block(Block::default().title(title).borders(Borders::ALL))
}

fn render_activity(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let pending_source = app
        .pending_source
        .as_ref()
        .map(|(_, mode, query)| activity_source_pending_label(*mode, query));
    let preview = app.preview_title();
    let health = app.health_summary();
    let ctx = ActivityContext {
        layout: app.layout_preset,
        mode: app.mode,
        pending_source: pending_source.as_deref(),
        pending_lsp: app.pending_lsp.as_ref().map(|(_, label)| *label),
        pending_dap: app.pending_dap.as_ref().map(|(_, label)| *label),
        pending_editor: app.pending_editor_open.is_some(),
        preview: &preview,
        status: &app.status,
        health: &health,
        pins: app.pinned_items.len(),
        navigation: app.navigation.len(),
        trace_session: &app.active_trace_session,
        trace_view: app.trace_view.label(),
        breakpoints: app.breakpoints.len(),
    };
    let mut text = activity_lines_for(ctx)
        .into_iter()
        .map(|line| activity_summary_line(app, &line))
        .collect::<Vec<Line<'static>>>();
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

fn activity_summary_line(app: &AppState, line: &str) -> Line<'static> {
    if let Some((label, value)) = line.split_once(':') {
        return highlight::activity_line(
            label,
            value.trim_start(),
            activity_label_color(label, app.status_level),
            theme(app),
        );
    }

    highlight::debug_line(line, theme(app))
}

fn activity_label_color(label: &str, status_level: StatusLevel) -> Color {
    match label {
        "work" => Color::LightGreen,
        "next" => Color::LightCyan,
        "preview" => Color::LightMagenta,
        "status" => status_color(status_level),
        "saved" => Color::LightBlue,
        "health" => Color::LightYellow,
        _ => Color::Gray,
    }
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
    let query_width = query_input_width(area.width, prompt, app.layout_preset, app.mode);
    let query = query_with_cursor_for_width(input, cursor, active, query_width);
    let mut spans = vec![
        Span::styled(format!("{prompt}: "), themed(app, Style::default().fg(Color::Yellow))),
        Span::raw(query),
        Span::raw("    "),
    ];
    spans.extend(source_tab_spans(app, area.width));
    spans.push(Span::raw("    "));
    spans.extend(shortcut_hint_spans(app, area.width, app.layout_preset, app.mode));
    let text = Line::from(spans);
    let paragraph = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn source_tab_spans(app: &AppState, width: u16) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, mode) in source_tab_modes(width, app.mode).iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                " ".to_string(),
                themed(app, Style::default().fg(Color::DarkGray)),
            ));
        }
        let label = mode.short_label();
        if *mode == app.mode {
            spans.push(Span::styled(
                format!("[{label}]"),
                selected_style(app, Color::Black, Color::Cyan),
            ));
        } else {
            spans.push(Span::styled(
                label.to_string(),
                themed(app, Style::default().fg(Color::DarkGray)),
            ));
        }
    }
    spans
}

fn render_command_palette(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let popup = anchored_popup(area, 78, 14);
    frame.render_widget(Clear, popup);

    let matches = app.command_matches();
    let show_descriptions = command_palette_show_descriptions(popup.width);
    let show_recent = app.command.trim().is_empty() && popup.width >= 64;
    let items = command_palette_items(app, &matches, show_descriptions, show_recent);
    let title = command_palette_title(&app.command, popup.width);
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, popup);
}

fn command_palette_items(
    app: &AppState,
    matches: &[&'static str],
    show_descriptions: bool,
    show_recent: bool,
) -> Vec<ListItem<'static>> {
    let mut items = Vec::new();
    if show_recent {
        let recent = recent_command_summary(&app.command_history, 3);
        if !recent.is_empty() {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    "recent ".to_string(),
                    themed(app, Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                ),
                Span::styled(recent, themed(app, Style::default().fg(Color::Gray))),
            ])));
        }
    }

    for (category, commands) in grouped_palette_commands(matches) {
        items.push(command_category_header(app, category));
        for (command_index, command) in commands {
            items.push(command_palette_command_item(
                app,
                command,
                command_index,
                show_descriptions,
            ));
        }
    }
    items
}

fn grouped_palette_commands(matches: &[&'static str]) -> Vec<(&'static str, Vec<(usize, &'static str)>)> {
    let mut groups = Vec::<(&'static str, Vec<(usize, &'static str)>)>::new();
    for (index, command) in matches.iter().enumerate() {
        let category = command_category(command);
        if let Some((_, commands)) = groups
            .iter_mut()
            .find(|(existing_category, _)| *existing_category == category)
        {
            commands.push((index, *command));
        } else {
            groups.push((category, vec![(index, *command)]));
        }
    }
    groups
}

fn recent_command_summary(history: &[String], limit: usize) -> String {
    let mut recent = Vec::new();
    for command in history.iter().rev() {
        let trimmed = command.trim();
        if trimmed.is_empty() || recent.iter().any(|value| value == trimmed) {
            continue;
        }
        recent.push(trimmed.to_string());
        if recent.len() >= limit {
            break;
        }
    }
    recent.join("  |  ")
}

fn command_category_header(app: &AppState, category: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(" ".to_string(), themed(app, Style::default().fg(Color::DarkGray))),
        Span::styled(
            command_category_label(category).to_string(),
            themed(
                app,
                Style::default()
                    .fg(command_category_color(category))
                    .add_modifier(Modifier::BOLD),
            ),
        ),
    ]))
}

fn command_palette_command_item(
    app: &AppState,
    command: &'static str,
    index: usize,
    show_descriptions: bool,
) -> ListItem<'static> {
    let style = if index == 0 {
        selected_style(app, Color::Black, Color::Cyan)
    } else {
        Style::default()
    };
    let mut spans = vec![
        Span::styled(
            format!("{:>2}. ", index + 1),
            themed(app, Style::default().fg(Color::DarkGray)),
        ),
        Span::styled(command.to_string(), style),
    ];
    if show_descriptions {
        spans.push(Span::styled(
            format!("  {}", command_description(command)),
            themed(app, Style::default().fg(Color::DarkGray)),
        ));
    }
    ListItem::new(Line::from(spans))
}

fn shortcut_hint_spans(app: &AppState, width: u16, layout: TuiLayoutPreset, mode: SourceMode) -> Vec<Span<'static>> {
    let hints = shortcut_hints_for_context(width, layout, mode);
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

fn command_category_label(category: &str) -> &'static str {
    match category {
        "debug" => "Debug",
        "trace" => "Trace",
        "view" => "View",
        "semantic" => "Semantic",
        "nav" => "Navigate",
        _ => "Core",
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

fn render_help_overlay(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let popup = centered_rect(area, 78, 72);
    frame.render_widget(Clear, popup);
    let paragraph = Paragraph::new(help_text(app))
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn help_text(app: &AppState) -> String {
    let common = "\
Common
  /                 live query
  enter or o        open selected location
  tab / shift-tab   change source
  j/k or arrows     move selection
  :                 command palette
  q / esc           close or quit
";

    let body = match app.layout_preset {
        TuiLayoutPreset::Search => {
            "\
Find
  source files/search/symbols  switch result source
  gd / gr / gt / gi            definition / refs / type / impl
  p / u                        pin / unpin
  a                            bookmark selected location
  layout trace/debug/balanced   show advanced panels
"
        }
        TuiLayoutPreset::Debug => {
            "\
Debug
  D / X                 debug source / run profile
  F5/F6/F10/F11         continue / pause / next / step-in
  shift-F11 / ctrl-F5   step-out / stop
  break sync            sync TUI breakpoints to DAP
  watch add <expr>      add watch expression
"
        }
        TuiLayoutPreset::Trace => {
            "\
Trace
  trace session <name>       switch trace session
  trace view session/timeline/graph
  trace semantic refs/def/incoming/outgoing
  B                          add trace locations as breakpoints
"
        }
        TuiLayoutPreset::Semantic => {
            "\
Semantic
  gd / gr / gt / gi      definition / refs / type / impl
  W / s / e              workspace symbols / document symbols / diagnostics
  c / C / h              incoming / outgoing / hover
  trace semantic <kind>  record semantic edges
"
        }
        TuiLayoutPreset::Balanced => HELP_OVERLAY_TEXT,
    };

    if app.layout_preset == TuiLayoutPreset::Balanced {
        body.to_string()
    } else {
        format!("fcs workbench\n\n{common}\n{body}")
    }
}

fn command_description(command: &str) -> &'static str {
    match command.split_whitespace().next().unwrap_or(command) {
        "source" | "files" | "search" | "symbols" | "debug" => "switch source or view",
        "query" => "set query text",
        "layout" => "change panel layout",
        "filter" => "narrow current results",
        "group" => "group current results",
        "preview" | "preview-lock" | "preview-up" | "preview-down" => "control preview",
        "def" | "refs" | "type" | "impl" | "diag" | "incoming" | "outgoing" | "hover" => "semantic navigation",
        "trace" => "record or inspect trace",
        "dap" | "dap-smoke" | "dap-sync" | "watch" | "var" | "eval" | "break" => "debug action",
        "pin" | "unpin" | "pins" => "manage pinned locations",
        "open" => "open selected location",
        "refresh" => "refresh current source",
        "quit" => "exit TUI",
        _ => "run action",
    }
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::config::Config;
    use crate::core::CodeItem;

    use super::*;

    fn temp_workspace(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is valid")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("fcs-render-{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).expect("create render smoke workspace");
        fs::write(root.join("main.rs"), "fn main() {}\n").expect("write render smoke source");
        root
    }

    fn render_to_text(app: &AppState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("create test terminal");
        terminal.draw(|frame| render(frame, app)).expect("render TUI");
        terminal.backend().to_string()
    }

    #[test]
    fn render_smoke_covers_default_search_layout() {
        let root = temp_workspace("search");
        let app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            None,
            None,
            None,
        )
        .expect("create app state");

        let screen = render_to_text(&app, 100, 32);

        assert!(screen.contains("fcs"));
        assert!(screen.contains("Results files"));
        assert!(screen.contains("Preview"));
        assert!(screen.contains("QUERY"));
        assert!(screen.contains("[files]"));
    }

    #[test]
    fn render_smoke_covers_debug_layout_panels() {
        let root = temp_workspace("debug");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            Some(SourceMode::Debug),
            None,
            None,
        )
        .expect("create app state");
        app.layout_preset = TuiLayoutPreset::Debug;
        app.refresh().expect("refresh debug source");

        let screen = render_to_text(&app, 104, 34);

        assert!(screen.contains("Debug Task"));
        assert!(screen.contains("Session idle"));
        assert!(screen.contains("profiles:"));
        assert!(screen.contains("next:"));
        assert!(screen.contains("Stack"));
        assert!(screen.contains("Variables"));
        assert!(screen.contains("Activity"));
    }

    #[test]
    fn render_smoke_covers_result_metadata() {
        let root = temp_workspace("metadata");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            Some(SourceMode::Files),
            None,
            None,
        )
        .expect("create app state");
        app.results = vec![CodeItem::file_with_display(root.join("main.rs"), "main.rs")];
        app.selected = 0;
        app.pending_source = None;

        let screen = render_to_text(&app, 100, 32);

        assert!(screen.contains("kind=file"));
        assert!(screen.contains("loc=1"));
        assert!(screen.contains("path="));
        assert!(screen.contains("actions="));
        assert!(screen.contains("p pin"));
    }

    #[test]
    fn render_smoke_covers_source_badges_in_sidebar() {
        let root = temp_workspace("source-badges");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            Some(SourceMode::Files),
            None,
            None,
        )
        .expect("create app state");
        app.layout_preset = TuiLayoutPreset::Balanced;
        let item = CodeItem::file_with_display(root.join("main.rs"), "main.rs");
        app.results = vec![item.clone()];
        app.pinned_items = vec![item.clone()];
        app.trace_items = vec![item.clone()];
        app.breakpoints
            .push(crate::dap::DapBreakpoint::from_location(&item.location));
        app.selected = 0;
        app.pending_source = None;

        let screen = render_to_text(&app, 120, 36);

        assert!(screen.contains("Diagnostics"));
        assert!(screen.contains("debug"));
        assert!(screen.contains("0p/1b"));
        assert!(screen.contains("actions="));
    }

    #[test]
    fn render_smoke_covers_preview_target_and_hits() {
        let root = temp_workspace("preview");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            Some(SourceMode::Files),
            Some("main".to_string()),
            None,
        )
        .expect("create app state");
        app.results = vec![CodeItem::file_with_display(root.join("main.rs"), "main.rs")];
        app.selected = 0;
        app.pending_source = None;

        let screen = render_to_text(&app, 112, 34);

        assert!(screen.contains("main.rs:1"));
        assert!(screen.contains("hits=1"));
    }

    #[test]
    fn render_smoke_wraps_preview_with_continuation_gutter() {
        let root = temp_workspace("preview-wrap");
        fs::write(
            root.join("main.rs"),
            "fn main() { let long_name = \"alpha beta gamma delta epsilon zeta eta theta\"; }\n",
        )
        .expect("write long preview source");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            Some(SourceMode::Files),
            Some("alpha".to_string()),
            None,
        )
        .expect("create app state");
        app.results = vec![CodeItem::file_with_display(root.join("main.rs"), "main.rs")];
        app.selected = 0;
        app.pending_source = None;

        let screen = render_to_text(&app, 76, 28);

        assert!(screen.contains(".. |"));
        assert!(!screen.contains("\ngamma delta"));
    }

    #[test]
    fn render_smoke_covers_trace_badges() {
        let root = temp_workspace("trace-badges");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            Some(SourceMode::Trace),
            None,
            None,
        )
        .expect("create app state");
        app.layout_preset = TuiLayoutPreset::Trace;
        app.trace_items = vec![CodeItem::symbol(
            root.join("src/main.rs"),
            "src/main.rs",
            1,
            None,
            "breakpoint observed {status=observed}",
            "debug-stop",
        )];

        let screen = render_to_text(&app, 120, 36);

        assert!(screen.contains("[observed]"));
        assert!(screen.contains("[debug-stop]"));
    }

    #[test]
    fn render_smoke_covers_grouped_command_palette() {
        let root = temp_workspace("palette");
        let mut app = AppState::new(
            Config::default(),
            Some(root.to_string_lossy().to_string()),
            None,
            None,
            None,
        )
        .expect("create app state");
        app.command_active = true;
        app.command_history = vec!["trace semantic refs".to_string(), "layout debug".to_string()];

        let screen = render_to_text(&app, 112, 36);

        assert!(screen.contains("recent"));
        assert!(screen.contains("trace semantic refs"));
        assert!(screen.contains("Navigate"));
        assert!(screen.contains("View"));
    }

    #[test]
    fn command_palette_recent_summary_deduplicates_history() {
        let history = vec![
            "layout debug".to_string(),
            "trace semantic refs".to_string(),
            "layout debug".to_string(),
            "dap start".to_string(),
        ];

        assert_eq!(
            recent_command_summary(&history, 3),
            "dap start  |  layout debug  |  trace semantic refs"
        );
    }

    #[test]
    fn command_palette_grouping_preserves_match_indices() {
        let groups = grouped_palette_commands(&["dap start", "trace semantic refs", "layout debug", "pin"]);

        assert_eq!(groups[0].0, "debug");
        assert_eq!(groups[0].1[0], (0, "dap start"));
        assert_eq!(groups[1].0, "trace");
        assert_eq!(groups[2].0, "view");
        assert_eq!(groups[3].0, "nav");
    }
}
