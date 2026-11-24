use crate::core::model::{EntryModifier, Signifier};
use crate::tui::app::{App, FocusPane, ModalState};
use crate::tui::theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.sync_list_states();

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

    match &app.modal {
        ModalState::None => {}
        ModalState::Help => render_help_modal(f, area),
        ModalState::Search {
            query,
            results,
            selected,
        } => render_search_modal(f, area, query, results, *selected),
        ModalState::Migrate {
            entry_preview,
            targets,
            selected,
            ..
        } => render_migrate_modal(f, area, entry_preview, targets, *selected),
    }
}

fn pane_block<'a>(
    title_left: &'a str,
    title_right: Option<&'a str>,
    bg: ratatui::style::Color,
    focused: bool,
) -> Block<'a> {
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
            format!(" folio --tui v{}", env!("CARGO_PKG_VERSION")),
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

fn render_body_grid(f: &mut Frame, app: &mut App, area: Rect) {
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

    render_signifiers_pane(f, app, right_rows[0]);
    render_carry_to_paper_pane(f, app, right_rows[1]);
    render_md_buffer_pane(f, app, right_rows[2]);
}

fn render_spaces_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let block = pane_block(
        "[1] SPACES",
        Some("Tab"),
        theme::SURFACE_LOW,
        app.focus == FocusPane::Spaces,
    );
    let inner = block.inner(area);
    app.ui_rects.spaces_inner = inner;
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

fn render_vault_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let right_title = format!("~/{}", app.vault_tree.vault_name);
    let block = pane_block(
        "[2] VAULT",
        Some(&right_title),
        theme::SURFACE_LOW,
        app.focus == FocusPane::Vault,
    );
    let inner = block.inner(area);
    app.ui_rects.vault_inner = inner;
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

    f.render_stateful_widget(List::new(items), inner, &mut app.vault_list_state);
}

