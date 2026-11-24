use crate::core::config::{FolioConfig, RtlMode};
use crate::core::markdown::serialize_markdown;
use crate::core::model::{
    Document, Entry, EntryFilter, EntryModifier, SearchHit, Signifier, SpaceInfo, VaultTree,
    is_rtl_text, parse_rapid_input_full,
};
use crate::core::review::{generate_monthly_review, generate_weekly_review};
use crate::core::vault::Vault;
use anyhow::Result;
use chrono::Duration;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    Column, Row, Space, button, column, container, image, row, scrollable, text, text_input,
};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Task, Theme, window};

const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-128.png");

#[derive(Debug, Clone, Copy)]
struct Palette {
    primary: Color,
    on_primary: Color,
    secondary: Color,
    tertiary: Color,
    surface: Color,
    surface_lowest: Color,
    surface_low: Color,
    surface_container: Color,
    surface_high: Color,
    on_surface: Color,
    on_surface_variant: Color,
    outline_variant: Color,
}

impl Palette {
    fn dark() -> Self {
        Self {
            primary: Color::from_rgb8(0xB1, 0xC7, 0xF1),
            on_primary: Color::from_rgb8(0x1A, 0x31, 0x52),
            secondary: Color::from_rgb8(0xFF, 0xB4, 0xA8),
            tertiary: Color::from_rgb8(0xE5, 0xC2, 0x81),
            surface: Color::from_rgb8(0x15, 0x13, 0x0E),
            surface_lowest: Color::from_rgb8(0x10, 0x0E, 0x09),
            surface_low: Color::from_rgb8(0x1E, 0x1B, 0x16),
            surface_container: Color::from_rgb8(0x22, 0x1F, 0x1A),
            surface_high: Color::from_rgb8(0x2C, 0x2A, 0x24),
            on_surface: Color::from_rgb8(0xE8, 0xE2, 0xD9),
            on_surface_variant: Color::from_rgb8(0xC4, 0xC6, 0xCF),
            outline_variant: Color::from_rgb8(0x44, 0x47, 0x4E),
        }
    }

    fn light() -> Self {
        Self {
            primary: Color::from_rgb8(0x0A, 0x23, 0x44),
            on_primary: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            secondary: Color::from_rgb8(0xA0, 0x3F, 0x32),
            tertiary: Color::from_rgb8(0x4B, 0x35, 0x01),
            surface: Color::from_rgb8(0xFF, 0xF8, 0xF0),
            surface_lowest: Color::from_rgb8(0xFF, 0xFF, 0xFF),
            surface_low: Color::from_rgb8(0xF9, 0xF3, 0xEA),
            surface_container: Color::from_rgb8(0xF4, 0xED, 0xE4),
            surface_high: Color::from_rgb8(0xEE, 0xE7, 0xDE),
            on_surface: Color::from_rgb8(0x1E, 0x1B, 0x16),
            on_surface_variant: Color::from_rgb8(0x44, 0x47, 0x4E),
            outline_variant: Color::from_rgb8(0xC4, 0xC6, 0xCF),
        }
    }
}

struct LaunchContext {
    vault: Vault,
    config: FolioConfig,
}

struct FolioApp {
    vault: Vault,
    config: FolioConfig,
    doc: Document,
    spaces: Vec<SpaceInfo>,
    vault_tree: VaultTree,
    raw_markdown: String,
    inline_tags: Vec<String>,
    icon_handle: image::Handle,

    // UI preferences
    rtl_mode: RtlMode,
    is_dark: bool,
    show_md_preview: bool,

    // Composer state
    composer_text: String,
    composer_sig: Signifier,
    composer_mod: EntryModifier,

    // Filters
    filter: EntryFilter,
    tag_filter: Option<String>,

    // Inline editing & migration
    editing_entry_id: Option<usize>,
    editing_text: String,
    migrating_entry_id: Option<usize>,

    // New space, threading & search
    new_space_name: String,
    thread_page_input: String,
    search_query: String,
    search_hits: Vec<SearchHit>,

    // Status banner
    status_msg: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    // Preferences
    SetRtlMode(RtlMode),
    ToggleTheme,
    ToggleMdPreview,

    // Navigation
    OpenDoc(String),
    OpenMasterIndex,
    NavDay(i64),
    AdjustPage(i32),
    ThreadPageChanged(String),
    AddThreadedPage,
    GenerateReview(bool), // true = weekly, false = monthly

    // Composer
    ComposerChanged(String),
    SetComposerSig(Signifier),
    ToggleComposerMod(EntryModifier),
    SubmitEntry,

    // Filters
    SetFilter(EntryFilter),
    ToggleTagFilter(String),

    // Entry actions
    ToggleEntryDone(usize),
    ToggleEntryCancelled(usize),
    ToggleEntryPaper(usize),
    ToggleEntryPriority(usize),
    ToggleEntryInspiration(usize),
    DeleteEntry(usize),
    MarkAllPaperSynced,

    // Inline Edit
    StartEdit(usize, String),
    EditTextChanged(String),
    SaveEdit,
    CancelEdit,

    // Migration
    ToggleMigratePicker(usize),
    MigrateTo {
        entry_id: usize,
        target_rel: String,
        scheduled: bool,
    },

    // Spaces & Search
    NewSpaceChanged(String),
    CreateSpace,
    SearchChanged(String),
    ClearSearch,
}

pub fn run_native_gui(vault: Vault, config: FolioConfig) -> Result<()> {
    let icon = window::icon::from_file_data(ICON_PNG, None).ok();
    let launch = std::sync::Mutex::new(Some(LaunchContext { vault, config }));

    iced::application(
        move || {
            let ctx = launch
                .lock()
                .expect("launch context lock")
                .take()
                .expect("boot called once");
            FolioApp::new(ctx.vault, ctx.config)
        },
        FolioApp::update,
        FolioApp::view,
    )
    .title(FolioApp::title)
    .theme(FolioApp::theme)
    .window(window::Settings {
        size: iced::Size::new(1260.0, 820.0),
        min_size: Some(iced::Size::new(920.0, 620.0)),
        icon,
        ..Default::default()
    })
    .run()
    .map_err(|e| anyhow::anyhow!("Native GUI error: {e}"))
}

