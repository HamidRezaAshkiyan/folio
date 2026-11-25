use crate::core::io::{ExportFormat, ImportFormat, export_document, import_from_file};
use crate::core::markdown::serialize_markdown;
use crate::core::model::{
    Document, Entry, EntryFilter, EntryModifier, SearchHit, Signifier, SpaceInfo, VaultEntryRef,
    VaultTree,
};
use crate::core::review::{generate_monthly_review, generate_weekly_review};
use crate::core::vault::Vault;
use anyhow::Result;
use chrono::{Duration, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{layout::Rect, widgets::ListState};
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

#[derive(Debug, Clone)]
pub struct MigrateTarget {
    pub label: String,
    pub rel_path: String,
    pub scheduled: bool,
    pub future_month: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    Help,
    Search {
        query: String,
        results: Vec<SearchHit>,
        selected: usize,
    },
    Migrate {
        entry_id: usize,
        entry_preview: String,
        targets: Vec<MigrateTarget>,
        selected: usize,
    },
    Reflect {
        am_buf: String,
        pm_buf: String,
        active_field: usize, // 0 = AM Focus, 1 = PM Reflection
        open_triage_count: usize,
        global_paper_count: usize,
    },
    Triage {
        items: Vec<VaultEntryRef>,
        selected: usize,
    },
    GlobalPaper {
        items: Vec<VaultEntryRef>,
        selected: usize,
    },
    RenameSpace {
        rel_path: String,
        name_buf: String,
    },
    DeleteSpaceConfirm {
        rel_path: String,
        name: String,
    },
}

/// Interactive hit-test rectangles recorded during each render pass for mouse support.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiRects {
    pub spaces_inner: Rect,
    pub vault_inner: Rect,
    pub entries_inner: Rect,
    pub prompt_box: Rect,
    pub signifiers_inner: Rect,
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
    pub vault_list_state: ListState,
    pub selected_entry_idx: usize,
    pub entry_list_state: ListState,
    pub filter: EntryFilter,
    pub tag_filter: Option<String>,
    pub focus: FocusPane,
    pub input_mod: EntryModifier,
    pub input_sig: Signifier,
    pub input_buf: String,
    pub editing_entry_id: Option<usize>,
    pub command_buf: String,
    pub dirty: bool,
    pub modal: ModalState,
    pub status_msg: Option<String>,
    pub ui_rects: UiRects,
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

        let mut vault_list_state = ListState::default();
        vault_list_state.select(Some(selected_vault_idx));
        let mut entry_list_state = ListState::default();
        if !doc.entries.is_empty() {
            entry_list_state.select(Some(0));
        }

        Ok(Self {
            vault,
            today,
            default_page,
            doc,
            spaces,
            active_space_idx: 0,
            vault_tree,
            selected_vault_idx,
            vault_list_state,
            selected_entry_idx: 0,
            entry_list_state,
            filter: EntryFilter::All,
            tag_filter: None,
            focus: FocusPane::RapidLog,
            input_mod: EntryModifier::None,
            input_sig: Signifier::Action,
            input_buf: String::new(),
            editing_entry_id: None,
            command_buf: String::new(),
            dirty: false,
            modal: ModalState::None,
            status_msg: None,
            ui_rects: UiRects::default(),
            should_quit: false,
        })
    }

    pub fn filtered_entries(&self) -> Vec<(usize, &Entry)> {
        self.doc
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.signifier.matches_filter(self.filter)
                    && self
                        .tag_filter
                        .as_deref()
                        .is_none_or(|t| e.has_tag(t))
            })
            .collect()
    }

    pub fn sync_list_states(&mut self) {
        let len = self.filtered_entries().len();
        if len == 0 {
            self.selected_entry_idx = 0;
            self.entry_list_state.select(None);
        } else {
            self.selected_entry_idx = self.selected_entry_idx.min(len - 1);
            self.entry_list_state.select(Some(self.selected_entry_idx));
        }

        let vlen = self.vault_tree.nodes.len();
        if vlen == 0 {
            self.selected_vault_idx = 0;
            self.vault_list_state.select(None);
        } else {
            self.selected_vault_idx = self.selected_vault_idx.min(vlen - 1);
            self.vault_list_state.select(Some(self.selected_vault_idx));
        }
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
        self.sync_list_states();
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
                self.tag_filter = None;
                self.dirty = false;
                self.refresh_metadata();
            }
            Err(e) => {
                self.status_msg = Some(format!("Open error: {e}"));
            }
        }
    }

    pub fn step_day(&mut self, delta_days: i64) {
        if self.dirty {
            self.save_current();
        }
        let base = self.doc.date.unwrap_or(self.today);
        let target_date = base + Duration::days(delta_days);
        match self.vault.open_or_create_daily(target_date, self.default_page) {
            Ok(doc) => {
                self.doc = doc;
                self.selected_entry_idx = 0;
                self.tag_filter = None;
                self.dirty = false;
                self.refresh_metadata();
                self.status_msg = Some(format!(
                    "Opened Daily Log {}",
                    target_date.format("%Y-%m-%d (%a)")
                ));
            }
            Err(e) => {
                self.status_msg = Some(format!("Day nav error: {e}"));
            }
        }
    }

    pub fn open_master_index(&mut self) {
        if self.dirty {
            self.save_current();
        }
        match self.vault.update_master_index() {
            Ok(_) => {
                self.open_rel_path("Index.md");
                self.status_msg = Some("Refreshed & opened Master Index (Index.md)".into());
            }
            Err(e) => self.status_msg = Some(format!("Index error: {e}")),
        }
    }

    pub fn open_monthly_log(&mut self) {
        if self.dirty {
            self.save_current();
        }
        let anchor = self.doc.date.unwrap_or(self.today);
        match self.vault.open_or_create_monthly_log(anchor) {
            Ok(doc) => {
                self.doc = doc;
                self.selected_entry_idx = 0;
                self.tag_filter = None;
                self.dirty = false;
                self.refresh_metadata();
                self.status_msg = Some(format!("Opened Monthly Log ({})", self.doc.title));
            }
            Err(e) => self.status_msg = Some(format!("Monthly Log error: {e}")),
        }
    }

    pub fn open_reflect_modal(&mut self) {
        let open_triage_count = self
            .vault
            .collect_open_actions(self.today)
            .map(|v| v.len())
            .unwrap_or(0);
        let global_paper_count = self
            .vault
            .collect_paper_pending()
            .map(|v| v.len())
            .unwrap_or(0);
        self.modal = ModalState::Reflect {
            am_buf: String::new(),
            pm_buf: String::new(),
            active_field: 0,
            open_triage_count,
            global_paper_count,
        };
    }

    pub fn open_triage_modal(&mut self) {
        let items = self
            .vault
            .collect_open_actions(self.today)
            .unwrap_or_default();
        self.modal = ModalState::Triage { items, selected: 0 };
    }

    pub fn open_global_paper_modal(&mut self) {
        let items = self.vault.collect_paper_pending().unwrap_or_default();
        self.modal = ModalState::GlobalPaper { items, selected: 0 };
    }

    pub fn open_rename_space_modal(&mut self, rel_path: String, current_name: String) {
        self.modal = ModalState::RenameSpace {
            rel_path,
            name_buf: current_name,
        };
    }

    pub fn open_delete_space_modal(&mut self, rel_path: String, name: String) {
        self.modal = ModalState::DeleteSpaceConfirm { rel_path, name };
    }

    pub fn open_search_modal(&mut self) {
        self.modal = ModalState::Search {
            query: String::new(),
            results: vec![],
            selected: 0,
        };
    }

    pub fn open_migrate_modal(&mut self) {
        let Some((entry_id, entry_preview)) = self
            .filtered_entries()
            .get(self.selected_entry_idx)
            .map(|(_, e)| (e.id, e.text.clone()))
        else {
            return;
        };

        let tomorrow = self.doc.date.unwrap_or(self.today) + Duration::days(1);
        let mut targets = vec![
            MigrateTarget {
                label: format!("> Tomorrow ({})", tomorrow.format("%Y-%m-%d")),
                rel_path: Vault::daily_rel_path(tomorrow),
                scheduled: false,
                future_month: None,
            },
            MigrateTarget {
                label: "< Future Log (General)".to_string(),
                rel_path: "Future Log.md".to_string(),
                scheduled: true,
                future_month: None,
            },
        ];

        for ym in Vault::upcoming_months(self.today, 4).into_iter().skip(1) {
            targets.push(MigrateTarget {
                label: format!("< Future Log [{ym}]"),
                rel_path: "Future Log.md".to_string(),
                scheduled: true,
                future_month: Some(ym),
            });
        }

        for sp in &self.spaces {
            if sp.rel_path != self.doc.rel_path && !sp.is_daily {
                targets.push(MigrateTarget {
                    label: format!("> Space: {}", sp.name),
                    rel_path: sp.rel_path.clone(),
                    scheduled: false,
                    future_month: None,
                });
            }
        }

        self.modal = ModalState::Migrate {
            entry_id,
            entry_preview,
            targets,
            selected: 0,
        };
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(self.modal, ModalState::None) {
            return;
        }
        let col = mouse.column;
        let row = mouse.row;
        let rects = self.ui_rects;

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if rect_contains(rects.spaces_inner, col, row) {
                    self.focus = FocusPane::Spaces;
                    let idx = (row - rects.spaces_inner.y) as usize;
                    if let Some(sp) = self.spaces.get(idx) {
                        self.active_space_idx = idx;
                        let rel = sp.rel_path.clone();
                        self.open_rel_path(&rel);
                    }
                } else if rect_contains(rects.vault_inner, col, row) {
                    self.focus = FocusPane::Vault;
                    let offset = self.vault_list_state.offset();
                    let idx = offset + (row - rects.vault_inner.y) as usize;
                    if let Some(node) = self.vault_tree.nodes.get(idx) {
                        self.selected_vault_idx = idx;
                        if !node.is_dir {
                            let rel = node.rel_path.clone();
                            self.open_rel_path(&rel);
                            self.focus = FocusPane::RapidLog;
                        }
                        self.sync_list_states();
                    }
                } else if rect_contains(rects.prompt_box, col, row) {
                    self.focus = FocusPane::Input;
                } else if rect_contains(rects.entries_inner, col, row) {
                    self.focus = FocusPane::RapidLog;
                    let offset = self.entry_list_state.offset();
                    let idx = offset + (row - rects.entries_inner.y) as usize;
                    let len = self.filtered_entries().len();
                    if idx < len {
                        self.selected_entry_idx = idx;
                        self.sync_list_states();
                    }
                } else if rect_contains(rects.signifiers_inner, col, row) {
                    let idx = (row - rects.signifiers_inner.y) as usize;
                    match idx {
                        0 => self.toggle_filter(EntryFilter::Action),
                        1 => self.toggle_filter(EntryFilter::Event),
                        2 => self.toggle_filter(EntryFilter::Emotion),
                        3 => self.toggle_filter(EntryFilter::Note),
                        _ => {}
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                if rect_contains(rects.vault_inner, col, row) {
                    let len = self.vault_tree.nodes.len();
                    if len > 0 {
                        self.selected_vault_idx = (self.selected_vault_idx + 1).min(len - 1);
                        self.sync_list_states();
                    }
                } else {
                    let len = self.filtered_entries().len();
                    if len > 0 {
                        self.selected_entry_idx = (self.selected_entry_idx + 1).min(len - 1);
                        self.sync_list_states();
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if rect_contains(rects.vault_inner, col, row) {
                    self.selected_vault_idx = self.selected_vault_idx.saturating_sub(1);
                    self.sync_list_states();
                } else {
                    self.selected_entry_idx = self.selected_entry_idx.saturating_sub(1);
                    self.sync_list_states();
                }
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Active modal takes priority
        match &self.modal {
            ModalState::Help => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Enter
                ) {
                    self.modal = ModalState::None;
                }
                return;
            }
            ModalState::Search { .. } => {
                self.handle_search_modal_key(key);
                return;
            }
            ModalState::Migrate { .. } => {
                self.handle_migrate_modal_key(key);
                return;
            }
            ModalState::Reflect { .. } => {
                self.handle_reflect_modal_key(key);
                return;
            }
            ModalState::Triage { .. } => {
                self.handle_triage_modal_key(key);
                return;
            }
            ModalState::GlobalPaper { .. } => {
                self.handle_global_paper_modal_key(key);
                return;
            }
            ModalState::RenameSpace { .. } => {
                self.handle_rename_space_modal_key(key);
                return;
            }
            ModalState::DeleteSpaceConfirm { .. } => {
                self.handle_delete_space_modal_key(key);
                return;
            }
            ModalState::None => {}
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
                KeyCode::Char('f') => {
                    self.open_search_modal();
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

    fn handle_search_modal_key(&mut self, key: KeyEvent) {
        let mut jump_to: Option<(String, Option<usize>)> = None;

        if let ModalState::Search {
            query,
            results,
            selected,
        } = &mut self.modal
        {
            match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                    return;
                }
                KeyCode::Down => {
                    if !results.is_empty() {
                        *selected = (*selected + 1).min(results.len() - 1);
                    }
                }
                KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                }
                KeyCode::Enter => {
                    if let Some(hit) = results.get(*selected) {
                        jump_to = Some((hit.rel_path.clone(), hit.entry_id));
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    *results = self.vault.search_vault(query).unwrap_or_default();
                    *selected = 0;
                }
                KeyCode::Char(c) => {
                    query.push(c);
                    *results = self.vault.search_vault(query).unwrap_or_default();
                    *selected = 0;
                }
                _ => {}
            }
        }

        if let Some((rel_path, entry_id)) = jump_to {
            self.modal = ModalState::None;
            self.filter = EntryFilter::All;
            self.tag_filter = None;
            self.open_rel_path(&rel_path);
            if let Some(eid) = entry_id
                && let Some(pos) = self.doc.entries.iter().position(|e| e.id == eid)
            {
                self.selected_entry_idx = pos;
                self.sync_list_states();
            }
            self.focus = FocusPane::RapidLog;
        }
    }

    fn handle_migrate_modal_key(&mut self, key: KeyEvent) {
        let mut chosen: Option<(usize, MigrateTarget)> = None;

        if let ModalState::Migrate {
            entry_id,
            targets,
            selected,
            ..
        } = &mut self.modal
        {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                    return;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if !targets.is_empty() {
                        *selected = (*selected + 1).min(targets.len() - 1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                }
                KeyCode::Enter => {
                    if let Some(target) = targets.get(*selected).cloned() {
                        chosen = Some((*entry_id, target));
                    }
                }
                _ => {}
            }
        }

        if let Some((entry_id, target)) = chosen {
            self.modal = ModalState::None;
            let res = if let Some(month_ym) = &target.future_month {
                self.vault
                    .schedule_to_future_month(&mut self.doc, entry_id, month_ym)
            } else {
                self.vault.migrate_entry(
                    &mut self.doc,
                    entry_id,
                    &target.rel_path,
                    target.scheduled,
                )
            };
            match res {
                Ok(()) => {
                    self.refresh_metadata();
                    if let Some(month_ym) = target.future_month {
                        self.status_msg =
                            Some(format!("Scheduled entry -> Future Log [{month_ym}]"));
                    } else {
                        self.status_msg = Some(format!("Migrated entry -> {}", target.rel_path));
                    }
                }
                Err(e) => {
                    self.status_msg = Some(format!("Migrate error: {e}"));
                }
            }
        }
    }

    fn handle_reflect_modal_key(&mut self, key: KeyEvent) {
        let mut submit_am: Option<String> = None;
        let mut submit_pm: Option<String> = None;

        if let ModalState::Reflect {
            am_buf,
            pm_buf,
            active_field,
            ..
        } = &mut self.modal
        {
            match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                    return;
                }
                KeyCode::Tab | KeyCode::Down | KeyCode::Up => {
                    *active_field = if *active_field == 0 { 1 } else { 0 };
                    return;
                }
                KeyCode::Enter => {
                    if *active_field == 0 {
                        let txt = am_buf.trim().to_string();
                        if !txt.is_empty() {
                            submit_am = Some(txt);
                            am_buf.clear();
                        }
                    } else {
                        let txt = pm_buf.trim().to_string();
                        if !txt.is_empty() {
                            submit_pm = Some(txt);
                            pm_buf.clear();
                        }
                    }
                }
                KeyCode::Backspace => {
                    if *active_field == 0 {
                        am_buf.pop();
                    } else {
                        pm_buf.pop();
                    }
                    return;
                }
                KeyCode::Char(c) => {
                    if *active_field == 0 {
                        am_buf.push(c);
                    } else {
                        pm_buf.push(c);
                    }
                    return;
                }
                _ => return,
            }
        }

        if let Some(txt) = submit_am
            && let Ok(mut daily) = self.vault.open_or_create_daily(self.today, self.default_page)
        {
            daily.add_entry_with_modifier(
                Vault::now_hhmm(),
                EntryModifier::Priority,
                Signifier::Action,
                txt,
            );
            if self.vault.save_document(&daily).is_ok() {
                if self.doc.rel_path == daily.rel_path {
                    self.doc = daily;
                }
                self.refresh_metadata();
                self.status_msg = Some("Logged AM Priority Focus (*. ) to Today".into());
            }
        }

        if let Some(txt) = submit_pm
            && let Ok(mut daily) = self.vault.open_or_create_daily(self.today, self.default_page)
        {
            daily.add_entry_with_modifier(
                Vault::now_hhmm(),
                EntryModifier::None,
                Signifier::Emotion,
                txt,
            );
            if self.vault.save_document(&daily).is_ok() {
                if self.doc.rel_path == daily.rel_path {
                    self.doc = daily;
                }
                self.refresh_metadata();
                self.status_msg = Some("Logged PM Reflection (= ) to Today".into());
            }
        }
    }

    fn handle_triage_modal_key(&mut self, key: KeyEvent) {
        let mut action_req: Option<(String, usize, &'static str)> = None;

        if let ModalState::Triage { items, selected } = &mut self.modal {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                    return;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if !items.is_empty() {
                        *selected = (*selected + 1).min(items.len() - 1);
                    }
                    return;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                    return;
                }
                KeyCode::Char('>') | KeyCode::Enter => {
                    if let Some(item) = items.get(*selected) {
                        action_req = Some((item.rel_path.clone(), item.entry.id, "today"));
                    }
                }
                KeyCode::Char('<') => {
                    if let Some(item) = items.get(*selected) {
                        action_req = Some((item.rel_path.clone(), item.entry.id, "future"));
                    }
                }
                KeyCode::Char('x') | KeyCode::Char(' ') => {
                    if let Some(item) = items.get(*selected) {
                        action_req = Some((item.rel_path.clone(), item.entry.id, "done"));
                    }
                }
                KeyCode::Char('~') => {
                    if let Some(item) = items.get(*selected) {
                        action_req = Some((item.rel_path.clone(), item.entry.id, "cancel"));
                    }
                }
                _ => return,
            }
        }

        if let Some((source_rel, entry_id, action)) = action_req {
            match self.vault.triage_open_action(
                &source_rel,
                entry_id,
                action,
                self.today,
                self.default_page,
            ) {
                Ok(()) => {
                    let cur_rel = self.doc.rel_path.clone();
                    if let Ok(reloaded) = self.vault.load_document(&cur_rel) {
                        self.doc = reloaded;
                    }
                    self.refresh_metadata();
                    let next_items = self
                        .vault
                        .collect_open_actions(self.today)
                        .unwrap_or_default();
                    if let ModalState::Triage { items, selected } = &mut self.modal {
                        *items = next_items;
                        if items.is_empty() {
                            *selected = 0;
                        } else {
                            *selected = (*selected).min(items.len() - 1);
                        }
                    }
                    self.status_msg = Some(format!("Triaged task ({action})"));
                }
                Err(e) => self.status_msg = Some(format!("Triage error: {e}")),
            }
        }
    }

    fn handle_global_paper_modal_key(&mut self, key: KeyEvent) {
        let mut sync_one: Option<(String, usize)> = None;
        let mut sync_all = false;

        if let ModalState::GlobalPaper { items, selected } = &mut self.modal {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                    return;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if !items.is_empty() {
                        *selected = (*selected + 1).min(items.len() - 1);
                    }
                    return;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                    return;
                }
                KeyCode::Char('m') | KeyCode::Char(' ') | KeyCode::Enter => {
                    if let Some(item) = items.get(*selected) {
                        sync_one = Some((item.rel_path.clone(), item.entry.id));
                    }
                }
                KeyCode::Char('M') => {
                    sync_all = true;
                }
                _ => return,
            }
        }

        if let Some((rel_path, entry_id)) = sync_one {
            match self.vault.mark_entry_paper_synced(&rel_path, entry_id, true) {
                Ok(()) => {
                    let cur_rel = self.doc.rel_path.clone();
                    if let Ok(reloaded) = self.vault.load_document(&cur_rel) {
                        self.doc = reloaded;
                    }
                    self.refresh_metadata();
                    let next_items = self.vault.collect_paper_pending().unwrap_or_default();
                    if let ModalState::GlobalPaper { items, selected } = &mut self.modal {
                        *items = next_items;
                        if items.is_empty() {
                            *selected = 0;
                        } else {
                            *selected = (*selected).min(items.len() - 1);
                        }
                    }
                    self.status_msg = Some("Marked entry carried to paper [paper:ok]".into());
                }
                Err(e) => self.status_msg = Some(format!("Paper sync error: {e}")),
            }
        } else if sync_all {
            match self.vault.mark_all_vault_paper_synced() {
                Ok(n) => {
                    let cur_rel = self.doc.rel_path.clone();
                    if let Ok(reloaded) = self.vault.load_document(&cur_rel) {
                        self.doc = reloaded;
                    }
                    self.refresh_metadata();
                    self.modal = ModalState::GlobalPaper {
                        items: vec![],
                        selected: 0,
                    };
                    self.status_msg = Some(format!(
                        "Marked {n} entries across vault carried to paper"
                    ));
                }
                Err(e) => self.status_msg = Some(format!("Paper sync error: {e}")),
            }
        }
    }

    fn handle_rename_space_modal_key(&mut self, key: KeyEvent) {
        let mut commit: Option<(String, String)> = None;

        if let ModalState::RenameSpace { rel_path, name_buf } = &mut self.modal {
            match key.code {
                KeyCode::Esc => {
                    self.modal = ModalState::None;
                    return;
                }
                KeyCode::Enter => {
                    let new_name = name_buf.trim().to_string();
                    if !new_name.is_empty() {
                        commit = Some((rel_path.clone(), new_name));
                    } else {
                        self.modal = ModalState::None;
                        return;
                    }
                }
                KeyCode::Backspace => {
                    name_buf.pop();
                    return;
                }
                KeyCode::Char(c) => {
                    name_buf.push(c);
                    return;
                }
                _ => return,
            }
        }

        if let Some((old_rel, new_name)) = commit {
            self.modal = ModalState::None;
            match self.vault.rename_space(&old_rel, &new_name) {
                Ok(doc) => {
                    if self.doc.rel_path == old_rel {
                        self.doc = doc;
                    }
                    self.refresh_metadata();
                    self.status_msg = Some(format!("Renamed space to '{new_name}'"));
                }
                Err(e) => self.status_msg = Some(format!("Rename space failed: {e}")),
            }
        }
    }

    fn handle_delete_space_modal_key(&mut self, key: KeyEvent) {
        let mut confirmed_rel: Option<(String, String)> = None;

        if let ModalState::DeleteSpaceConfirm { rel_path, name } = &self.modal {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    confirmed_rel = Some((rel_path.clone(), name.clone()));
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ModalState::None;
                    return;
                }
                _ => return,
            }
        }

        if let Some((rel_path, name)) = confirmed_rel {
            self.modal = ModalState::None;
            match self.vault.delete_space(&rel_path) {
                Ok(()) => {
                    if self.doc.rel_path == rel_path
                        && let Ok(daily) =
                            self.vault.open_or_create_daily(self.today, self.default_page)
                    {
                        self.doc = daily;
                        self.selected_entry_idx = 0;
                        self.tag_filter = None;
                    }
                    self.active_space_idx = 0;
                    self.refresh_metadata();
                    self.status_msg = Some(format!("Deleted space '{name}'"));
                }
                Err(e) => self.status_msg = Some(format!("Delete space failed: {e}")),
            }
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.editing_entry_id = None;
                self.input_mod = EntryModifier::None;
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
                // Live rapid-log prefix detection:
                // - 2-char: `. `, `o `, `= `, `- `, `* `, `! `
                // - 3-char: `*. `, `!. `, `!- `, `*o `, etc.
                if self.input_buf.ends_with(' ') {
                    let prefix = self.input_buf.trim_end();
                    let chars: Vec<char> = prefix.chars().collect();
                    if chars.len() == 1 {
                        match chars[0] {
                            '*' => {
                                self.input_mod = self.input_mod.toggle_priority();
                                self.input_buf.clear();
                            }
                            '!' => {
                                self.input_mod = self.input_mod.toggle_inspiration();
                                self.input_buf.clear();
                            }
                            ch => {
                                if let Some(sig) = Signifier::from_symbol(&ch.to_string()) {
                                    self.input_sig = sig;
                                    self.input_buf.clear();
                                }
                            }
                        }
                    } else if chars.len() == 2
                        && (chars[0] == '*' || chars[0] == '!')
                        && let Some(sig) = Signifier::from_symbol(&chars[1].to_string())
                    {
                        self.input_mod = if chars[0] == '*' {
                            EntryModifier::Priority
                        } else {
                            EntryModifier::Inspiration
                        };
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
                entry.modifier = self.input_mod;
                entry.signifier = self.input_sig;
                entry.text = text;
                self.dirty = true;
                self.save_current();
            }
        } else {
            let now = Vault::now_hhmm();
            self.doc
                .add_entry_with_modifier(now, self.input_mod, self.input_sig, text);
            self.dirty = true;
            let count = self.filtered_entries().len();
            if count > 0 {
                self.selected_entry_idx = count - 1;
            }
            self.save_current();
        }
        self.input_mod = EntryModifier::None;
        self.input_buf.clear();
        self.sync_list_states();
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
            "thread" => {
                if let Ok(p) = arg.parse::<u32>() {
                    self.doc.add_threaded_page(p);
                    self.dirty = true;
                    self.save_current();
                    self.status_msg = Some(format!("Threaded page p. {p} ({})", self.doc.format_pages()));
                } else {
                    self.status_msg = Some("Usage: :thread <page-number>".into());
                }
            }
            "index" => self.open_master_index(),
            "monthly-log" | "mlog" => self.open_monthly_log(),
            "reflect" => self.open_reflect_modal(),
            "triage" => self.open_triage_modal(),
            "paper" => self.open_global_paper_modal(),
            "rename-space" => {
                if arg.is_empty() {
                    self.status_msg = Some("Usage: :rename-space <New Name>".into());
                } else if !self.doc.rel_path.starts_with("Spaces/") {
                    self.status_msg = Some("Open a custom Space first to rename it".into());
                } else {
                    let old_rel = self.doc.rel_path.clone();
                    match self.vault.rename_space(&old_rel, arg) {
                        Ok(doc) => {
                            self.doc = doc;
                            self.refresh_metadata();
                            self.status_msg = Some(format!("Renamed space to '{arg}'"));
                        }
                        Err(e) => self.status_msg = Some(format!("Rename space failed: {e}")),
                    }
                }
            }
            "delete-space" => {
                if !self.doc.rel_path.starts_with("Spaces/") {
                    self.status_msg = Some("Open a custom Space first to delete it".into());
                } else {
                    let rel = self.doc.rel_path.clone();
                    let name = self.doc.title.clone();
                    self.open_delete_space_modal(rel, name);
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
                                Err(e) => {
                                    self.status_msg = Some(format!("Export write error: {e}"))
                                }
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
                    self.tag_filter = None;
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
                    self.tag_filter = None;
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
                    self.tag_filter = None;
                    self.refresh_metadata();
                    self.status_msg = Some("Opened today's Daily Log".into());
                }
            }
            "date" => {
                if let Ok(d) = NaiveDate::parse_from_str(arg, "%Y-%m-%d") {
                    if let Ok(doc) = self.vault.open_or_create_daily(d, self.default_page) {
                        self.doc = doc;
                        self.selected_entry_idx = 0;
                        self.tag_filter = None;
                        self.refresh_metadata();
                        self.status_msg = Some(format!("Opened {}", d.format("%Y-%m-%d")));
                    }
                } else {
                    self.status_msg = Some("Usage: :date YYYY-MM-DD".into());
                }
            }
            "help" => {
                self.modal = ModalState::Help;
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
            KeyCode::Char('/') => self.open_search_modal(),
            KeyCode::Char('I') => self.open_master_index(),
            KeyCode::Char('L') => self.open_monthly_log(),
            KeyCode::Char('R') => self.open_reflect_modal(),
            KeyCode::Char('T') => self.open_triage_modal(),
            KeyCode::Char('P') => self.open_global_paper_modal(),
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
            KeyCode::Char('r') => {
                if let Some(sp) = self.spaces.get(self.active_space_idx).cloned() {
                    if sp.is_daily {
                        self.status_msg = Some("Daily Log cannot be renamed".into());
                    } else {
                        self.open_rename_space_modal(sp.rel_path, sp.name);
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some(sp) = self.spaces.get(self.active_space_idx).cloned() {
                    if sp.is_daily {
                        self.status_msg = Some("Daily Log cannot be deleted".into());
                    } else {
                        self.open_delete_space_modal(sp.rel_path, sp.name);
                    }
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
            KeyCode::Char('?') => self.modal = ModalState::Help,
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_vault_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.cycle_space(),
            KeyCode::Char('/') => self.open_search_modal(),
            KeyCode::Char('I') => self.open_master_index(),
            KeyCode::Char('L') => self.open_monthly_log(),
            KeyCode::Char('R') => self.open_reflect_modal(),
            KeyCode::Char('T') => self.open_triage_modal(),
            KeyCode::Char('P') => self.open_global_paper_modal(),
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.vault_tree.nodes.is_empty() {
                    self.selected_vault_idx =
                        (self.selected_vault_idx + 1) % self.vault_tree.nodes.len();
                    self.sync_list_states();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.vault_tree.nodes.is_empty() {
                    self.selected_vault_idx = if self.selected_vault_idx == 0 {
                        self.vault_tree.nodes.len() - 1
                    } else {
                        self.selected_vault_idx - 1
                    };
                    self.sync_list_states();
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
            KeyCode::Char('?') => self.modal = ModalState::Help,
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_rapid_log_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => self.cycle_space(),
            KeyCode::Char('/') => self.open_search_modal(),
            KeyCode::Char('I') => self.open_master_index(),
            KeyCode::Char('L') => self.open_monthly_log(),
            KeyCode::Char('R') => self.open_reflect_modal(),
            KeyCode::Char('T') => self.open_triage_modal(),
            KeyCode::Char('P') => self.open_global_paper_modal(),
            KeyCode::Char('[') => self.step_day(-1),
            KeyCode::Char(']') => self.step_day(1),
            KeyCode::Char('t') => self.execute_command("today"),
            KeyCode::Char('q') => {
                if self.dirty {
                    self.save_current();
                }
                self.should_quit = true;
            }
            KeyCode::Char('?') => self.modal = ModalState::Help,
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
                    self.sync_list_states();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_entry_idx = self.selected_entry_idx.saturating_sub(1);
                self.sync_list_states();
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
            // `~` toggles Cancelled / Irrelevant (`~`)
            KeyCode::Char('~') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let cur = self.doc.entries[real_idx].signifier;
                    self.doc.entries[real_idx].signifier = cur.toggle_cancelled();
                    self.dirty = true;
                    self.save_current();
                    self.status_msg = Some("Toggled Cancelled (~)".into());
                }
            }
            // `c` cycles entry base signifier: `.` -> `o` -> `=` -> `-` -> `.`
            KeyCode::Char('c') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let next = self.doc.entries[real_idx].signifier.cycle_base();
                    self.doc.entries[real_idx].signifier = next;
                    self.dirty = true;
                    self.save_current();
                    self.status_msg = Some(format!(
                        "Signifier -> {} ({})",
                        next.symbol(),
                        next.label()
                    ));
                }
            }
            // `}` / `{` indents / outdents nested sub-tasks (0..=3)
            KeyCode::Char('}') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let cur = self.doc.entries[real_idx].indent;
                    if cur < 3 {
                        self.doc.entries[real_idx].indent = cur + 1;
                        self.dirty = true;
                        self.save_current();
                    }
                }
            }
            KeyCode::Char('{') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let cur = self.doc.entries[real_idx].indent;
                    if cur > 0 {
                        self.doc.entries[real_idx].indent = cur - 1;
                        self.dirty = true;
                        self.save_current();
                    }
                }
            }
            // `*` toggles Priority modifier
            KeyCode::Char('*') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let cur = self.doc.entries[real_idx].modifier;
                    self.doc.entries[real_idx].modifier = cur.toggle_priority();
                    self.dirty = true;
                    self.save_current();
                }
            }
            // `!` toggles Inspiration modifier
            KeyCode::Char('!') => {
                if let Some((real_idx, _)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(i, e)| (*i, *e))
                {
                    let cur = self.doc.entries[real_idx].modifier;
                    self.doc.entries[real_idx].modifier = cur.toggle_inspiration();
                    self.dirty = true;
                    self.save_current();
                }
            }
            // `#` cycles inline `#tag` filter on current document
            KeyCode::Char('#') => {
                let tags = self.doc.collect_inline_tags();
                if tags.is_empty() {
                    self.tag_filter = None;
                    self.status_msg = Some("No #tags in this note yet".into());
                } else {
                    self.tag_filter = match &self.tag_filter {
                        None => Some(tags[0].clone()),
                        Some(cur) => {
                            let pos = tags.iter().position(|t| t == cur).unwrap_or(0);
                            if pos + 1 < tags.len() {
                                Some(tags[pos + 1].clone())
                            } else {
                                None
                            }
                        }
                    };
                    self.selected_entry_idx = 0;
                    self.sync_list_states();
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
            // `>` opens Smart Migration Target Picker (Tomorrow / Future Log / any Space)
            KeyCode::Char('>') => {
                self.open_migrate_modal();
            }
            // `<` quick-schedules action directly to Future Log.md
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
                if let Some((id, modifier, sig, text)) = self
                    .filtered_entries()
                    .get(self.selected_entry_idx)
                    .map(|(_, e)| (e.id, e.modifier, e.signifier, e.text.clone()))
                {
                    self.editing_entry_id = Some(id);
                    self.input_mod = modifier;
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
            KeyCode::Char('0') => {
                self.tag_filter = None;
                self.set_filter(EntryFilter::All);
            }
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
        self.sync_list_states();
    }

    fn toggle_filter(&mut self, f: EntryFilter) {
        if self.filter == f {
            self.filter = EntryFilter::All;
        } else {
            self.filter = f;
        }
        self.selected_entry_idx = 0;
        self.sync_list_states();
    }

    fn cycle_space(&mut self) {
        if self.spaces.is_empty() {
            return;
        }
        self.active_space_idx = (self.active_space_idx + 1) % self.spaces.len();
        let rel = self.spaces[self.active_space_idx].rel_path.clone();
        if !Path::new(&self.vault.root).join(&rel).exists()
            && self.spaces[self.active_space_idx].is_daily
        {
            let _ = self.vault.open_or_create_daily(self.today, self.default_page);
        }
        self.open_rel_path(&rel);
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x.saturating_add(r.width) && row >= r.y && row < r.y.saturating_add(r.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tui_rapid_log_modifiers_search_and_day_nav() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(
            dir.path().to_path_buf(),
            &["Studio & Work".into(), "Personal".into()],
        )
        .unwrap();
        let mut app = App::new(vault, 42).unwrap();

        // Simulate rapid log with `*. ` Priority Action prefix detection
        app.focus = FocusPane::Input;
        app.handle_key(KeyEvent::from(KeyCode::Char('*')));
        app.handle_key(KeyEvent::from(KeyCode::Char('.')));
        app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(app.input_mod, EntryModifier::Priority);
        assert_eq!(app.input_sig, Signifier::Action);
        assert!(app.input_buf.is_empty());

        for ch in "Finish studio lighting #studio".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.doc.entries.len(), 1);
        assert_eq!(app.doc.entries[0].modifier, EntryModifier::Priority);
        assert_eq!(app.doc.entries[0].signifier, Signifier::Action);

        // Toggle Inspiration modifier with `!`
        app.focus = FocusPane::RapidLog;
        app.handle_key(KeyEvent::from(KeyCode::Char('!')));
        assert_eq!(app.doc.entries[0].modifier, EntryModifier::Inspiration);

        // Tag filter with `#`
        app.handle_key(KeyEvent::from(KeyCode::Char('#')));
        assert_eq!(app.tag_filter.as_deref(), Some("#studio"));

        // Prev day with `[`
        let original_date = app.doc.date.unwrap();
        app.handle_key(KeyEvent::from(KeyCode::Char('[')));
        assert_eq!(app.doc.date.unwrap(), original_date - Duration::days(1));

        // Search modal with `/` and jump back to `#studio` entry
        app.handle_key(KeyEvent::from(KeyCode::Char('/')));
        for ch in "lighting".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.doc.date.unwrap(), original_date);
        assert!(matches!(app.modal, ModalState::None));

        // Smart migrate modal with `>` -> select a Space (after Tomorrow, Future Log, and 3 month buckets)
        app.handle_key(KeyEvent::from(KeyCode::Char('>')));
        assert!(matches!(app.modal, ModalState::Migrate { .. }));
        for _ in 0..5 {
            app.handle_key(KeyEvent::from(KeyCode::Down));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.doc.entries[0].signifier, Signifier::Migrated);
    }

    #[test]
    fn tui_bujo_index_monthly_reflect_cycle_indent_and_spaces() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(
            dir.path().to_path_buf(),
            &["Studio & Work".into(), "Personal".into()],
        )
        .unwrap();
        let mut app = App::new(vault, 12).unwrap();

        // Add an action entry
        app.doc
            .add_entry("09:00".into(), Signifier::Action, "Draft layout".into());
        app.save_current();
        app.focus = FocusPane::RapidLog;

        // Cycle entry signifier with `c`: Action -> Event -> Emotion -> Note -> Action
        app.handle_key(KeyEvent::from(KeyCode::Char('c')));
        assert_eq!(app.doc.entries[0].signifier, Signifier::Event);
        app.handle_key(KeyEvent::from(KeyCode::Char('c')));
        assert_eq!(app.doc.entries[0].signifier, Signifier::Emotion);
        app.handle_key(KeyEvent::from(KeyCode::Char('c')));
        assert_eq!(app.doc.entries[0].signifier, Signifier::Note);
        app.handle_key(KeyEvent::from(KeyCode::Char('c')));
        assert_eq!(app.doc.entries[0].signifier, Signifier::Action);

        // Indent & outdent with `}` and `{`
        app.handle_key(KeyEvent::from(KeyCode::Char('}')));
        app.handle_key(KeyEvent::from(KeyCode::Char('}')));
        assert_eq!(app.doc.entries[0].indent, 2);
        app.handle_key(KeyEvent::from(KeyCode::Char('{')));
        assert_eq!(app.doc.entries[0].indent, 1);

        // AM / PM Reflection modal (`R`)
        app.handle_key(KeyEvent::from(KeyCode::Char('R')));
        assert!(matches!(app.modal, ModalState::Reflect { .. }));
        for ch in "Ship TUI index".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        for ch in "Calm and focused".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(matches!(app.modal, ModalState::None));
        assert!(app
            .doc
            .entries
            .iter()
            .any(|e| e.modifier == EntryModifier::Priority && e.text == "Ship TUI index"));
        assert!(app
            .doc
            .entries
            .iter()
            .any(|e| e.signifier == Signifier::Emotion && e.text == "Calm and focused"));

        // Master Index (`I`) and Monthly Log (`L`)
        app.handle_key(KeyEvent::from(KeyCode::Char('I')));
        assert_eq!(app.doc.rel_path, "Index.md");
        app.handle_key(KeyEvent::from(KeyCode::Char('L')));
        assert!(app.doc.rel_path.ends_with("-monthly.md"));

        // Rename & Delete Space from Spaces pane (`s` -> `j` -> `r` / `d`)
        app.focus = FocusPane::Spaces;
        app.active_space_idx = 0;
        app.handle_key(KeyEvent::from(KeyCode::Char('j'))); // move to first custom space
        app.handle_key(KeyEvent::from(KeyCode::Char('r')));
        assert!(matches!(app.modal, ModalState::RenameSpace { .. }));
        // Clear existing name buffer and type new name
        for _ in 0..32 {
            app.handle_key(KeyEvent::from(KeyCode::Backspace));
        }
        for ch in "Deep Work".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(ch)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.spaces.iter().any(|s| s.name == "Deep Work"));

        // Delete the renamed space
        let dw_idx = app
            .spaces
            .iter()
            .position(|s| s.name == "Deep Work")
            .unwrap();
        app.active_space_idx = dw_idx;
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(matches!(app.modal, ModalState::DeleteSpaceConfirm { .. }));
        app.handle_key(KeyEvent::from(KeyCode::Char('y')));
        assert!(!app.spaces.iter().any(|s| s.name == "Deep Work"));
    }
}