fn render_rapid_log_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let filtered_count = app.filtered_entries().len();
    let title_left = format!("[3] RAPID LOG — {}", app.doc.title);
    let title_right = if let Some(tag) = &app.tag_filter {
        format!("Filter: {} {tag} ({filtered_count})", app.filter.label())
    } else {
        format!("Filter: {} ({filtered_count})", app.filter.label())
    };

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

    app.ui_rects.entries_inner = rows[0];
    app.ui_rects.prompt_box = rows[1];

    // Entry rows with auto-scrolling ListState
    let width = rows[0].width as usize;
    let filtered = app.filtered_entries();
    if filtered.is_empty() && !app.doc.prose_lines.is_empty() {
        let prose_text = app.doc.prose_lines.join("\n");
        let para = Paragraph::new(prose_text)
            .style(Style::default().fg(theme::ON_SURFACE))
            .wrap(Wrap { trim: false });
        f.render_widget(para, rows[0]);
    } else if filtered.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  No entries yet. Press Ctrl+N or [i] to rapid log (.  o  =  -  *.  !-)",
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

                let (mod_str, mod_color) = match entry.modifier {
                    EntryModifier::None => (" ", theme::ON_SURFACE_VARIANT),
                    EntryModifier::Priority => ("*", theme::TERTIARY),
                    EntryModifier::Inspiration => ("!", theme::SECONDARY),
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

                // " HH:MM * .  <text> [paper:no] "
                let prefix_len = 1 + entry.time.len() + 2 + 2 + 2;
                let suffix_len = paper_label.len() + 1;
                let max_text = width.saturating_sub(prefix_len + suffix_len + 1);
                let display_text = truncate_str(&entry.text, max_text);
                let pad = width
                    .saturating_sub(prefix_len + display_text.chars().count() + suffix_len);

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {} ", entry.time),
                        Style::default().fg(theme::ON_SURFACE_VARIANT).bg(row_bg),
                    ),
                    Span::styled(
                        mod_str,
                        Style::default()
                            .fg(mod_color)
                            .bg(row_bg)
                            .add_modifier(Modifier::BOLD),
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

        f.render_stateful_widget(List::new(items), rows[0], &mut app.entry_list_state);
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

    let sig_prefix = format!(
        " :log [{}{}] > ",
        app.input_mod.symbol(),
        app.input_sig.symbol()
    );
    let prompt_line = if app.input_buf.is_empty() && !prompt_focused {
        Line::from(vec![
            Span::styled(
                sig_prefix.clone(),
                Style::default()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Prefix .  o  =  -  or *.  !-  then Enter",
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

    if prompt_focused && matches!(app.modal, ModalState::None) {
        let cursor_x =
            prompt_cols[0].x + (sig_prefix.chars().count() + app.input_buf.chars().count()) as u16;
        let max_x = prompt_cols[0].x + prompt_cols[0].width.saturating_sub(1);
        f.set_cursor_position((cursor_x.min(max_x), prompt_cols[0].y));
    }
}

fn render_signifiers_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let block = pane_block("[4] SIGNIFIERS", Some("* ! #"), theme::SURFACE_LOW, false);
    let inner = block.inner(area);
    app.ui_rects.signifiers_inner = inner;
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
                Span::styled(
                    format!("{key} "),
                    Style::default().fg(theme::ON_SURFACE_VARIANT),
                ),
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
        Line::from(
            vec![
                key_span(" ^N", " Log  "),
                key_span("/", " Search  "),
                key_span("[/]", " Day  "),
                key_span("Space", " ./×  "),
                key_span("*!", " Mod  "),
                key_span(">", " Migrate  "),
                key_span("m", " Paper  "),
                key_span("?", " Help"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        )
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

fn render_search_modal(
    f: &mut Frame,
    area: Rect,
    query: &str,
    results: &[crate::core::model::SearchHit],
    selected: usize,
) {
    let popup = centered_rect(68, 66, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::PRIMARY))
        .style(Style::default().bg(theme::SURFACE_LOW))
        .title(Line::from(Span::styled(
            " / Instant Vault Search (Enter to jump, Esc to close) ",
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(inner);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::OUTLINE_VARIANT))
        .style(Style::default().bg(theme::SURFACE_LOWEST));
    let input_inner = input_block.inner(rows[0]);
    f.render_widget(input_block, rows[0]);

    let q_line = Line::from(vec![
        Span::styled(
            " / ",
            Style::default()
                .fg(theme::TERTIARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(query, Style::default().fg(theme::ON_SURFACE)),
    ]);
    f.render_widget(Paragraph::new(q_line), input_inner);
    f.set_cursor_position((
        input_inner.x + 3 + query.chars().count() as u16,
        input_inner.y,
    ));

    if query.trim().is_empty() {
        f.render_widget(
            Paragraph::new("  Type to search across all Daily Notes, Spaces, Reviews, and #tags...")
                .style(Style::default().fg(theme::ON_SURFACE_VARIANT)),
            rows[1],
        );
    } else if results.is_empty() {
        f.render_widget(
            Paragraph::new("  No matching entries found.")
                .style(Style::default().fg(theme::ON_SURFACE_VARIANT)),
            rows[1],
        );
    } else {
        let max_snip = (rows[1].width as usize).saturating_sub(28);
        let items: Vec<ListItem> = results
            .iter()
            .enumerate()
            .take(rows[1].height as usize)
            .map(|(i, hit)| {
                let bg = if i == selected {
                    theme::SURFACE_HIGH
                } else {
                    theme::SURFACE_LOW
                };
                let sig = hit.signifier.map(|s| s.symbol()).unwrap_or(" ");
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" [{}] ", truncate_str(&hit.doc_title, 18)),
                        Style::default().fg(theme::PRIMARY).bg(bg),
                    ),
                    Span::styled(
                        format!("{}{} ", hit.modifier.symbol(), sig),
                        Style::default()
                            .fg(theme::TERTIARY)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        truncate_str(&hit.snippet, max_snip),
                        Style::default().fg(theme::ON_SURFACE).bg(bg),
                    ),
                ]))
            })
            .collect();
        f.render_widget(List::new(items), rows[1]);
    }
}

fn render_migrate_modal(
    f: &mut Frame,
    area: Rect,
    entry_preview: &str,
    targets: &[crate::tui::app::MigrateTarget],
    selected: usize,
) {
    let popup = centered_rect(54, 46, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::PRIMARY))
        .style(Style::default().bg(theme::SURFACE_LOW))
        .title(Line::from(Span::styled(
            " > Smart Migration Target (Enter to migrate) ",
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Entry: ", Style::default().fg(theme::ON_SURFACE_VARIANT)),
            Span::styled(
                truncate_str(entry_preview, (rows[0].width as usize).saturating_sub(10)),
                Style::default().fg(theme::TERTIARY),
            ),
        ])),
        rows[0],
    );

    let items: Vec<ListItem> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_sel = i == selected;
            let style = if is_sel {
                Style::default()
                    .fg(theme::PRIMARY)
                    .bg(theme::SURFACE_HIGH)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::ON_SURFACE)
            };
            ListItem::new(Line::from(Span::styled(format!("  {} ", t.label), style)))
        })
        .collect();
    f.render_widget(List::new(items), rows[1]);
}

