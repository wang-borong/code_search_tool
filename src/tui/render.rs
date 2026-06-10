use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use super::{query_with_cursor, AppState, SourceMode, StatusLevel, HELP_OVERLAY_TEXT, HELP_TEXT};

pub(super) fn render(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(8),
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

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let title = format!(
        " fcs TUI | workspace: {} | mode: {} ",
        app.root.display(),
        app.mode.label()
    );
    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(&app.semantic_status, Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled(&app.status, status_style(app.status_level)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_main(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Percentage(42),
            Constraint::Percentage(58),
        ])
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
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(mode.short_label(), style),
                Span::raw(" "),
                Span::styled(mode.label(), Style::default().fg(Color::DarkGray)),
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
        .map(|item| {
            ListItem::new(Line::from(Span::styled(
                item.display_text(),
                Style::default().fg(Color::LightCyan),
            )))
        })
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
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(Line::from(Span::styled(item.display_text(), style)))
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
    let title = format!("Results {selected}/{}", app.results.len());
    let items = app.results[start..end]
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let absolute_index = start + index;
            let style = if absolute_index == app.selected {
                Style::default().fg(Color::Black).bg(Color::Green)
            } else {
                Style::default()
            };
            let pin = if app.is_pinned(item) { "P " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(pin, Style::default().fg(Color::LightCyan)),
                Span::styled(item.display_text().to_string(), style),
            ]))
        })
        .collect::<Vec<ListItem>>();
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn render_preview(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let text = app.preview_for_current(area.height);
    let paragraph = Paragraph::new(text)
        .block(Block::default().title(app.preview_title()).borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn status_style(level: StatusLevel) -> Style {
    match level {
        StatusLevel::Info => Style::default().fg(Color::Yellow),
        StatusLevel::Warning => Style::default().fg(Color::LightMagenta),
        StatusLevel::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

fn render_bottom(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    let trace_lines = app
        .trace_items
        .iter()
        .take(area.height.saturating_sub(2).max(1) as usize)
        .map(|item| ListItem::new(item.display_text().to_string()))
        .collect::<Vec<ListItem>>();
    let trace = List::new(trace_lines).block(Block::default().title("Trace").borders(Borders::ALL));
    frame.render_widget(trace, chunks[0]);

    let debug = app.debug_panel_text();
    let paragraph = Paragraph::new(debug)
        .block(Block::default().title("Debug").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[1]);

    render_activity(frame, chunks[2], app);
}

fn render_activity(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let source = app
        .pending_source
        .as_ref()
        .map(|(_, mode, query)| format!("source: {} '{}'", mode.short_label(), query))
        .unwrap_or_else(|| "source: idle".to_string());
    let lsp = app
        .pending_lsp
        .as_ref()
        .map(|(_, label)| format!("lsp: {label}"))
        .unwrap_or_else(|| "lsp: idle".to_string());
    let preview = format!("preview: {}", app.preview_title());
    let counts = format!(
        "pins: {}  jumps: {}  breakpoints: {}",
        app.pinned_items.len(),
        app.navigation.len(),
        app.breakpoints.len()
    );
    let text = vec![
        Line::from(source),
        Line::from(lsp),
        Line::from(preview),
        Line::from(counts),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::default().title("Activity").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
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
    let text = Line::from(vec![
        Span::styled(format!("{prompt}: "), Style::default().fg(Color::Yellow)),
        Span::raw(query),
        Span::raw("    "),
        Span::styled(HELP_TEXT, Style::default().fg(Color::DarkGray)),
    ]);
    let paragraph = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_command_palette(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let popup = anchored_popup(area, 68, 9);
    frame.render_widget(Clear, popup);

    let matches = app.command_matches();
    let items = matches
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let style = if index == 0 {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:>2}. ", index + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(*command, style),
            ]))
        })
        .collect::<Vec<ListItem>>();
    let title = format!("Command Palette '{}'", app.command);
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, popup);
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
