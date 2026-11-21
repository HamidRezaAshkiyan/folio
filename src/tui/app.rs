use crate::core::io::{ExportFormat, ImportFormat, export_document, import_from_file};
use crate::core::markdown::serialize_markdown;
use crate::core::model::{Document, Entry, EntryFilter, Signifier, SpaceInfo, VaultTree};
use crate::core::review::{generate_monthly_review, generate_weekly_review};
use crate::core::vault::Vault;
use anyhow::Result;
use chrono::{Duration, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Spaces,
    Vault,
    RapidLog,
    Input,
    Command,
}

pub struct App {
    pub vault: Vault,
    pub today: NaiveDate,
    pub default_page: u32,
    pub doc: Document,
    pub spaces: Vec<SpaceInfo>,
    pub active_space_idx: usize,
    pub vault_tree: VaultTree,
    pub selected_vault_idx: usize,
    pub selected_entry_idx: usize,
    pub filter: EntryFilter,
    pub focus: FocusPane,
    pub input_sig: Signifier,
    pub input_buf: String,
    pub editing_entry_id: Option<usize>,
    pub command_buf: String,
    pub dirty: bool,
    pub show_help: bool,
    pub status_msg: Option<String>,
    pub should_quit: bool,
}

impl App {
    pub fn new(vault: Vault, default_page: u32) -> Result<Self> {
        let today = Vault::today_date();
        let doc = vault.open_or_create_daily(today, default_page)?;
        let spaces = vault.list_spaces(today)?;
        let vault_tree = vault.vault_tree()?;
        let selected_vault_idx = vault_tree
            .nodes
            .iter()
            .position(|n| n.rel_path == doc.rel_path)
            .unwrap_or(1);

        Ok(Self {
            vault,
            today,
            default_page,
            doc,
            spaces,
            active_space_idx: 0,
            vault_tree,
            selected_vault_idx,
            selected_entry_idx: 0,
            filter: EntryFilter::All,
            focus: FocusPane::RapidLog,
            input_sig: Signifier::Action,
            input_buf: String::new(),
            editing_entry_id: None,
            command_buf: String::new(),
            dirty: false,
            show_help: false,
            status_msg: None,
            should_quit: false,
        })
    }