impl FolioApp {
    fn new(vault: Vault, config: FolioConfig) -> Self {
        let today = Vault::today_date();
        let doc = vault
            .open_or_create_daily(today, config.notebook_page)
            .expect("failed to open today's daily note");
        let spaces = vault.list_spaces(today).unwrap_or_default();
        let vault_tree = vault.vault_tree().unwrap_or(VaultTree {
            vault_name: "Vault".to_string(),
            vault_root: vault.display_root(),
            nodes: vec![],
        });
        let raw_markdown = serialize_markdown(&doc);
        let inline_tags = doc.collect_inline_tags();
        let rtl_mode = config.rtl_mode;
        let is_dark = config.gui_theme != "light";

        Self {
            vault,
            config,
            doc,
            spaces,
            vault_tree,
            raw_markdown,
            inline_tags,
            icon_handle: image::Handle::from_bytes(ICON_PNG.to_vec()),
            rtl_mode,
            is_dark,
            show_md_preview: true,
            composer_text: String::new(),
            composer_sig: Signifier::Action,
            composer_mod: EntryModifier::None,
            filter: EntryFilter::All,
            tag_filter: None,
            editing_entry_id: None,
            editing_text: String::new(),
            migrating_entry_id: None,
            new_space_name: String::new(),
            thread_page_input: String::new(),
            search_query: String::new(),
            search_hits: Vec::new(),
            status_msg: None,
        }
    }

    fn title(&self) -> String {
        format!("Folio v{} — {}", env!("CARGO_PKG_VERSION"), self.doc.title)
    }

    fn theme(&self) -> Theme {
        let p = self.palette();
        Theme::custom(
            if self.is_dark {
                "Folio Dark".to_string()
            } else {
                "Folio Light".to_string()
            },
            iced::theme::Palette {
                background: p.surface,
                text: p.on_surface,
                primary: p.primary,
                success: p.tertiary,
                warning: p.tertiary,
                danger: p.secondary,
            },
        )
    }

    fn palette(&self) -> Palette {
        if self.is_dark {
            Palette::dark()
        } else {
            Palette::light()
        }
    }

    fn is_ui_rtl(&self) -> bool {
        self.rtl_mode == RtlMode::Rtl
    }

    fn is_text_rtl(&self, sample: &str) -> bool {
        match self.rtl_mode {
            RtlMode::Ltr => false,
            RtlMode::Rtl => true,
            RtlMode::Auto => is_rtl_text(sample),
        }
    }

    fn refresh_metadata(&mut self) {
        let today = Vault::today_date();
        self.raw_markdown = serialize_markdown(&self.doc);
        self.inline_tags = self.doc.collect_inline_tags();
        if let Ok(spaces) = self.vault.list_spaces(today) {
            self.spaces = spaces;
        }
        if let Ok(tree) = self.vault.vault_tree() {
            self.vault_tree = tree;
        }
    }

    fn save_current_doc(&mut self) {
        if let Err(e) = self.vault.save_document(&self.doc) {
            self.status_msg = Some(format!("Save error: {e}"));
        } else {
            self.refresh_metadata();
        }
    }

