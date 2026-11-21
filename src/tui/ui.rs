use crate::core::model::Signifier;
use crate::tui::app::{App, FocusPane};
use crate::tui::theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    // Base warm dark surface background (#15130E)
    f.render_widget(
        Block::default().style(Style::default().bg(theme::SURFACE)),
        area,
    );

    // Outer vertical layout: Top Bar (3), Main 3-Column Grid (Min), Bottom Keybinding Bar (3)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);

    render_top_bar(f, app, outer[0]);
    render_body_grid(f, app, outer[1]);
    render_bottom_bar(f, app, outer[2]);

    if app.show_help {
        render_help_modal(f, area);
    }
}

fn pane_block<'a>(title_left: &'a str, title_right: Option<&'a str>, bg: ratatui::style::Color, focused: bool) -> Block<'a> {
    let border_color = if focused {
        theme::PRIMARY
    } else {
        theme::OUTLINE_VARIANT
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg))
        .title(
            Line::from(Span::styled(
                format!(" {title_left} "),
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Left),
        );

    if let Some(right) = title_right {
        block = block.title(
            Line::from(Span::styled(
                format!(" {right} "),
                Style::default().fg(theme::ON_SURFACE_VARIANT),
            ))
            .alignment(Alignment::Right),
        );
    }
    block
}

fn render_top_bar(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::OUTLINE_VARIANT))
        .style(Style::default().bg(theme::SURFACE_LOW));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(30), Constraint::Length(30)])
        .split(inner);

    let doc_path = app.vault.display_doc_path(&app.doc.rel_path);
    let left = Line::from(vec![
        Span::styled(
            " folio --tui v1.4",
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled(doc_path, Style::default().fg(theme::ON_SURFACE_VARIANT)),
    ]);
    f.render_widget(Paragraph::new(left), cols[0]);

    let page_label = app
        .doc
        .page
        .map(|p| format!("Notebook p. {p}"))
        .unwrap_or_else(|| "Notebook p. —".to_string());
    let sync_badge = if app.dirty {
        "[DIRTY .md]"
    } else {
        "[SYNCED .md]"
    };
    let sync_color = if app.dirty {
        theme::SECONDARY
    } else {
        theme::PRIMARY
    };
    let right = Line::from(vec![
        Span::styled(page_label, Style::default().fg(theme::TERTIARY)),
        Span::raw("  "),
        Span::styled(
            sync_badge,
            Style::default()
                .fg(sync_color)
                .bg(theme::SURFACE_CONTAINER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])
    .alignment(Alignment::Right);
    f.render_widget(Paragraph::new(right), cols[1]);
}

fn render_body_grid(f: &mut Frame, app: &App, area: Rect) {
    // 3-Pane TUI Grid matching [248px | 1fr | 272px]
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(44),
            Constraint::Length(32),
        ])
        .split(area);

    // Left column: [1] SPACES + [2] VAULT
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((app.spaces.len() as u16 + 3).clamp(6, 12)),
            Constraint::Min(8),
        ])
        .split(cols[0]);

    render_spaces_pane(f, app, left_rows[0]);
    render_vault_pane(f, app, left_rows[1]);

    // Center column: [3] RAPID LOG + Live Capture Prompt
    render_rapid_log_pane(f, app, cols[1]);

    // Right column: [4] SIGNIFIERS + [5] CARRY TO PAPER + [6] .MD BUFFER
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Min(8),
        ])
        .split(cols[2]);

    render_signifiers_pane(f, right_rows[0]);
    render_carry_to_paper_pane(f, app, right_rows[1]);
    render_md_buffer_pane(f, app, right_rows[2]);
}