    pub fn filtered_entries(&self) -> Vec<(usize, &Entry)> {
        self.doc
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.signifier.matches_filter(self.filter))
            .collect()
    }

    pub fn raw_markdown_preview(&self) -> String {
        serialize_markdown(&self.doc)
    }

    pub fn refresh_metadata(&mut self) {
        if let Ok(s) = self.vault.list_spaces(self.today) {
            self.spaces = s;
        }
        if let Ok(t) = self.vault.vault_tree() {
            self.vault_tree = t;
        }
        if let Some(idx) = self
            .spaces
            .iter()
            .position(|s| s.rel_path == self.doc.rel_path)
        {
            self.active_space_idx = idx;
        }
        if let Some(idx) = self
            .vault_tree
            .nodes
            .iter()
            .position(|n| n.rel_path == self.doc.rel_path)
        {
            self.selected_vault_idx = idx;
        }
    }

    pub fn save_current(&mut self) {
        match self.vault.save_document(&self.doc) {
            Ok(()) => {
                self.dirty = false;
                self.refresh_metadata();
                self.status_msg = Some(format!("Saved {}", self.doc.rel_path));
            }
            Err(e) => {
                self.status_msg = Some(format!("Save error: {e}"));
            }
        }
    }

    pub fn open_rel_path(&mut self, rel_path: &str) {
        if self.dirty {
            self.save_current();
        }
        match self.vault.load_document(rel_path) {
            Ok(doc) => {
                self.doc = doc;
                self.selected_entry_idx = 0;
                self.dirty = false;
                self.refresh_metadata();
            }
            Err(e) => {
                self.status_msg = Some(format!("Open error: {e}"));
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Help overlay dismissal
        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Enter
            ) {
                self.show_help = false;
            }
            return;
        }

        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    if self.dirty {
                        self.save_current();
                    }
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('n') => {
                    self.focus = FocusPane::Input;
                    self.editing_entry_id = None;
                    return;
                }
                KeyCode::Char('s') => {
                    self.save_current();
                    return;
                }
                _ => {}
            }
        }

        match self.focus {
            FocusPane::Input => self.handle_input_key(key),
            FocusPane::Command => self.handle_command_key(key),
            FocusPane::Spaces => self.handle_spaces_key(key),
            FocusPane::Vault => self.handle_vault_key(key),
            FocusPane::RapidLog => self.handle_rapid_log_key(key),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.editing_entry_id = None;
                self.input_buf.clear();
                self.focus = FocusPane::RapidLog;
            }
            KeyCode::Enter => {
                self.submit_input();
            }
            KeyCode::Backspace => {
                self.input_buf.pop();
            }
            KeyCode::Char(c) => {
                self.input_buf.push(c);
                // Live rapid-log prefix detection (matches HTML mockup script):
                // typing `. `, `o `, `= `, `- ` switches the active signifier immediately.
                if self.input_buf.len() == 2 && self.input_buf.ends_with(' ') {
                    let first = self.input_buf.chars().next().unwrap_or('.');
                    if let Some(sig) = Signifier::from_symbol(&first.to_string()) {
                        self.input_sig = sig;
                        self.input_buf.clear();
                    }
                }
            }
            _ => {}
        }
    }

    pub fn submit_input(&mut self) {
        let text = self.input_buf.trim().to_string();
        if text.is_empty() {
            self.focus = FocusPane::RapidLog;
            return;
        }

        if let Some(edit_id) = self.editing_entry_id.take() {
            if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == edit_id) {
                entry.signifier = self.input_sig;
                entry.text = text;
                self.dirty = true;
                self.save_current();
            }
        } else {
            let now = Vault::now_hhmm();
            self.doc.add_entry(now, self.input_sig, text);
            self.dirty = true;
            let count = self.filtered_entries().len();
            if count > 0 {
                self.selected_entry_idx = count - 1;
            }
            self.save_current();
        }
        self.input_buf.clear();
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.command_buf.clear();
                self.focus = FocusPane::RapidLog;
            }
            KeyCode::Enter => {
                let cmd = self.command_buf.trim().to_string();
                self.command_buf.clear();
                self.focus = FocusPane::RapidLog;
                if !cmd.is_empty() {
                    self.execute_command(&cmd);
                }
            }
            KeyCode::Backspace => {
                self.command_buf.pop();
                if self.command_buf.is_empty() {
                    self.focus = FocusPane::RapidLog;
                }
            }
            KeyCode::Char(c) => {
                self.command_buf.push(c);
            }
            _ => {}
        }
    }

    pub fn execute_command(&mut self, cmd: &str) {
        let trimmed = cmd.trim().trim_start_matches(':');
        let mut parts = trimmed.splitn(2, ' ');
        let verb = parts.next().unwrap_or("").to_ascii_lowercase();
        let arg = parts.next().unwrap_or("").trim();

        match verb.as_str() {
            "w" | "write" => self.save_current(),
            "q" | "quit" => {
                self.should_quit = true;
            }
            "wq" | "x" => {
                self.save_current();
                self.should_quit = true;
            }
            "page" => {
                if let Ok(p) = arg.parse::<u32>() {
                    self.doc.page = Some(p);
                    self.dirty = true;
                    self.save_current();
                    self.status_msg = Some(format!("Notebook page set to p. {p}"));
                } else {
                    self.status_msg = Some("Usage: :page <number>".into());
                }
            }
            "space" => {
                if arg.is_empty() {
                    self.status_msg = Some("Usage: :space <Space Name>".into());
                } else {
                    match self.vault.create_space(arg) {
                        Ok(doc) => {
                            let rel = doc.rel_path.clone();
                            self.open_rel_path(&rel);
                            self.status_msg = Some(format!("Opened space '{arg}'"));
                        }
                        Err(e) => self.status_msg = Some(format!("Space error: {e}")),
                    }
                }
            }
            "imp" | "import" => {
                if arg.is_empty() {
                    self.status_msg = Some("Usage: :imp <path-to-notes-file>".into());
                } else {
                    let expanded = crate::core::config::expand_tilde(arg.into());
                    match import_from_file(&expanded, ImportFormat::Auto, &mut self.doc) {
                        Ok(n) => {
                            self.dirty = true;
                            self.save_current();
                            self.status_msg =
                                Some(format!("Imported {n} entries from {}", expanded.display()));
                        }
                        Err(e) => self.status_msg = Some(format!("Import failed: {e}")),
                    }
                }
            }
            "exp" | "export" => {
                let mut exp_parts = arg.splitn(2, ' ');
                let fmt_str = exp_parts.next().unwrap_or("");
                let dest = exp_parts.next().unwrap_or("").trim();
                if fmt_str.is_empty() || dest.is_empty() {
                    self.status_msg = Some("Usage: :exp <md|json|csv> <output-path>".into());
                } else {
                    match ExportFormat::parse(fmt_str)
                        .and_then(|fmt| export_document(&self.doc, fmt))
                    {
                        Ok(content) => {
                            let out_path = crate::core::config::expand_tilde(dest.into());
                            match fs::write(&out_path, content) {
                                Ok(()) => {
                                    self.status_msg =
                                        Some(format!("Exported to {}", out_path.display()));
                                }
                                Err(e) => self.status_msg = Some(format!("Export write error: {e}")),
                            }
                        }
                        Err(e) => self.status_msg = Some(format!("Export error: {e}")),
                    }
                }
            }
            "weekly" => match generate_weekly_review(&self.vault, self.today) {
                Ok((doc, sum)) => {
                    self.doc = doc;
                    self.selected_entry_idx = 0;
                    self.dirty = false;
                    self.refresh_metadata();
                    self.status_msg = Some(format!(
                        "Weekly Review ({} days, {} done, {} open)",
                        sum.days_logged, sum.actions_done, sum.actions_open
                    ));
                }
                Err(e) => self.status_msg = Some(format!("Weekly review error: {e}")),
            },
            "monthly" => match generate_monthly_review(&self.vault, self.today) {
                Ok((doc, sum)) => {
                    self.doc = doc;
                    self.selected_entry_idx = 0;
                    self.dirty = false;
                    self.refresh_metadata();
                    self.status_msg = Some(format!(
                        "Monthly Review ({} days, {} done, {} open)",
                        sum.days_logged, sum.actions_done, sum.actions_open
                    ));
                }
                Err(e) => self.status_msg = Some(format!("Monthly review error: {e}")),
            },
            "today" => {
                if let Ok(doc) = self.vault.open_or_create_daily(self.today, self.default_page) {
                    self.doc = doc;
                    self.selected_entry_idx = 0;
                    self.refresh_metadata();
                    self.status_msg = Some("Opened today's Daily Log".into());
                }
            }
            "date" => {
                if let Ok(d) = NaiveDate::parse_from_str(arg, "%Y-%m-%d") {
                    if let Ok(doc) = self.vault.open_or_create_daily(d, self.default_page) {
                        self.doc = doc;
                        self.selected_entry_idx = 0;
                        self.refresh_metadata();
                        self.status_msg = Some(format!("Opened {}", d.format("%Y-%m-%d")));
                    }
                } else {
                    self.status_msg = Some("Usage: :date YYYY-MM-DD".into());
                }
            }
            "help" => {
                self.show_help = true;
            }
            other => {
                self.status_msg = Some(format!(
                    "Unknown command ':{other}' — try :w, :imp, :exp, :space, :page, :weekly, :monthly, :q"
                ));
            }
        }
    }

    fn handle_spaces_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.cycle_space(),
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.spaces.is_empty() {
                    self.active_space_idx = (self.active_space_idx + 1) % self.spaces.len();
                    let rel = self.spaces[self.active_space_idx].rel_path.clone();
                    self.open_rel_path(&rel);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.spaces.is_empty() {
                    self.active_space_idx = if self.active_space_idx == 0 {
                        self.spaces.len() - 1
                    } else {
                        self.active_space_idx - 1
                    };
                    let rel = self.spaces[self.active_space_idx].rel_path.clone();
                    self.open_rel_path(&rel);
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.focus = FocusPane::RapidLog;
            }
            KeyCode::Char('v') => self.focus = FocusPane::Vault,
            KeyCode::Char(':') => {
                self.focus = FocusPane::Command;
                self.command_buf.clear();
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_vault_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.cycle_space(),
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.vault_tree.nodes.is_empty() {
                    self.selected_vault_idx =
                        (self.selected_vault_idx + 1) % self.vault_tree.nodes.len();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.vault_tree.nodes.is_empty() {
                    self.selected_vault_idx = if self.selected_vault_idx == 0 {
                        self.vault_tree.nodes.len() - 1
                    } else {
                        self.selected_vault_idx - 1
                    };
                }
            }
            KeyCode::Enter => {
                if let Some(node) = self.vault_tree.nodes.get(self.selected_vault_idx)
                    && !node.is_dir
                {
                    let rel = node.rel_path.clone();
                    self.open_rel_path(&rel);
                    self.focus = FocusPane::RapidLog;
                }
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Esc => {
                self.focus = FocusPane::RapidLog;
            }
            KeyCode::Char(':') => {
                self.focus = FocusPane::Command;
                self.command_buf.clear();
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_rapid_log_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.cycle_space(),
            KeyCode::Char('q') => {
                if self.dirty {
                    self.save_current();
                }
                self.should_quit = true;
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char(':') => {
                self.focus = FocusPane::Command;
                self.command_buf.clear();
            }
            KeyCode::Char('i') | KeyCode::Char('a') => {
                self.focus = FocusPane::Input;
                self.editing_entry_id = None;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = FocusPane::Vault;
            }
            KeyCode::Char('s') => {
                self.focus = FocusPane::Spaces;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.filtered_entries().len();
                if len > 0 {
                    self.selected_entry_idx = (self.selected_entry_idx + 1).min(len - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_entry_idx = self.selected_entry_idx.saturating_sub(1);
            }
            // Space toggles Action `.` <-> Done `×`
            KeyCode::Char(' ') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let cur = self.doc.entries[real_idx].signifier;
                    self.doc.entries[real_idx].signifier = cur.toggle_done();
                    self.dirty = true;
                    self.save_current();
                }
            }
            // `m` marks selected entry copied to physical paper notebook
            KeyCode::Char('m') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    self.doc.entries[real_idx].paper_synced =
                        !self.doc.entries[real_idx].paper_synced;
                    self.dirty = true;
                    self.save_current();
                }
            }
            // `M` marks all entries in current doc as paper:ok
            KeyCode::Char('M') => {
                for entry in &mut self.doc.entries {
                    entry.paper_synced = true;
                }
                self.dirty = true;
                self.save_current();
                self.status_msg = Some("Marked all entries as copied to paper [paper:ok]".into());
            }
            // `>` migrates open action to tomorrow's daily log
            KeyCode::Char('>') => {
                if let Some(entry_id) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(_, e)| e.id)
                {
                    let tomorrow = self.doc.date.unwrap_or(self.today) + Duration::days(1);
                    let target = Vault::daily_rel_path(tomorrow);
                    match self.vault.migrate_entry(&mut self.doc, entry_id, &target, false) {
                        Ok(()) => {
                            self.refresh_metadata();
                            self.status_msg = Some(format!(
                                "Migrated action to {}",
                                tomorrow.format("%Y-%m-%d")
                            ));
                        }
                        Err(e) => self.status_msg = Some(format!("Migrate error: {e}")),
                    }
                }
            }
            // `<` schedules action to Future Log.md
            KeyCode::Char('<') => {
                if let Some(entry_id) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(_, e)| e.id)
                {
                    match self
                        .vault
                        .migrate_entry(&mut self.doc, entry_id, "Future Log.md", true)
                    {
                        Ok(()) => {
                            self.refresh_metadata();
                            self.status_msg = Some("Scheduled action to Future Log.md".into());
                        }
                        Err(e) => self.status_msg = Some(format!("Schedule error: {e}")),
                    }
                }
            }
            // `e` edits selected entry
            KeyCode::Char('e') => {
                if let Some((id, sig, text)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(_, e)| (e.id, e.signifier, e.text.clone()))
                {
                    self.editing_entry_id = Some(id);
                    self.input_sig = sig;
                    self.input_buf = text;
                    self.focus = FocusPane::Input;
                }
            }
            // `d` deletes selected entry
            KeyCode::Char('d') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    self.doc.entries.remove(real_idx);
                    if self.selected_entry_idx > 0 {
                        self.selected_entry_idx -= 1;
                    }
                    self.dirty = true;
                    self.save_current();
                }
            }
            // Signifier filters [1] Action, [2] Event, [3] Emotion, [4] Note, [0] All
            KeyCode::Char('0') => self.set_filter(EntryFilter::All),
            KeyCode::Char('1') => self.toggle_filter(EntryFilter::Action),
            KeyCode::Char('2') => self.toggle_filter(EntryFilter::Event),
            KeyCode::Char('3') => self.toggle_filter(EntryFilter::Emotion),
            KeyCode::Char('4') => self.toggle_filter(EntryFilter::Note),
            // Quick review shortcuts
            KeyCode::Char('w') => self.execute_command("weekly"),
            KeyCode::Char('r') => self.execute_command("monthly"),
            _ => {}
        }
    }

    fn set_filter(&mut self, f: EntryFilter) {
        self.filter = f;
        self.selected_entry_idx = 0;
    }

    fn toggle_filter(&mut self, f: EntryFilter) {
        if self.filter == f {
            self.filter = EntryFilter::All;
        } else {
            self.filter = f;
        }
        self.selected_entry_idx = 0;
    }

    fn cycle_space(&mut self) {
        if self.spaces.is_empty() {
            return;
        }
        self.active_space_idx = (self.active_space_idx + 1) % self.spaces.len();
        let rel = self.spaces[self.active_space_idx].rel_path.clone();
        if !Path::new(&self.vault.root).join(&rel).exists() && self.spaces[self.active_space_idx].is_daily {
            let _ = self.vault.open_or_create_daily(self.today, self.default_page);
        }
        self.open_rel_path(&rel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tui_rapid_log_and_commands() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(
            dir.path().to_path_buf(),
            &["Studio & Work".into(), "Personal".into()],
        )
        .unwrap();
        let mut app = App::new(vault, 42).unwrap();

        // Simulate rapid log with `o ` prefix detection
        app.focus = FocusPane::Input;
        app.handle_key(KeyEvent::from(KeyCode::Char('o')));
        app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(app.input_sig, Signifier::Event);
        assert!(app.input_buf.is_empty());

        for ch in "Coffee with Maya".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.doc.entries.len(), 1);
        assert_eq!(app.doc.entries[0].signifier, Signifier::Event);
        assert_eq!(app.doc.entries[0].text, "Coffee with Maya");

        // Mark paper synced
        app.focus = FocusPane::RapidLog;
        app.handle_key(KeyEvent::from(KeyCode::Char('m')));
        assert!(app.doc.entries[0].paper_synced);
        assert_eq!(app.doc.paper_pending_count(), 0);

        // Command bar: create custom space & set page
        app.execute_command("page 43");
        assert_eq!(app.doc.page, Some(43));
        app.execute_command("space Reading & Study");
        assert_eq!(app.doc.space.as_deref(), Some("Reading & Study"));
    }
}