fn render_help_modal(f: &mut Frame, area: Rect) {
    let popup = centered_rect(66, 78, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::PRIMARY))
        .style(Style::default().bg(theme::SURFACE_LOW))
        .title(Line::from(Span::styled(
            format!(" Folio v{} — Bullet Journal Keybindings & Commands ", env!("CARGO_PKG_VERSION")),
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let help_lines = vec![
        Line::from(Span::styled(
            " Rapid Logging & Classic BuJo Modifiers",
            Style::default()
                .fg(theme::TERTIARY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Ctrl+N / i      Focus rapid log prompt (prefix . o = - or *. !-)"),
        Line::from("  Space           Toggle Action (.) <-> Completed (×)"),
        Line::from("  * / !           Toggle Priority (*) / Inspiration (!) modifier"),
        Line::from("  > / <           Smart Migrate Picker (>) / Quick Future Log (<)"),
        Line::from("  m / M           Mark selected / all copied to paper [paper:ok]"),
        Line::from("  e / d           Edit / Delete selected entry"),
        Line::from("  1..4 / # / 0    Filter signifier / cycle #tag filter / ALL"),
        Line::from(""),
        Line::from(Span::styled(
            " Navigation, Search & Reviews",
            Style::default()
                .fg(theme::TERTIARY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  /  or  Ctrl+F   Instant Vault Search across all notes & #tags"),
        Line::from("  [ / ] / t       Previous Day / Next Day / Today"),
        Line::from("  Tab / s / h     Cycle Spaces / Focus Spaces / Focus Vault"),
        Line::from("  j / k / Mouse   Navigate & click panes, entries, or scroll"),
        Line::from("  w / r           Generate Weekly Review / Monthly Review"),
        Line::from(""),
        Line::from(Span::styled(
            " Command Bar (:)",
            Style::default()
                .fg(theme::TERTIARY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  :w / :q / :wq   Save .md / Quit"),
        Line::from("  :space <name>   Create & open a custom Space"),
        Line::from("  :page <num>     Set physical Notebook page number"),
        Line::from("  :imp <path>     Import notes (md, obsidian, json, csv, txt)"),
        Line::from("  :exp <fmt> <p>  Export current doc (md, json, csv)"),
        Line::from(""),
        Line::from(Span::styled(
            " Press Esc or ? to close",
            Style::default().fg(theme::ON_SURFACE_VARIANT),
        )),
    ];

    f.render_widget(
        Paragraph::new(help_lines).style(Style::default().fg(theme::ON_SURFACE)),
        inner,
    );
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
        app.doc.add_entry_with_modifier(
            "08:15".into(),
            EntryModifier::Priority,
            Signifier::Action,
            "Finalize studio lighting sketch #studio".into(),
        );
        app.doc
            .add_entry("09:30".into(), Signifier::Event, "Coffee with Maya".into());

        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();

        let buf = terminal.backend().buffer().clone();
        let rendered: String = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains(&format!("folio --tui v{}", env!("CARGO_PKG_VERSION"))));
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