    fn persist_config(&mut self) {
        self.config.rtl_mode = self.rtl_mode;
        self.config.gui_theme = if self.is_dark { "dark" } else { "light" }.to_string();
        let _ = self.config.save();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        self.status_msg = None;

        match message {
            Message::SetRtlMode(mode) => {
                self.rtl_mode = mode;
                self.persist_config();
                self.status_msg = Some(match mode {
                    RtlMode::Auto => "Direction: AUTO (detects RTL per entry)".to_string(),
                    RtlMode::Rtl => "Direction: RTL (full mirrored layout)".to_string(),
                    RtlMode::Ltr => "Direction: LTR (standard left-to-right)".to_string(),
                });
            }
            Message::ToggleTheme => {
                self.is_dark = !self.is_dark;
                self.persist_config();
            }
            Message::ToggleMdPreview => {
                self.show_md_preview = !self.show_md_preview;
            }
            Message::OpenDoc(rel_path) => match self.vault.load_document(&rel_path) {
                Ok(doc) => {
                    self.doc = doc;
                    self.editing_entry_id = None;
                    self.migrating_entry_id = None;
                    self.tag_filter = None;
                    self.search_query.clear();
                    self.search_hits.clear();
                    self.refresh_metadata();
                }
                Err(e) => {
                    self.status_msg = Some(format!("Could not open {rel_path}: {e}"));
                }
            },
            Message::OpenMasterIndex => match self.vault.update_master_index() {
                Ok(doc) => {
                    self.doc = doc;
                    self.editing_entry_id = None;
                    self.migrating_entry_id = None;
                    self.tag_filter = None;
                    self.refresh_metadata();
                    self.status_msg = Some("Refreshed Master Index (Index.md)".to_string());
                }
                Err(e) => {
                    self.status_msg = Some(format!("Master Index error: {e}"));
                }
            },
            Message::NavDay(delta) => {
                let target_date = if delta == 0 {
                    Vault::today_date()
                } else {
                    self.doc.date.unwrap_or_else(Vault::today_date) + Duration::days(delta)
                };
                match self
                    .vault
                    .open_or_create_daily(target_date, self.config.notebook_page)
                {
                    Ok(doc) => {
                        self.doc = doc;
                        self.editing_entry_id = None;
                        self.migrating_entry_id = None;
                        self.tag_filter = None;
                        self.refresh_metadata();
                    }
                    Err(e) => self.status_msg = Some(format!("Day navigation failed: {e}")),
                }
            }
            Message::AdjustPage(delta) => {
                let current = self.doc.page.unwrap_or(self.config.notebook_page) as i32;
                let next = (current + delta).max(1) as u32;
                self.doc.page = Some(next);
                self.config.notebook_page = next;
                self.persist_config();
                self.save_current_doc();
            }
            Message::ThreadPageChanged(val) => {
                self.thread_page_input = val;
            }
            Message::AddThreadedPage => {
                let target_page = self
                    .thread_page_input
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|&p| p > 0)
                    .unwrap_or(self.config.notebook_page);
                self.doc.add_threaded_page(target_page);
                self.thread_page_input.clear();
                self.save_current_doc();
                self.status_msg = Some(format!(
                    "Threaded page p.{target_page} ({})",
                    self.doc.format_pages()
                ));
            }
            Message::GenerateReview(weekly) => {
                let today = Vault::today_date();
                let res = if weekly {
                    generate_weekly_review(&self.vault, today)
                } else {
                    generate_monthly_review(&self.vault, today)
                };
                match res {
                    Ok((doc, _summary)) => {
                        self.doc = doc;
                        self.editing_entry_id = None;
                        self.migrating_entry_id = None;
                        self.refresh_metadata();
                        self.status_msg = Some(format!(
                            "Generated {} review",
                            if weekly { "Weekly" } else { "Monthly" }
                        ));
                    }
                    Err(e) => self.status_msg = Some(format!("Review failed: {e}")),
                }
            }
            Message::ComposerChanged(val) => {
                self.composer_text = val;
            }
            Message::SetComposerSig(sig) => {
                self.composer_sig = sig;
            }
            Message::ToggleComposerMod(modifier) => {
                self.composer_mod = if self.composer_mod == modifier {
                    EntryModifier::None
                } else {
                    modifier
                };
            }
            Message::SubmitEntry => {
                let raw = self.composer_text.trim().to_string();
                if !raw.is_empty() {
                    let (parsed_mod, sig, clean_text) =
                        parse_rapid_input_full(&raw, self.composer_sig);
                    let effective_mod = if parsed_mod != EntryModifier::None {
                        parsed_mod
                    } else {
                        self.composer_mod
                    };
                    if !clean_text.is_empty() {
                        self.doc.add_entry_with_modifier(
                            Vault::now_hhmm(),
                            effective_mod,
                            sig,
                            clean_text,
                        );
                        self.composer_text.clear();
                        self.composer_mod = EntryModifier::None;
                        self.save_current_doc();
                    }
                }
            }
            Message::SetFilter(f) => {
                self.filter = f;
            }
            Message::ToggleTagFilter(tag) => {
                if self.tag_filter.as_deref() == Some(tag.as_str()) {
                    self.tag_filter = None;
                } else {
                    self.tag_filter = Some(tag);
                }
            }
            Message::ToggleEntryDone(id) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    entry.signifier = entry.signifier.toggle_done();
                    self.save_current_doc();
                }
            }
            Message::ToggleEntryCancelled(id) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    entry.signifier = entry.signifier.toggle_cancelled();
                    self.save_current_doc();
                }
            }
            Message::ToggleEntryPaper(id) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    entry.paper_synced = !entry.paper_synced;
                    self.save_current_doc();
                }
            }
            Message::ToggleEntryPriority(id) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    entry.modifier = entry.modifier.toggle_priority();
                    self.save_current_doc();
                }
            }
            Message::ToggleEntryInspiration(id) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    entry.modifier = entry.modifier.toggle_inspiration();
                    self.save_current_doc();
                }
            }
            Message::DeleteEntry(id) => {
                self.doc.entries.retain(|e| e.id != id);
                if self.editing_entry_id == Some(id) {
                    self.editing_entry_id = None;
                }
                if self.migrating_entry_id == Some(id) {
                    self.migrating_entry_id = None;
                }
                self.save_current_doc();
            }
            Message::MarkAllPaperSynced => {
                for entry in &mut self.doc.entries {
                    entry.paper_synced = true;
                }
                self.save_current_doc();
                self.status_msg = Some("All entries marked carried to paper".to_string());
            }
            Message::StartEdit(id, text_val) => {
                self.editing_entry_id = Some(id);
                self.editing_text = text_val;
                self.migrating_entry_id = None;
            }
            Message::EditTextChanged(val) => {
                self.editing_text = val;
            }
            Message::SaveEdit => {
                if let Some(id) = self.editing_entry_id.take() {
                    let trimmed = self.editing_text.trim().to_string();
                    if !trimmed.is_empty()
                        && let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id)
                    {
                        entry.text = trimmed;
                        self.save_current_doc();
                    }
                    self.editing_text.clear();
                }
            }
            Message::CancelEdit => {
                self.editing_entry_id = None;
                self.editing_text.clear();
            }
            Message::ToggleMigratePicker(id) => {
                if self.migrating_entry_id == Some(id) {
                    self.migrating_entry_id = None;
                } else {
                    self.migrating_entry_id = Some(id);
                    self.editing_entry_id = None;
                }
            }
            Message::MigrateTo {
                entry_id,
                target_rel,
                scheduled,
            } => {
                match self
                    .vault
                    .migrate_entry(&mut self.doc, entry_id, &target_rel, scheduled)
                {
                    Ok(()) => {
                        self.migrating_entry_id = None;
                        self.refresh_metadata();
                        self.status_msg = Some(format!("Migrated entry to {target_rel}"));
                    }
                    Err(e) => self.status_msg = Some(format!("Migration failed: {e}")),
                }
            }
            Message::NewSpaceChanged(val) => {
                self.new_space_name = val;
            }
            Message::CreateSpace => {
                let name = self.new_space_name.trim().to_string();
                if !name.is_empty() {
                    match self.vault.create_space(&name) {
                        Ok(doc) => {
                            self.doc = doc;
                            self.new_space_name.clear();
                            self.refresh_metadata();
                            self.status_msg = Some(format!("Opened Space '{name}'"));
                        }
                        Err(e) => self.status_msg = Some(format!("Space error: {e}")),
                    }
                }
            }
            Message::SearchChanged(q) => {
                self.search_query = q.clone();
                if q.trim().is_empty() {
                    self.search_hits.clear();
                } else if let Ok(hits) = self.vault.search_vault(&q) {
                    self.search_hits = hits.into_iter().take(12).collect();
                }
            }
            Message::ClearSearch => {
                self.search_query.clear();
                self.search_hits.clear();
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let p = self.palette();
        let ui_rtl = self.is_ui_rtl();

        let top_bar = self.view_top_bar(p, ui_rtl);
        let left_nav = self.view_nav_pane(p, ui_rtl);
        let center_log = self.view_rapid_log_pane(p, ui_rtl);
        let right_inspector = self.view_inspector_pane(p, ui_rtl);

        let body: Row<'_, Message> = if ui_rtl {
            row![right_inspector, center_log, left_nav]
                .spacing(10)
                .height(Length::Fill)
        } else {
            row![left_nav, center_log, right_inspector]
                .spacing(10)
                .height(Length::Fill)
        };

        let mut root_col = column![top_bar].spacing(10);

        if !self.search_hits.is_empty() {
            root_col = root_col.push(self.view_search_results(p, ui_rtl));
        }

        if let Some(msg) = &self.status_msg {
            root_col = root_col.push(
                container(
                    text(msg.clone())
                        .size(12)
                        .color(p.tertiary)
                        .shaping(text::Shaping::Advanced),
                )
                .width(Length::Fill)
                .padding([4, 12])
                .style(move |_| pane_style(p.surface_container, p.outline_variant)),
            );
        }

        root_col = root_col.push(body);

        // Footer status bar with version, vault root, and direction mode
        let footer = container(
            row![
                text(format!("Folio v{}", env!("CARGO_PKG_VERSION")))
                    .size(11)
                    .color(p.primary),
                text(format!("· Vault: {}", self.vault.display_root()))
                    .size(11)
                    .color(p.on_surface_variant)
                    .shaping(text::Shaping::Advanced),
                Space::new().width(Length::Fill),
                text(format!(
                    "Dir: {} · {}",
                    self.rtl_mode.label(),
                    if self.is_dark { "Dark" } else { "Light" }
                ))
                .size(11)
                .color(p.on_surface_variant),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([4, 10])
        .style(move |_| pane_style(p.surface_low, p.outline_variant));

        root_col = root_col.push(footer);

        container(root_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(12)
            .style(move |_| container::Style {
                background: Some(p.surface.into()),
                ..Default::default()
            })
            .into()
    }

    fn view_top_bar(&self, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        let icon_img = image(self.icon_handle.clone()).width(22).height(22);
        let brand_title = text("FOLIO")
            .size(14)
            .color(p.primary)
            .shaping(text::Shaping::Advanced);
        let version_badge = container(
            text(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(11)
                .color(p.primary),
        )
        .padding([2, 6])
        .style(move |_| badge_style(p));

        let path_label = text(self.vault.display_doc_path(&self.doc.rel_path))
            .size(11)
            .color(p.on_surface_variant)
            .shaping(text::Shaping::Advanced);

        let brand_group = if ui_rtl {
            row![path_label, version_badge, brand_title, icon_img]
                .spacing(8)
                .align_y(Alignment::Center)
        } else {
            row![icon_img, brand_title, version_badge, path_label]
                .spacing(8)
                .align_y(Alignment::Center)
        };

        // RTL mode segmented toggle: LTR | AUTO | RTL
        let rtl_toggle = row![
            self.rtl_pill(p, RtlMode::Ltr),
            self.rtl_pill(p, RtlMode::Auto),
            self.rtl_pill(p, RtlMode::Rtl),
        ]
        .spacing(3)
        .align_y(Alignment::Center);

        let theme_btn = pill_button(
            if self.is_dark { "Light" } else { "Dark" },
            false,
            p,
            Message::ToggleTheme,
        );

        let day_nav = row![
            pill_button("‹ Prev", false, p, Message::NavDay(-1)),
            pill_button("Today", false, p, Message::NavDay(0)),
            pill_button("Next ›", false, p, Message::NavDay(1)),
        ]
        .spacing(4);

        let page_num = self.doc.page.unwrap_or(self.config.notebook_page);
        let page_ctrl = row![
            pill_button("-", false, p, Message::AdjustPage(-1)),
            container(text(format!("p.{page_num}")).size(11).color(p.tertiary))
                .padding([3, 7])
                .style(move |_| badge_style(p)),
            pill_button("+", false, p, Message::AdjustPage(1)),
        ]
        .spacing(3)
        .align_y(Alignment::Center);

        let reviews = row![
            pill_button(
                "Index",
                self.doc.rel_path.eq_ignore_ascii_case("Index.md"),
                p,
                Message::OpenMasterIndex,
            ),
            pill_button("Weekly", false, p, Message::GenerateReview(true)),
            pill_button("Monthly", false, p, Message::GenerateReview(false)),
        ]
        .spacing(4);

        let search_input = text_input("Search vault or #tag...", &self.search_query)
            .on_input(Message::SearchChanged)
            .size(12)
            .padding([4, 8])
            .width(Length::Fixed(185.0))
            .style(move |_, status| input_style(p, status));

        let controls = if ui_rtl {
            row![
                search_input,
                reviews,
                page_ctrl,
                day_nav,
                theme_btn,
                rtl_toggle,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        } else {
            row![
                day_nav,
                page_ctrl,
                reviews,
                rtl_toggle,
                theme_btn,
                search_input,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };

        let bar_row = if ui_rtl {
            row![controls, Space::new().width(Length::Fill), brand_group]
                .spacing(12)
                .align_y(Alignment::Center)
        } else {
            row![brand_group, Space::new().width(Length::Fill), controls]
                .spacing(12)
                .align_y(Alignment::Center)
        };

        container(bar_row)
            .width(Length::Fill)
            .padding([8, 12])
            .style(move |_| pane_style(p.surface_low, p.outline_variant))
            .into()
    }

    fn rtl_pill(&self, p: Palette, mode: RtlMode) -> Element<'_, Message> {
        let active = self.rtl_mode == mode;
        pill_button(mode.label(), active, p, Message::SetRtlMode(mode))
    }

    fn view_search_results(&self, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        let mut col = Column::new().spacing(4);
        let header = row![
            text(format!("Search results ({})", self.search_hits.len()))
                .size(12)
                .color(p.primary),
            Space::new().width(Length::Fill),
            pill_button("Close", false, p, Message::ClearSearch),
        ]
        .align_y(Alignment::Center);
        col = col.push(header);

        for hit in &self.search_hits {
            let hit_rtl = self.is_text_rtl(&hit.snippet);
            let label = format!("{} — {}", hit.doc_title, hit.snippet);
            let btn_content = text(label)
                .size(12)
                .color(p.on_surface)
                .shaping(text::Shaping::Advanced)
                .width(Length::Fill)
                .align_x(if hit_rtl || ui_rtl {
                    Horizontal::Right
                } else {
                    Horizontal::Left
                });

            col = col.push(
                button(btn_content)
                    .width(Length::Fill)
                    .padding([5, 8])
                    .style(move |_, status| list_item_style(p, false, status))
                    .on_press(Message::OpenDoc(hit.rel_path.clone())),
            );
        }

        container(scrollable(col).height(Length::Fixed(180.0)))
            .width(Length::Fill)
            .padding(8)
            .style(move |_| pane_style(p.surface_low, p.primary))
            .into()
    }

    fn view_nav_pane(&self, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        // [1] SPACES
        let mut spaces_col = Column::new().spacing(4);
        spaces_col = spaces_col.push(section_header("[1] SPACES", p.primary, ui_rtl));

        for sp in &self.spaces {
            let active = self.doc.rel_path == sp.rel_path;
            let count_txt = text(format!("{}", sp.entry_count))
                .size(11)
                .color(if active {
                    p.primary
                } else {
                    p.on_surface_variant
                });
            let name_txt = text(sp.name.clone())
                .size(12)
                .color(if active { p.primary } else { p.on_surface })
                .shaping(text::Shaping::Advanced);

            let item_row = if ui_rtl || self.is_text_rtl(&sp.name) {
                row![count_txt, Space::new().width(Length::Fill), name_txt]
                    .align_y(Alignment::Center)
            } else {
                row![name_txt, Space::new().width(Length::Fill), count_txt]
                    .align_y(Alignment::Center)
            };

            spaces_col = spaces_col.push(
                button(item_row)
                    .width(Length::Fill)
                    .padding([5, 8])
                    .style(move |_, status| list_item_style(p, active, status))
                    .on_press(Message::OpenDoc(sp.rel_path.clone())),
            );
        }

        // New Space creator
        let new_space_in = text_input("New space...", &self.new_space_name)
            .on_input(Message::NewSpaceChanged)
            .on_submit(Message::CreateSpace)
            .size(11)
            .padding([4, 6])
            .style(move |_, status| input_style(p, status));
        let add_space_btn = pill_button("+", false, p, Message::CreateSpace);
        spaces_col = spaces_col.push(
            row![new_space_in, add_space_btn]
                .spacing(4)
                .align_y(Alignment::Center),
        );

        let spaces_card = container(spaces_col)
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.outline_variant));

        // [2] VAULT TREE
        let mut vault_col = Column::new().spacing(2);
        vault_col = vault_col.push(section_header("[2] VAULT", p.primary, ui_rtl));

        for node in &self.vault_tree.nodes {
            if node.is_dir {
                vault_col = vault_col.push(
                    container(
                        text(format!("▸ {}", node.label))
                            .size(11)
                            .color(p.on_surface_variant)
                            .shaping(text::Shaping::Advanced)
                            .width(Length::Fill)
                            .align_x(if ui_rtl {
                                Horizontal::Right
                            } else {
                                Horizontal::Left
                            }),
                    )
                    .padding(Padding {
                        top: 6.0,
                        right: 4.0,
                        bottom: 2.0,
                        left: 4.0,
                    }),
                );
            } else {
                let active = self.doc.rel_path == node.rel_path;
                let indent = (node.depth as f32) * 10.0;
                let pad = if ui_rtl {
                    Padding {
                        top: 4.0,
                        right: 6.0 + indent,
                        bottom: 4.0,
                        left: 6.0,
                    }
                } else {
                    Padding {
                        top: 4.0,
                        right: 6.0,
                        bottom: 4.0,
                        left: 6.0 + indent,
                    }
                };

                vault_col = vault_col.push(
                    button(
                        text(node.label.clone())
                            .size(11)
                            .color(if active { p.primary } else { p.on_surface })
                            .shaping(text::Shaping::Advanced)
                            .width(Length::Fill)
                            .align_x(if ui_rtl {
                                Horizontal::Right
                            } else {
                                Horizontal::Left
                            }),
                    )
                    .width(Length::Fill)
                    .padding(pad)
                    .style(move |_, status| list_item_style(p, active, status))
                    .on_press(Message::OpenDoc(node.rel_path.clone())),
                );
            }
        }

        let vault_card = container(scrollable(vault_col))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.outline_variant));

        column![spaces_card, vault_card]
            .spacing(10)
            .width(Length::Fixed(245.0))
            .height(Length::Fill)
            .into()
    }

    fn view_rapid_log_pane(&self, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        let doc_rtl = ui_rtl || self.is_text_rtl(&self.doc.title);
        let title_txt = text(self.doc.title.clone())
            .size(18)
            .color(p.primary)
            .shaping(text::Shaping::Advanced);

        let pending = self.doc.paper_pending_count();
        let pages_str = self.doc.format_pages();
        let meta_txt = text(format!(
            "{} · {} entries · {} pending paper",
            pages_str,
            self.doc.entries.len(),
            pending
        ))
        .size(11)
        .color(p.on_surface_variant);

        let thread_in = text_input("p.#", &self.thread_page_input)
            .on_input(Message::ThreadPageChanged)
            .on_submit(Message::AddThreadedPage)
            .size(11)
            .padding([3, 6])
            .width(Length::Fixed(52.0))
            .style(move |_, status| input_style(p, status));
        let thread_btn = pill_button("+ Thread", false, p, Message::AddThreadedPage);
        let thread_ctrl = row![thread_in, thread_btn]
            .spacing(4)
            .align_y(Alignment::Center);

        let doc_header = if doc_rtl {
            row![
                thread_ctrl,
                meta_txt,
                Space::new().width(Length::Fill),
                title_txt
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        } else {
            row![
                title_txt,
                Space::new().width(Length::Fill),
                meta_txt,
                thread_ctrl
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };

        // Filter bar (Signifiers + Inline #tags)
        let mut filter_row = Row::new().spacing(4).align_y(Alignment::Center);
        for f in [
            EntryFilter::All,
            EntryFilter::Action,
            EntryFilter::Event,
            EntryFilter::Emotion,
            EntryFilter::Note,
        ] {
            filter_row = filter_row.push(pill_button(
                f.label(),
                self.filter == f,
                p,
                Message::SetFilter(f),
            ));
        }

        let mut tags_row = Row::new().spacing(4).align_y(Alignment::Center);
        for tag in &self.inline_tags {
            let active = self.tag_filter.as_deref() == Some(tag.as_str());
            tags_row = tags_row.push(pill_button(
                tag,
                active,
                p,
                Message::ToggleTagFilter(tag.clone()),
            ));
        }

        // Composer
        let composer = self.view_composer(p, ui_rtl);

        // Entries list
        let filtered_entries: Vec<&Entry> = self
            .doc
            .entries
            .iter()
            .filter(|e| e.signifier.matches_filter(self.filter))
            .filter(|e| {
                self.tag_filter
                    .as_deref()
                    .map(|t| e.has_tag(t))
                    .unwrap_or(true)
            })
            .collect();

        let mut entries_col = Column::new().spacing(4);

        // Prose lines (for Reviews / Future Log)
        if !self.doc.prose_lines.is_empty() {
            let mut prose_box = Column::new().spacing(3);
            for line in &self.doc.prose_lines {
                if !line.trim().is_empty() {
                    let line_rtl = self.is_text_rtl(line);
                    prose_box = prose_box.push(
                        text(line.clone())
                            .size(12)
                            .color(p.on_surface_variant)
                            .shaping(text::Shaping::Advanced)
                            .width(Length::Fill)
                            .align_x(if line_rtl {
                                Horizontal::Right
                            } else {
                                Horizontal::Left
                            }),
                    );
                }
            }
            entries_col = entries_col.push(
                container(prose_box)
                    .width(Length::Fill)
                    .padding(8)
                    .style(move |_| pane_style(p.surface_container, p.outline_variant)),
            );
        }

        if filtered_entries.is_empty() {
            entries_col = entries_col.push(
                container(
                    text("No entries yet. Use the rapid-log bar above to add one (e.g. `. Finish sketch` or Persian/Arabic text).")
                        .size(12)
                        .color(p.on_surface_variant)
                        .shaping(text::Shaping::Advanced)
                        .width(Length::Fill)
                        .align_x(if ui_rtl { Horizontal::Right } else { Horizontal::Left }),
                )
                .padding(16),
            );
        } else {
            for entry in filtered_entries {
                entries_col = entries_col.push(self.view_entry_row(entry, p));
                if self.migrating_entry_id == Some(entry.id) {
                    entries_col = entries_col.push(self.view_migrate_bar(entry.id, p, ui_rtl));
                }
            }
        }

        let mut main_col = column![doc_header, filter_row].spacing(8);
        if !self.inline_tags.is_empty() {
            main_col = main_col.push(tags_row);
        }
        main_col = main_col.push(composer);
        main_col = main_col.push(scrollable(entries_col).height(Length::Fill));

        container(main_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(14)
            .style(move |_| pane_style(p.surface_lowest, p.outline_variant))
            .into()
    }

    fn view_composer(&self, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        let sig_pills = row![
            pill_button(
                ".",
                self.composer_sig == Signifier::Action,
                p,
                Message::SetComposerSig(Signifier::Action),
            ),
            pill_button(
                "o",
                self.composer_sig == Signifier::Event,
                p,
                Message::SetComposerSig(Signifier::Event),
            ),
            pill_button(
                "=",
                self.composer_sig == Signifier::Emotion,
                p,
                Message::SetComposerSig(Signifier::Emotion),
            ),
            pill_button(
                "-",
                self.composer_sig == Signifier::Note,
                p,
                Message::SetComposerSig(Signifier::Note),
            ),
            pill_button(
                "*",
                self.composer_mod == EntryModifier::Priority,
                p,
                Message::ToggleComposerMod(EntryModifier::Priority),
            ),
            pill_button(
                "!",
                self.composer_mod == EntryModifier::Inspiration,
                p,
                Message::ToggleComposerMod(EntryModifier::Inspiration),
            ),
        ]
        .spacing(3);

        let composer_rtl = self.is_text_rtl(&self.composer_text);
        let placeholder = if ui_rtl {
            "ثبت سریع... (یا . برای کار، o برای رویداد، = برای حس، - برای یادداشت)"
        } else {
            "Rapid log... (prefix `. `, `o `, `= `, `- `, `*. `, `!- ` or type RTL directly)"
        };

        let input = text_input(placeholder, &self.composer_text)
            .on_input(Message::ComposerChanged)
            .on_submit(Message::SubmitEntry)
            .size(13)
            .padding([6, 10])
            .align_x(if composer_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            })
            .style(move |_, status| input_style(p, status));

        let submit_btn = button(
            text("+ Log")
                .size(12)
                .color(p.on_primary)
                .shaping(text::Shaping::Advanced),
        )
        .padding([6, 12])
        .style(move |_, status| primary_btn_style(p, status))
        .on_press(Message::SubmitEntry);

        let composer_row = if ui_rtl {
            row![submit_btn, input, sig_pills]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![sig_pills, input, submit_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        container(composer_row)
            .width(Length::Fill)
            .padding(8)
            .style(move |_| pane_style(p.surface_container, p.outline_variant))
            .into()
    }

    fn view_entry_row(&self, entry: &Entry, p: Palette) -> Element<'_, Message> {
        let entry_rtl = self.is_text_rtl(&entry.text);

        if self.editing_entry_id == Some(entry.id) {
            let edit_in = text_input("Edit entry...", &self.editing_text)
                .on_input(Message::EditTextChanged)
                .on_submit(Message::SaveEdit)
                .size(13)
                .padding([5, 8])
                .align_x(if self.is_text_rtl(&self.editing_text) {
                    Horizontal::Right
                } else {
                    Horizontal::Left
                })
                .style(move |_, status| input_style(p, status));

            return container(
                row![
                    edit_in,
                    pill_button("Save", true, p, Message::SaveEdit),
                    pill_button("Cancel", false, p, Message::CancelEdit),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([6, 8])
            .style(move |_| pane_style(p.surface_high, p.primary))
            .into();
        }

        let time_txt = text(format!("[{}]", entry.time))
            .size(11)
            .color(p.on_surface_variant);

        let mod_color = match entry.modifier {
            EntryModifier::Priority => p.tertiary,
            EntryModifier::Inspiration => p.secondary,
            EntryModifier::None => p.on_surface_variant,
        };
        let mod_txt = text(entry.modifier.symbol())
            .size(13)
            .color(mod_color)
            .width(Length::Fixed(10.0));

        let sig_color = match entry.signifier {
            Signifier::Action => p.primary,
            Signifier::Done => p.tertiary,
            Signifier::Event => p.secondary,
            Signifier::Emotion => p.tertiary,
            Signifier::Note => p.on_surface_variant,
            Signifier::Migrated | Signifier::Scheduled => p.primary,
            Signifier::Cancelled => p.on_surface_variant,
        };

        let sig_btn = button(
            text(entry.signifier.symbol())
                .size(14)
                .color(sig_color)
                .align_x(Horizontal::Center)
                .width(Length::Fixed(18.0)),
        )
        .padding([2, 3])
        .style(move |_, status| subtle_btn_style(p, status))
        .on_press(Message::ToggleEntryDone(entry.id));

        let text_color = if entry.signifier == Signifier::Done
            || entry.signifier == Signifier::Cancelled
        {
            p.on_surface_variant
        } else {
            p.on_surface
        };

        let body_display = if entry.signifier == Signifier::Cancelled {
            format!("~~ {} ~~", entry.text)
        } else {
            entry.text.clone()
        };

        let body_txt = text(body_display)
            .size(13)
            .color(text_color)
            .shaping(text::Shaping::Advanced)
            .width(Length::Fill)
            .align_x(if entry_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            });

        let paper_label = if entry.paper_synced { "ok" } else { "paper" };
        let actions = row![
            mini_btn(
                "~",
                entry.signifier == Signifier::Cancelled,
                p,
                Message::ToggleEntryCancelled(entry.id),
            ),
            mini_btn(
                "*",
                entry.modifier == EntryModifier::Priority,
                p,
                Message::ToggleEntryPriority(entry.id),
            ),
            mini_btn(
                "!",
                entry.modifier == EntryModifier::Inspiration,
                p,
                Message::ToggleEntryInspiration(entry.id),
            ),
            mini_btn(
                paper_label,
                entry.paper_synced,
                p,
                Message::ToggleEntryPaper(entry.id),
            ),
            mini_btn(
                ">",
                self.migrating_entry_id == Some(entry.id),
                p,
                Message::ToggleMigratePicker(entry.id),
            ),
            mini_btn(
                "Edit",
                false,
                p,
                Message::StartEdit(entry.id, entry.text.clone()),
            ),
            mini_btn("×", false, p, Message::DeleteEntry(entry.id)),
        ]
        .spacing(3)
        .align_y(Alignment::Center);

        let entry_row = if entry_rtl {
            row![actions, body_txt, sig_btn, mod_txt, time_txt]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![time_txt, mod_txt, sig_btn, body_txt, actions]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        container(entry_row)
            .width(Length::Fill)
            .padding([5, 8])
            .style(move |_| container::Style {
                background: Some(p.surface_low.into()),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 5.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn view_migrate_bar(&self, entry_id: usize, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        let base_date = self.doc.date.unwrap_or_else(Vault::today_date);
        let tomorrow_rel = Vault::daily_rel_path(base_date + Duration::days(1));

        let mut bar = Row::new().spacing(5).align_y(Alignment::Center);
        bar = bar.push(
            text("Migrate to:")
                .size(11)
                .color(p.tertiary)
                .shaping(text::Shaping::Advanced),
        );
        bar = bar.push(pill_button(
            "Tomorrow (>)",
            false,
            p,
            Message::MigrateTo {
                entry_id,
                target_rel: tomorrow_rel,
                scheduled: false,
            },
        ));
        bar = bar.push(pill_button(
            "Future Log (<)",
            false,
            p,
            Message::MigrateTo {
                entry_id,
                target_rel: "Future Log.md".to_string(),
                scheduled: true,
            },
        ));

        for sp in &self.spaces {
            if !sp.is_daily && sp.rel_path != self.doc.rel_path {
                bar = bar.push(pill_button(
                    &sp.name,
                    false,
                    p,
                    Message::MigrateTo {
                        entry_id,
                        target_rel: sp.rel_path.clone(),
                        scheduled: false,
                    },
                ));
            }
        }

        container(bar)
            .width(Length::Fill)
            .padding([6, 10])
            .align_x(if ui_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            })
            .style(move |_| pane_style(p.surface_container, p.tertiary))
            .into()
    }

    fn view_inspector_pane(&self, p: Palette, ui_rtl: bool) -> Element<'_, Message> {
        // [4] SIGNIFIERS
        let sig_lines = [
            (".", "Action (task)"),
            ("×", "Completed"),
            ("~", "Cancelled / Irrelevant"),
            (">", "Migrated"),
            ("<", "Scheduled"),
            ("o", "Event"),
            ("=", "Emotion"),
            ("-", "Note"),
            ("*", "Priority modifier"),
            ("!", "Inspiration modifier"),
        ];

        let mut sig_col = Column::new().spacing(3);
        sig_col = sig_col.push(section_header("[4] SIGNIFIERS", p.primary, ui_rtl));
        for (sym, desc) in sig_lines {
            let s = text(sym).size(12).color(p.primary).width(Length::Fixed(16.0));
            let d = text(desc).size(11).color(p.on_surface_variant);
            let r = if ui_rtl {
                row![Space::new().width(Length::Fill), d, s].spacing(6)
            } else {
                row![s, d].spacing(6)
            };
            sig_col = sig_col.push(r);
        }

        let sig_card = container(sig_col)
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.outline_variant));

        // [5] CARRY TO PAPER
        let pending = self.doc.paper_pending_count();
        let synced = self.doc.entries.len().saturating_sub(pending);
        let paper_summary = text(format!("{pending} to copy · {synced} on paper"))
            .size(12)
            .color(if pending > 0 { p.secondary } else { p.tertiary })
            .width(Length::Fill)
            .align_x(if ui_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            });

        let mark_all_btn = button(
            text("Mark All Copied to Paper")
                .size(11)
                .color(p.on_primary)
                .width(Length::Fill)
                .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .padding([6, 8])
        .style(move |_, status| primary_btn_style(p, status))
        .on_press(Message::MarkAllPaperSynced);

        let paper_card = container(
            column![
                section_header("[5] CARRY TO PAPER", p.secondary, ui_rtl),
                paper_summary,
                mark_all_btn,
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .padding(10)
        .style(move |_| pane_style(p.surface_low, p.outline_variant));

        // [6] .MD BUFFER
        let md_toggle = pill_button(
            if self.show_md_preview { "Hide" } else { "Show" },
            false,
            p,
            Message::ToggleMdPreview,
        );
        let md_head = if ui_rtl {
            row![
                md_toggle,
                Space::new().width(Length::Fill),
                text("[6] .MD BUFFER").size(11).color(p.primary)
            ]
            .align_y(Alignment::Center)
        } else {
            row![
                text("[6] .MD BUFFER").size(11).color(p.primary),
                Space::new().width(Length::Fill),
                md_toggle
            ]
            .align_y(Alignment::Center)
        };

        let mut md_col = Column::new().spacing(6);
        md_col = md_col.push(md_head);
        if self.show_md_preview {
            md_col = md_col.push(
                scrollable(
                    text(self.raw_markdown.clone())
                        .size(11)
                        .color(p.on_surface_variant)
                        .shaping(text::Shaping::Advanced),
                )
                .height(Length::Fill),
            );
        }

        let md_card = container(md_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.outline_variant));

        column![sig_card, paper_card, md_card]
            .spacing(10)
            .width(Length::Fixed(265.0))
            .height(Length::Fill)
            .into()
    }
}

fn section_header<'a>(title: &'a str, color: Color, ui_rtl: bool) -> Element<'a, Message> {
    text(title)
        .size(11)
        .color(color)
        .width(Length::Fill)
        .align_x(if ui_rtl {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .into()
}

fn pill_button<'a>(
    label: &str,
    active: bool,
    p: Palette,
    msg: Message,
) -> Element<'a, Message> {
    button(
        text(label.to_string())
            .size(11)
            .color(if active { p.on_primary } else { p.on_surface })
            .shaping(text::Shaping::Advanced),
    )
    .padding([4, 8])
    .style(move |_, status| {
        if active {
            primary_btn_style(p, status)
        } else {
            secondary_btn_style(p, status)
        }
    })
    .on_press(msg)
    .into()
}

fn mini_btn<'a>(label: &str, active: bool, p: Palette, msg: Message) -> Element<'a, Message> {
    button(
        text(label.to_string())
            .size(10)
            .color(if active {
                p.on_primary
            } else {
                p.on_surface_variant
            })
            .align_y(Vertical::Center),
    )
    .padding([2, 5])
    .style(move |_, status| {
        if active {
            primary_btn_style(p, status)
        } else {
            secondary_btn_style(p, status)
        }
    })
    .on_press(msg)
    .into()
}

fn pane_style(bg: Color, border_color: Color) -> container::Style {
    container::Style {
        background: Some(bg.into()),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow::default(),
        text_color: None,
        snap: true,
    }
}

fn badge_style(p: Palette) -> container::Style {
    container::Style {
        background: Some(p.surface_container.into()),
        border: Border {
            color: p.outline_variant,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn primary_btn_style(p: Palette, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color { a: 0.9, ..p.primary },
        button::Status::Pressed => Color { a: 0.8, ..p.primary },
        _ => p.primary,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: p.on_primary,
        border: Border {
            color: p.primary,
            width: 1.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

fn secondary_btn_style(p: Palette, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => p.surface_high,
        _ => p.surface_container,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: p.on_surface,
        border: Border {
            color: p.outline_variant,
            width: 1.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

fn subtle_btn_style(p: Palette, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Some(p.surface_high.into()),
        _ => None,
    };
    button::Style {
        background: bg,
        text_color: p.on_surface,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn list_item_style(p: Palette, active: bool, status: button::Status) -> button::Style {
    let bg = if active {
        Some(p.surface_high.into())
    } else {
        match status {
            button::Status::Hovered | button::Status::Pressed => Some(p.surface_container.into()),
            _ => None,
        }
    };
    button::Style {
        background: bg,
        text_color: if active { p.primary } else { p.on_surface },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 5.0.into(),
        },
        ..Default::default()
    }
}

fn input_style(p: Palette, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => p.primary,
        _ => p.outline_variant,
    };
    text_input::Style {
        background: p.surface_lowest.into(),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 5.0.into(),
        },
        icon: p.on_surface_variant,
        placeholder: p.on_surface_variant,
        value: p.on_surface,
        selection: Color { a: 0.35, ..p.primary },
    }
}