fn render_spaces_pane(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(
        "[1] SPACES",
        Some("Tab"),
        theme::SURFACE_LOW,
        app.focus == FocusPane::Spaces,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let width = inner.width as usize;
    let items: Vec<ListItem> = app
        .spaces
        .iter()
        .enumerate()
        .map(|(idx, s)| {
            let is_active = s.rel_path == app.doc.rel_path || idx == app.active_space_idx;
            let prefix = if is_active { "> " } else { "  " };
            let count_str = s.entry_count.to_string();
            let max_name = width.saturating_sub(prefix.len() + count_str.len() + 2);
            let name = truncate_str(&s.name, max_name);
            let pad = width
                .saturating_sub(prefix.len() + name.chars().count() + count_str.len() + 1);

            let style = if is_active {
                Style::default()
                    .fg(theme::PRIMARY)
                    .bg(theme::SURFACE_HIGH)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::ON_SURFACE)
            };
            let count_style = if is_active {
                style
            } else {
                Style::default().fg(theme::ON_SURFACE_VARIANT)
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{prefix}{name}"), style),
                Span::styled(" ".repeat(pad.max(1)), style),
                Span::styled(format!("{count_str} "), count_style),
            ]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn render_vault_pane(f: &mut Frame, app: &App, area: Rect) {
    let right_title = format!("~/{}", app.vault_tree.vault_name);
    let block = pane_block(
        "[2] VAULT",
        Some(&right_title),
        theme::SURFACE_LOW,
        app.focus == FocusPane::Vault,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = app
        .vault_tree
        .nodes
        .iter()
        .enumerate()
        .map(|(idx, node)| {
            let is_current_doc = !node.is_dir && node.rel_path == app.doc.rel_path;
            let is_cursor = app.focus == FocusPane::Vault && idx == app.selected_vault_idx;
            if node.is_dir {
                ListItem::new(Line::from(Span::styled(
                    format!(" ▾ {}", node.label),
                    Style::default().fg(theme::ON_SURFACE_VARIANT),
                )))
            } else {
                let indent = if node.depth > 0 { "   " } else { " " };
                let marker = if is_current_doc { "* " } else { "  " };
                let style = if is_current_doc || is_cursor {
                    Style::default()
                        .fg(theme::PRIMARY)
                        .bg(theme::SURFACE_HIGH)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::ON_SURFACE)
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{indent}{marker}{}", node.label),
                    style,
                )))
            }
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn render_rapid_log_pane(f: &mut Frame, app: &App, area: Rect) {
    let filtered = app.filtered_entries();
    let title_left = format!("[3] RAPID LOG — {}", app.doc.title);
    let title_right = format!("Filter: {} ({})", app.filter.label(), filtered.len());

    let block = pane_block(
        &title_left,
        Some(&title_right),
        theme::SURFACE_LOWEST,
        matches!(app.focus, FocusPane::RapidLog | FocusPane::Input),
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(inner);

    // Entry rows
    let width = rows[0].width as usize;
    if filtered.is_empty() && !app.doc.prose_lines.is_empty() {
        let prose_text = app.doc.prose_lines.join("\n");
        let para = Paragraph::new(prose_text)
            .style(Style::default().fg(theme::ON_SURFACE))
            .wrap(Wrap { trim: false });
        f.render_widget(para, rows[0]);
    } else if filtered.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  No entries yet. Press Ctrl+N or [i] to rapid log (.  o  =  -)",
            Style::default().fg(theme::ON_SURFACE_VARIANT),
        )));
        f.render_widget(empty, rows[0]);
    } else {
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(row_idx, (_, entry))| {
                let is_selected =
                    app.focus == FocusPane::RapidLog && row_idx == app.selected_entry_idx;
                let row_bg = if is_selected {
                    theme::SURFACE_CONTAINER
                } else {
                    theme::SURFACE_LOWEST
                };

                let paper_label = if entry.paper_synced {
                    "[paper:ok]"
                } else {
                    "[paper:no]"
                };
                let paper_color = if entry.paper_synced {
                    theme::ON_SURFACE_VARIANT
                } else {
                    theme::SECONDARY
                };

                let sig_color = theme::signifier_color(entry.signifier);
                let mut text_style = Style::default().bg(row_bg);
                if matches!(entry.signifier, Signifier::Done | Signifier::Cancelled) {
                    text_style = text_style
                        .fg(theme::ON_SURFACE_VARIANT)
                        .add_modifier(Modifier::CROSSED_OUT);
                } else {
                    text_style = text_style.fg(theme::ON_SURFACE);
                }

                // Compute padding so `[paper:no]` / `[paper:ok]` stays right-aligned
                let prefix_len = 1 + entry.time.len() + 2 + 1 + 2; // " HH:MM  .  "
                let suffix_len = paper_label.len() + 1;
                let max_text = width.saturating_sub(prefix_len + suffix_len + 1);
                let display_text = truncate_str(&entry.text, max_text);
                let pad = width
                    .saturating_sub(prefix_len + display_text.chars().count() + suffix_len);

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {}  ", entry.time),
                        Style::default().fg(theme::ON_SURFACE_VARIANT).bg(row_bg),
                    ),
                    Span::styled(
                        format!("{}  ", entry.signifier.symbol()),
                        Style::default()
                            .fg(sig_color)
                            .bg(row_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(display_text, text_style),
                    Span::styled(" ".repeat(pad.max(1)), Style::default().bg(row_bg)),
                    Span::styled(
                        format!("{paper_label} "),
                        Style::default().fg(paper_color).bg(row_bg),
                    ),
                ]))
            })
            .collect();

        f.render_widget(List::new(items), rows[0]);
    }

    // Live TUI Rapid Capture Prompt
    let prompt_focused = app.focus == FocusPane::Input;
    let prompt_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if prompt_focused {
            theme::PRIMARY
        } else {
            theme::OUTLINE_VARIANT
        }))
        .style(Style::default().bg(theme::SURFACE_LOW));
    let prompt_inner = prompt_block.inner(rows[1]);
    f.render_widget(prompt_block, rows[1]);

    let sig_prefix = format!(" :log [{}] > ", app.input_sig.symbol());
    let prompt_line = if app.input_buf.is_empty() && !prompt_focused {
        Line::from(vec![
            Span::styled(
                sig_prefix.clone(),
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Prefix with .  o  =  -  then Enter",
                Style::default().fg(theme::ON_SURFACE_VARIANT),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                sig_prefix.clone(),
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.input_buf.as_str(),
                Style::default().fg(theme::ON_SURFACE),
            ),
        ])
    };

    let prompt_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(9)])
        .split(prompt_inner);
    f.render_widget(Paragraph::new(prompt_line), prompt_cols[0]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ENTER ",
            Style::default()
                .fg(theme::ON_PRIMARY)
                .bg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Right),
        prompt_cols[1],
    );

    if prompt_focused {
        let cursor_x = prompt_cols[0].x + (sig_prefix.chars().count() + app.input_buf.chars().count()) as u16;
        let max_x = prompt_cols[0].x + prompt_cols[0].width.saturating_sub(1);
        f.set_cursor_position((cursor_x.min(max_x), prompt_cols[0].y));
    }
}

fn render_signifiers_pane(f: &mut Frame, area: Rect) {
    let block = pane_block("[4] SIGNIFIERS", None, theme::SURFACE_LOW, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let width = inner.width as usize;
    let rows = [
        (".  Action", "[1]", theme::PRIMARY),
        ("o  Event", "[2]", theme::TERTIARY),
        ("=  Emotion", "[3]", theme::SECONDARY),
        ("-  Note", "[4]", theme::ON_SURFACE),
    ];

    let lines: Vec<Line> = rows
        .iter()
        .map(|(label, key, color)| {
            let pad = width.saturating_sub(label.len() + key.len() + 2);
            Line::from(vec![
                Span::styled(format!(" {label}"), Style::default().fg(*color)),
                Span::raw(" ".repeat(pad.max(1))),
                Span::styled(format!("{key} "), Style::default().fg(theme::ON_SURFACE_VARIANT)),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_carry_to_paper_pane(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::OUTLINE_VARIANT))
        .style(Style::default().bg(theme::SURFACE_LOW))
        .title(Line::from(Span::styled(
            " [5] CARRY TO PAPER ",
            Style::default()
                .fg(theme::SECONDARY)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pending = app.doc.paper_pending_count();
    let count_line = if pending == 1 {
        " 1 entry waiting for notebook".to_string()
    } else {
        format!(" {pending} entries waiting for notebook")
    };

    let lines = vec![
        Line::from(Span::styled(
            count_line,
            Style::default().fg(theme::ON_SURFACE),
        )),
        Line::from(Span::styled(
            " Press [m] after copying into",
            Style::default().fg(theme::ON_SURFACE_VARIANT),
        )),
        Line::from(Span::styled(
            " your physical spread.",
            Style::default().fg(theme::ON_SURFACE_VARIANT),
        )),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_md_buffer_pane(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block("[6] .MD BUFFER", None, theme::SURFACE_LOW, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let raw = app.raw_markdown_preview();
    let para = Paragraph::new(raw)
        .style(Style::default().fg(theme::ON_SURFACE_VARIANT))
        .wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}

fn render_bottom_bar(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.focus == FocusPane::Command {
            theme::PRIMARY
        } else {
            theme::OUTLINE_VARIANT
        }))
        .style(Style::default().bg(theme::SURFACE_LOW));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.focus == FocusPane::Command {
        let line = Line::from(vec![
            Span::styled(
                " :",
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.command_buf.as_str(),
                Style::default().fg(theme::ON_SURFACE),
            ),
        ]);
        f.render_widget(Paragraph::new(line), inner);
        f.set_cursor_position((
            inner.x + 2 + app.command_buf.chars().count() as u16,
            inner.y,
        ));
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(22)])
        .split(inner);

    let left = if let Some(msg) = &app.status_msg {
        Line::from(vec![
            Span::styled(
                " [status] ",
                Style::default()
                    .fg(theme::TERTIARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(msg.as_str(), Style::default().fg(theme::ON_SURFACE)),
        ])
    } else {
        Line::from(vec![
            key_span(" ^N", " New Entry  "),
            key_span("Tab", " Space  "),
            key_span("Space", " Toggle ./×  "),
            key_span("m", " Mark Paper  "),
            key_span(":w", " Write .md  "),
            key_span(":imp", " Import Notes  "),
            key_span("?", " Help"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>())
    };
    f.render_widget(Paragraph::new(left), cols[0]);

    let right = Line::from(Span::styled(
        "GUI: folio --gui ",
        Style::default().fg(theme::ON_SURFACE),
    ))
    .alignment(Alignment::Right);
    f.render_widget(Paragraph::new(right), cols[1]);
}

fn key_span<'a>(k: &'a str, label: &'a str) -> [Span<'a>; 2] {
    [
        Span::styled(
            k,
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label, Style::default().fg(theme::ON_SURFACE_VARIANT)),
    ]
}

fn render_help_modal(f: &mut Frame, area: Rect) {
    let popup = centered_rect(64, 72, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::PRIMARY))
        .style(Style::default().bg(theme::SURFACE_LOW))
        .title(Line::from(Span::styled(
            " Folio — Bullet Journal Keybindings & Commands ",
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let help_lines = vec![
        Line::from(Span::styled(
            " Rapid Logging",
            Style::default().fg(theme::TERTIARY).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Ctrl+N / i      Focus rapid log prompt (prefix .  o  =  -)"),
        Line::from("  Space           Toggle Action (.) <-> Completed (×)"),
        Line::from("  > / <           Migrate to tomorrow (>) / Schedule to Future Log (<)"),
        Line::from("  m / M           Mark selected / all copied to paper [paper:ok]"),
        Line::from("  e / d           Edit / Delete selected entry"),
        Line::from("  1..4 / 0        Filter by . / o / = / - / ALL"),
        Line::from(""),
        Line::from(Span::styled(
            " Navigation & Reviews",
            Style::default().fg(theme::TERTIARY).add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab / s / h     Cycle Spaces / Focus Spaces / Focus Vault"),
        Line::from("  j / k / ↑ / ↓   Move selection"),
        Line::from("  w / r           Open Weekly Review / Monthly Review"),
        Line::from(""),
        Line::from(Span::styled(
            " Command Bar (:)",
            Style::default().fg(theme::TERTIARY).add_modifier(Modifier::BOLD),
        )),
        Line::from("  :w / :q / :wq   Save .md / Quit"),
        Line::from("  :space <name>   Create & open a custom Space"),
        Line::from("  :page <num>     Set physical Notebook page number"),
        Line::from("  :imp <path>     Import notes (md, obsidian, json, csv, txt)"),
        Line::from("  :exp <fmt> <p>  Export current doc (md, json, csv)"),
        Line::from("  :weekly         Generate & open Weekly Review"),
        Line::from("  :monthly        Generate & open Monthly Review"),
        Line::from("  :today          Jump back to today's Daily Log"),
        Line::from(""),
        Line::from(Span::styled(
            " Press Esc or ? to close",
            Style::default().fg(theme::ON_SURFACE_VARIANT),
        )),
    ];

    f.render_widget(Paragraph::new(help_lines).style(Style::default().fg(theme::ON_SURFACE)), inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else if max_chars <= 1 {
        "…".to_string()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::vault::Vault;
    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::tempdir;

    #[test]
    fn renders_all_six_tui_panes() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(
            dir.path().to_path_buf(),
            &[
                "Studio & Work".into(),
                "Reading & Study".into(),
                "Personal".into(),
            ],
        )
        .unwrap();
        let mut app = App::new(vault, 42).unwrap();
        app.doc.add_entry(
            "08:15".into(),
            Signifier::Action,
            "Finalize studio lighting sketch".into(),
        );
        app.doc.add_entry(
            "09:30".into(),
            Signifier::Event,
            "Coffee with Maya".into(),
        );

        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        let buf = terminal.backend().buffer().clone();
        let rendered: String = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("folio --tui v1.4"));
        assert!(rendered.contains("[SYNCED .md]"));
        assert!(rendered.contains("[1] SPACES"));
        assert!(rendered.contains("[2] VAULT"));
        assert!(rendered.contains("[3] RAPID LOG"));
        assert!(rendered.contains("[4] SIGNIFIERS"));
        assert!(rendered.contains("[5] CARRY TO PAPER"));
        assert!(rendered.contains("[6] .MD BUFFER"));
        assert!(rendered.contains("Finalize studio lighting sketch"));
        assert!(rendered.contains("GUI: folio --gui"));
    }
}
