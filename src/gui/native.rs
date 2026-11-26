use crate::core::config::{FolioConfig, RtlMode, expand_tilde};
use crate::core::io::{ExportFormat, ImportFormat, export_document, import_from_file, import_from_str};
use crate::core::markdown::serialize_markdown;
use crate::core::model::{
    Document, Entry, EntryFilter, EntryModifier, SearchHit, Signifier, SpaceInfo, VaultEntryRef,
    VaultTree, is_rtl_text, parse_rapid_input_full,
};
use crate::core::review::{generate_monthly_review, generate_weekly_review};
use crate::core::updater::{InstallOutcome, UpdateInfo, check_for_updates, install_update};
use crate::core::vault::Vault;
use anyhow::Result;
use chrono::Duration;
use iced::alignment::{Horizontal, Vertical};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::{
    Column, Row, Space, button, column, container, image, operation, row, scrollable, text,
    text_input,
};
use iced::{
    Alignment, Border, Color, Element, Font, Length, Padding, Shadow, Subscription, Task, Theme,
    window,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-128.png");
const COMPOSER_ID: &str = "folio_composer";
const SEARCH_ID: &str = "folio_search";
const OPEN_FONTS_INSTALLER_URL: &str = "https://github.com/HamidRezaAshkiyan/open-fonts-installer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePanel {
    None,
    MigrationTriage,
    GlobalPaperQueue,
    Reflect,
    ImportExport,
    Settings,
}

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
    open_triage: Vec<VaultEntryRef>,
    global_paper: Vec<VaultEntryRef>,
    icon_handle: image::Handle,

    // UI preferences & panels
    rtl_mode: RtlMode,
    persian_font: String,
    has_vazirmatn: bool,
    has_dastkhat: bool,
    is_dark: bool,
    show_md_preview: bool,
    active_panel: ActivePanel,

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

    // Manage Spaces (create + rename)
    new_space_name: String,
    renaming_space_rel: Option<String>,
    renaming_space_text: String,

    // Threading & search
    thread_page_input: String,
    search_query: String,
    search_hits: Vec<SearchHit>,

    // AM / PM Reflection inputs
    am_focus_input: String,
    pm_emotion_input: String,

    // Import / Export inputs
    import_path_input: String,
    import_paste_input: String,
    import_format: ImportFormat,

    // Settings inputs
    settings_vault_input: String,
    settings_page_input: String,
    settings_spaces_input: String,

    // Auto-update state
    update_checking: bool,
    update_installing: bool,
    update_info: Option<UpdateInfo>,
    update_dismissed: bool,
    update_feedback: Option<String>,

    // Status banner
    status_msg: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    // Keyboard shortcuts
    KeyPressed(Key, Modifiers),

    // Preferences & Panels
    SetRtlMode(RtlMode),
    CycleRtlMode,
    TogglePersianFont,
    InstallPersianFonts,
    ToggleTheme,
    ToggleMdPreview,
    TogglePanel(ActivePanel),
    ClosePanels,

    // Navigation
    OpenDoc(String),
    OpenMasterIndex,
    OpenMonthlyLog,
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
    CycleEntrySignifier(usize),
    IndentEntry(usize, i8),
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

    // Migration & Future Log Month Buckets
    ToggleMigratePicker(usize),
    MigrateTo {
        entry_id: usize,
        target_rel: String,
        scheduled: bool,
    },
    ScheduleToMonth {
        entry_id: usize,
        month_ym: String,
    },

    // Batch Migration Triage
    TriageAction {
        source_rel: String,
        entry_id: usize,
        action: String,
    },

    // Global Carry-to-Paper Queue
    GlobalPaperSyncOne {
        rel_path: String,
        entry_id: usize,
    },
    GlobalPaperSyncAll,

    // AM / PM Reflection
    AmFocusChanged(String),
    SubmitAmFocus,
    PmEmotionChanged(String),
    SubmitPmEmotion,

    // Import / Export
    ImportPathChanged(String),
    ImportPasteChanged(String),
    SetImportFormat(ImportFormat),
    RunImportFile,
    RunImportPaste,
    RunExport(ExportFormat),

    // Manage Spaces & Search
    NewSpaceChanged(String),
    CreateSpace,
    StartRenameSpace(String, String),
    RenameSpaceChanged(String),
    SaveRenameSpace,
    CancelRenameSpace,
    DeleteSpace(String),
    SearchChanged(String),
    ClearSearch,

    // Settings
    SettingsVaultChanged(String),
    SettingsBrowseVault,
    SettingsOpenVaultFolder,
    SettingsOpenConfigDir,
    SettingsPageChanged(String),
    SettingsSpacesChanged(String),
    SaveSettings,
    ResetSettingsDefaults,

    // Auto-update
    ToggleAutoCheckUpdates,
    CheckForUpdates,
    UpdateCheckFinished(Result<UpdateInfo, String>),
    InstallUpdate,
    UpdateInstallFinished(Result<InstallOutcome, String>),
    OpenReleasePage,
    DismissUpdateBanner,
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
            let auto_check = ctx.config.auto_check_updates;
            let mut app = FolioApp::new(ctx.vault, ctx.config);
            let boot_task = if auto_check {
                app.update_checking = true;
                Task::perform(
                    async { check_for_updates().map_err(|e| e.to_string()) },
                    Message::UpdateCheckFinished,
                )
            } else {
                Task::none()
            };
            (app, boot_task)
        },
        FolioApp::update,
        FolioApp::view,
    )
    .title(FolioApp::title)
    .theme(FolioApp::theme)
    .subscription(FolioApp::subscription)
    .window(window::Settings {
        size: iced::Size::new(1320.0, 840.0),
        min_size: Some(iced::Size::new(960.0, 640.0)),
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
        let open_triage = vault.collect_open_actions(today).unwrap_or_default();
        let global_paper = vault.collect_paper_pending().unwrap_or_default();
        let rtl_mode = config.rtl_mode;
        let persian_font = config.persian_font.clone();
        let (has_vazirmatn, has_dastkhat) = detect_installed_persian_fonts();
        let is_dark = config.gui_theme != "light";
        let settings_vault_input = vault.root.to_string_lossy().to_string();
        let settings_page_input = config.notebook_page.to_string();
        let settings_spaces_input = config.default_spaces.join(", ");

        Self {
            vault,
            config,
            doc,
            spaces,
            vault_tree,
            raw_markdown,
            inline_tags,
            open_triage,
            global_paper,
            icon_handle: image::Handle::from_bytes(ICON_PNG.to_vec()),
            rtl_mode,
            persian_font,
            has_vazirmatn,
            has_dastkhat,
            is_dark,
            show_md_preview: true,
            active_panel: ActivePanel::None,
            composer_text: String::new(),
            composer_sig: Signifier::Action,
            composer_mod: EntryModifier::None,
            filter: EntryFilter::All,
            tag_filter: None,
            editing_entry_id: None,
            editing_text: String::new(),
            migrating_entry_id: None,
            new_space_name: String::new(),
            renaming_space_rel: None,
            renaming_space_text: String::new(),
            thread_page_input: String::new(),
            search_query: String::new(),
            search_hits: Vec::new(),
            am_focus_input: String::new(),
            pm_emotion_input: String::new(),
            import_path_input: String::new(),
            import_paste_input: String::new(),
            import_format: ImportFormat::Auto,
            settings_vault_input,
            settings_page_input,
            settings_spaces_input,
            update_checking: false,
            update_installing: false,
            update_info: None,
            update_dismissed: false,
            update_feedback: None,
            status_msg: None,
        }
    }

    fn title(&self) -> String {
        format!("Folio v{} — {}", env!("CARGO_PKG_VERSION"), self.doc.title)
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::KeyPressed(key, modifiers))
            }
            _ => None,
        })
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

    fn tr<'a>(&self, en: &'a str, fa: &'a str) -> &'a str {
        if self.is_ui_rtl() { fa } else { en }
    }

    fn persian_text_font(&self) -> Font {
        if self.persian_font.eq_ignore_ascii_case("dastkhat") {
            if self.has_dastkhat {
                return Font::with_name("Dastkhat");
            }
            if self.has_vazirmatn {
                return Font::with_name("Vazirmatn");
            }
        } else {
            if self.has_vazirmatn {
                return Font::with_name("Vazirmatn");
            }
            if self.has_dastkhat {
                return Font::with_name("Dastkhat");
            }
        }
        Font::DEFAULT
    }

    fn active_ui_font(&self) -> Font {
        if self.is_ui_rtl() {
            self.persian_text_font()
        } else {
            Font::DEFAULT
        }
    }

    fn persian_font_label(&self) -> &'static str {
        if self.persian_font.eq_ignore_ascii_case("dastkhat") && self.has_dastkhat {
            "دست‌خط"
        } else if self.has_vazirmatn {
            "وزیرمتن"
        } else if self.has_dastkhat {
            "دست‌خط"
        } else {
            "فونت"
        }
    }

    fn filter_label(&self, f: EntryFilter) -> &'static str {
        if self.is_ui_rtl() {
            match f {
                EntryFilter::All => "همه",
                EntryFilter::Action => "کار (.)",
                EntryFilter::Event => "رویداد (o)",
                EntryFilter::Emotion => "احساس (=)",
                EntryFilter::Note => "یادداشت (-)",
            }
        } else {
            f.label()
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
        if let Ok(triage) = self.vault.collect_open_actions(today) {
            self.open_triage = triage;
        }
        if let Ok(paper) = self.vault.collect_paper_pending() {
            self.global_paper = paper;
        }
    }

    fn reload_current_doc(&mut self) {
        let rel = self.doc.rel_path.clone();
        if let Ok(doc) = self.vault.load_document(&rel) {
            self.doc = doc;
        }
        self.refresh_metadata();
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
        self.config.persian_font = self.persian_font.clone();
        self.config.gui_theme = if self.is_dark { "dark" } else { "light" }.to_string();
        let _ = self.config.save();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        self.status_msg = None;

        match message {
            Message::KeyPressed(key, mods) => {
                let cmd = mods.command();
                match key.as_ref() {
                    Key::Named(Named::Escape) => {
                        self.active_panel = ActivePanel::None;
                        self.editing_entry_id = None;
                        self.migrating_entry_id = None;
                        self.renaming_space_rel = None;
                        self.search_hits.clear();
                    }
                    Key::Character("n") if cmd => {
                        return operation::focus(COMPOSER_ID);
                    }
                    Key::Character("f") if cmd => {
                        return operation::focus(SEARCH_ID);
                    }
                    Key::Character("r") if cmd && mods.shift() => {
                        return self.update(Message::CycleRtlMode);
                    }
                    Key::Character("[") if cmd => {
                        return self.update(Message::NavDay(-1));
                    }
                    Key::Character("]") if cmd => {
                        return self.update(Message::NavDay(1));
                    }
                    Key::Character("t") if cmd => {
                        return self.update(Message::NavDay(0));
                    }
                    Key::Character("i") if cmd => {
                        return self.update(Message::OpenMasterIndex);
                    }
                    Key::Character("m") if cmd => {
                        return self.update(Message::TogglePanel(ActivePanel::MigrationTriage));
                    }
                    Key::Character(",") if cmd => {
                        return self.update(Message::TogglePanel(ActivePanel::Settings));
                    }
                    _ => {}
                }
            }
            Message::SetRtlMode(mode) => {
                self.rtl_mode = mode;
                self.persist_config();
                self.status_msg = Some(match mode {
                    RtlMode::Auto => "Direction: AUTO (aligned columns, auto-RTL text)".to_string(),
                    RtlMode::Rtl => {
                        if !self.has_vazirmatn && !self.has_dastkhat {
                            "رابط فارسی فعال شد — برای نمایش بهتر، فونت وزیرمتن یا دست‌خط را نصب کنید".to_string()
                        } else {
                            format!("رابط فارسی فعال شد (فونت: {})", self.persian_font_label())
                        }
                    }
                    RtlMode::Ltr => "Direction: LTR (standard left-to-right)".to_string(),
                });
            }
            Message::CycleRtlMode => {
                let next = match self.rtl_mode {
                    RtlMode::Ltr => RtlMode::Auto,
                    RtlMode::Auto => RtlMode::Rtl,
                    RtlMode::Rtl => RtlMode::Ltr,
                };
                return self.update(Message::SetRtlMode(next));
            }
            Message::TogglePersianFont => {
                self.persian_font = if self.persian_font.eq_ignore_ascii_case("vazirmatn") {
                    "dastkhat".to_string()
                } else {
                    "vazirmatn".to_string()
                };
                self.persist_config();
                self.status_msg = Some(format!("فونت فارسی: {}", self.persian_font_label()));
            }
            Message::InstallPersianFonts => {
                match install_persian_fonts_via_open_fonts_installer() {
                    Ok(msg) => self.status_msg = Some(msg),
                    Err(e) => {
                        let _ = open::that(OPEN_FONTS_INSTALLER_URL);
                        self.status_msg = Some(format!(
                            "Opened open-fonts-installer ({e})"
                        ));
                    }
                }
            }
            Message::ToggleTheme => {
                self.is_dark = !self.is_dark;
                self.persist_config();
            }
            Message::ToggleMdPreview => {
                self.show_md_preview = !self.show_md_preview;
            }
            Message::TogglePanel(panel) => {
                self.active_panel = if self.active_panel == panel {
                    ActivePanel::None
                } else {
                    if panel == ActivePanel::Settings {
                        self.settings_vault_input = self.vault.root.to_string_lossy().to_string();
                        self.settings_page_input = self.config.notebook_page.to_string();
                        self.settings_spaces_input = self.config.default_spaces.join(", ");
                    }
                    self.refresh_metadata();
                    panel
                };
            }
            Message::ClosePanels => {
                self.active_panel = ActivePanel::None;
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
            Message::OpenMonthlyLog => {
                let anchor = self.doc.date.unwrap_or_else(Vault::today_date);
                match self.vault.open_or_create_monthly_log(anchor) {
                    Ok(doc) => {
                        self.doc = doc;
                        self.editing_entry_id = None;
                        self.migrating_entry_id = None;
                        self.tag_filter = None;
                        self.refresh_metadata();
                        self.status_msg = Some("Opened Monthly Log (Calendar + Task List)".to_string());
                    }
                    Err(e) => self.status_msg = Some(format!("Monthly Log error: {e}")),
                }
            }
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
            Message::CycleEntrySignifier(id) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    entry.signifier = entry.signifier.cycle_base();
                    self.save_current_doc();
                }
            }
            Message::IndentEntry(id, delta) => {
                if let Some(entry) = self.doc.entries.iter_mut().find(|e| e.id == id) {
                    let next = (entry.indent as i8 + delta).clamp(0, 3) as u8;
                    if next != entry.indent {
                        entry.indent = next;
                        self.save_current_doc();
                    }
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
                self.status_msg = Some("All entries in this doc marked carried to paper".to_string());
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
            Message::ScheduleToMonth { entry_id, month_ym } => {
                match self
                    .vault
                    .schedule_to_future_month(&mut self.doc, entry_id, &month_ym)
                {
                    Ok(()) => {
                        self.migrating_entry_id = None;
                        self.refresh_metadata();
                        self.status_msg =
                            Some(format!("Scheduled entry to Future Log [{month_ym}]"));
                    }
                    Err(e) => self.status_msg = Some(format!("Schedule failed: {e}")),
                }
            }
            Message::TriageAction {
                source_rel,
                entry_id,
                action,
            } => {
                let today = Vault::today_date();
                match self.vault.triage_open_action(
                    &source_rel,
                    entry_id,
                    &action,
                    today,
                    self.config.notebook_page,
                ) {
                    Ok(()) => {
                        self.reload_current_doc();
                        self.status_msg = Some(format!("Triaged task ({action})"));
                    }
                    Err(e) => self.status_msg = Some(format!("Triage error: {e}")),
                }
            }
            Message::GlobalPaperSyncOne { rel_path, entry_id } => {
                match self.vault.mark_entry_paper_synced(&rel_path, entry_id, true) {
                    Ok(()) => {
                        self.reload_current_doc();
                        self.status_msg = Some("Marked entry carried to paper".to_string());
                    }
                    Err(e) => self.status_msg = Some(format!("Paper sync error: {e}")),
                }
            }
            Message::GlobalPaperSyncAll => match self.vault.mark_all_vault_paper_synced() {
                Ok(n) => {
                    self.reload_current_doc();
                    self.status_msg =
                        Some(format!("Marked {n} entries across vault carried to paper"));
                }
                Err(e) => self.status_msg = Some(format!("Paper sync error: {e}")),
            },
            Message::AmFocusChanged(v) => self.am_focus_input = v,
            Message::SubmitAmFocus => {
                let txt = self.am_focus_input.trim().to_string();
                if !txt.is_empty() {
                    let today = Vault::today_date();
                    if let Ok(mut daily) =
                        self.vault.open_or_create_daily(today, self.config.notebook_page)
                    {
                        daily.add_entry_with_modifier(
                            Vault::now_hhmm(),
                            EntryModifier::Priority,
                            Signifier::Action,
                            txt,
                        );
                        let _ = self.vault.save_document(&daily);
                        self.am_focus_input.clear();
                        self.reload_current_doc();
                        self.status_msg = Some("Added AM priority focus (*. ) to Today".to_string());
                    }
                }
            }
            Message::PmEmotionChanged(v) => self.pm_emotion_input = v,
            Message::SubmitPmEmotion => {
                let txt = self.pm_emotion_input.trim().to_string();
                if !txt.is_empty() {
                    let today = Vault::today_date();
                    if let Ok(mut daily) =
                        self.vault.open_or_create_daily(today, self.config.notebook_page)
                    {
                        daily.add_entry_with_modifier(
                            Vault::now_hhmm(),
                            EntryModifier::None,
                            Signifier::Emotion,
                            txt,
                        );
                        let _ = self.vault.save_document(&daily);
                        self.pm_emotion_input.clear();
                        self.reload_current_doc();
                        self.status_msg = Some("Logged PM reflection (= ) to Today".to_string());
                    }
                }
            }
            Message::ImportPathChanged(v) => self.import_path_input = v,
            Message::ImportPasteChanged(v) => self.import_paste_input = v,
            Message::SetImportFormat(fmt) => self.import_format = fmt,
            Message::RunImportFile => {
                let raw_path = self.import_path_input.trim();
                if !raw_path.is_empty() {
                    let expanded = crate::core::config::expand_tilde(PathBuf::from(raw_path));
                    match import_from_file(&expanded, self.import_format, &mut self.doc) {
                        Ok(n) => {
                            self.import_path_input.clear();
                            self.save_current_doc();
                            self.status_msg = Some(format!(
                                "Imported {n} entries from {}",
                                expanded.display()
                            ));
                        }
                        Err(e) => self.status_msg = Some(format!("Import failed: {e}")),
                    }
                }
            }
            Message::RunImportPaste => {
                let raw = self.import_paste_input.clone();
                if !raw.trim().is_empty() {
                    match import_from_str(&raw, self.import_format, &mut self.doc) {
                        Ok(n) => {
                            self.import_paste_input.clear();
                            self.save_current_doc();
                            self.status_msg = Some(format!("Imported {n} entries from text"));
                        }
                        Err(e) => self.status_msg = Some(format!("Import failed: {e}")),
                    }
                }
            }
            Message::RunExport(fmt) => {
                let ext = match fmt {
                    ExportFormat::Markdown => "md",
                    ExportFormat::Json => "json",
                    ExportFormat::Csv => "csv",
                };
                match export_document(&self.doc, fmt) {
                    Ok(content) => {
                        let exports_dir = self.vault.root.join("Exports");
                        let _ = fs::create_dir_all(&exports_dir);
                        let stem = self
                            .doc
                            .path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("folio");
                        let out_path = exports_dir.join(format!("{stem}.{ext}"));
                        match fs::write(&out_path, content) {
                            Ok(()) => {
                                self.status_msg =
                                    Some(format!("Exported to {}", out_path.display()));
                            }
                            Err(e) => self.status_msg = Some(format!("Export write failed: {e}")),
                        }
                    }
                    Err(e) => self.status_msg = Some(format!("Export failed: {e}")),
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
            Message::StartRenameSpace(rel, current_name) => {
                self.renaming_space_rel = Some(rel);
                self.renaming_space_text = current_name;
            }
            Message::RenameSpaceChanged(v) => {
                self.renaming_space_text = v;
            }
            Message::SaveRenameSpace => {
                if let Some(old_rel) = self.renaming_space_rel.take() {
                    let new_name = self.renaming_space_text.trim().to_string();
                    self.renaming_space_text.clear();
                    if !new_name.is_empty() {
                        match self.vault.rename_space(&old_rel, &new_name) {
                            Ok(doc) => {
                                if self.doc.rel_path == old_rel {
                                    self.doc = doc;
                                }
                                self.refresh_metadata();
                                self.status_msg = Some(format!("Renamed space to '{new_name}'"));
                            }
                            Err(e) => self.status_msg = Some(format!("Rename failed: {e}")),
                        }
                    }
                }
            }
            Message::CancelRenameSpace => {
                self.renaming_space_rel = None;
                self.renaming_space_text.clear();
            }
            Message::DeleteSpace(rel_path) => match self.vault.delete_space(&rel_path) {
                Ok(()) => {
                    if self.doc.rel_path == rel_path {
                        let today = Vault::today_date();
                        if let Ok(daily) =
                            self.vault.open_or_create_daily(today, self.config.notebook_page)
                        {
                            self.doc = daily;
                        }
                    }
                    self.refresh_metadata();
                    self.status_msg = Some(format!("Deleted space {rel_path}"));
                }
                Err(e) => self.status_msg = Some(format!("Delete failed: {e}")),
            },
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
            Message::SettingsVaultChanged(val) => {
                self.settings_vault_input = val;
            }
            Message::SettingsBrowseVault => {
                if let Some(chosen) = pick_folder_dialog(&self.vault.root) {
                    self.settings_vault_input = chosen.to_string_lossy().to_string();
                } else {
                    self.status_msg = Some(
                        self.tr(
                            "Folder picker cancelled or unavailable — enter path directly.",
                            "انتخابگر پوشه لغو شد — مسیر را مستقیماً وارد کنید.",
                        )
                        .to_string(),
                    );
                }
            }
            Message::SettingsOpenVaultFolder => {
                if let Err(e) = open::that(&self.vault.root) {
                    self.status_msg = Some(format!("Could not open vault folder: {e}"));
                }
            }
            Message::SettingsOpenConfigDir => {
                let cfg_path = FolioConfig::config_path();
                let dir = cfg_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or(cfg_path);
                let _ = fs::create_dir_all(&dir);
                if let Err(e) = open::that(&dir) {
                    self.status_msg = Some(format!("Could not open config folder: {e}"));
                }
            }
            Message::SettingsPageChanged(val) => {
                self.settings_page_input = val;
            }
            Message::SettingsSpacesChanged(val) => {
                self.settings_spaces_input = val;
            }
            Message::SaveSettings => {
                let raw_vault = self.settings_vault_input.trim();
                if raw_vault.is_empty() {
                    self.status_msg = Some(
                        self.tr(
                            "Vault folder path cannot be empty.",
                            "مسیر پوشه بایگانی نمی‌تواند خالی باشد.",
                        )
                        .to_string(),
                    );
                } else {
                    let parsed_page = self
                        .settings_page_input
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .filter(|&p| p > 0)
                        .unwrap_or(self.config.notebook_page.max(1));

                    let parsed_spaces: Vec<String> = self
                        .settings_spaces_input
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let next_spaces = if parsed_spaces.is_empty() {
                        FolioConfig::default().default_spaces
                    } else {
                        parsed_spaces
                    };

                    let expanded_vault = expand_tilde(PathBuf::from(raw_vault));
                    let vault_changed = expanded_vault != self.vault.root;

                    match Vault::open_or_init(expanded_vault.clone(), &next_spaces) {
                        Ok(new_vault) => {
                            self.config.vault_path = expanded_vault;
                            self.config.notebook_page = parsed_page;
                            self.config.default_spaces = next_spaces;
                            self.persist_config();

                            self.vault = new_vault;
                            self.settings_vault_input =
                                self.vault.root.to_string_lossy().to_string();
                            self.settings_page_input = self.config.notebook_page.to_string();
                            self.settings_spaces_input = self.config.default_spaces.join(", ");

                            if vault_changed {
                                let today = Vault::today_date();
                                if let Ok(daily) = self
                                    .vault
                                    .open_or_create_daily(today, self.config.notebook_page)
                                {
                                    self.doc = daily;
                                    self.editing_entry_id = None;
                                    self.migrating_entry_id = None;
                                    self.tag_filter = None;
                                    self.search_query.clear();
                                    self.search_hits.clear();
                                }
                            }
                            self.refresh_metadata();
                            self.status_msg = Some(if vault_changed {
                                format!(
                                    "{} {}",
                                    self.tr("Settings saved. Switched vault to", "تنظیمات ذخیره شد. بایگانی فعال:"),
                                    self.vault.display_root()
                                )
                            } else {
                                self.tr(
                                    "Settings saved to config.toml.",
                                    "تنظیمات در config.toml ذخیره شد.",
                                )
                                .to_string()
                            });
                        }
                        Err(e) => {
                            self.status_msg = Some(format!("Failed to open vault folder: {e}"));
                        }
                    }
                }
            }
            Message::ResetSettingsDefaults => {
                let def = FolioConfig::default();
                self.settings_vault_input = def.vault_path.to_string_lossy().to_string();
                self.settings_page_input = def.notebook_page.to_string();
                self.settings_spaces_input = def.default_spaces.join(", ");
                self.rtl_mode = def.rtl_mode;
                self.is_dark = def.gui_theme != "light";
                self.persian_font = def.persian_font;
                self.config.auto_check_updates = def.auto_check_updates;
                return self.update(Message::SaveSettings);
            }
            Message::ToggleAutoCheckUpdates => {
                self.config.auto_check_updates = !self.config.auto_check_updates;
                self.persist_config();
            }
            Message::CheckForUpdates => {
                if !self.update_checking && !self.update_installing {
                    self.update_checking = true;
                    self.update_dismissed = false;
                    self.update_feedback = Some(
                        self.tr("Checking GitHub Releases...", "در حال بررسی نسخه‌های جدید...")
                            .to_string(),
                    );
                    return Task::perform(
                        async { check_for_updates().map_err(|e| e.to_string()) },
                        Message::UpdateCheckFinished,
                    );
                }
            }
            Message::UpdateCheckFinished(res) => {
                self.update_checking = false;
                match res {
                    Ok(info) => {
                        if info.has_update {
                            self.update_feedback = Some(format!(
                                "{} v{}",
                                self.tr("Update available:", "نسخه جدید موجود است:"),
                                info.latest_version
                            ));
                        } else {
                            self.update_feedback = Some(format!(
                                "{} (v{})",
                                self.tr("Up to date", "به‌روز است"),
                                info.current_version
                            ));
                        }
                        self.update_info = Some(info);
                    }
                    Err(e) => {
                        if self.active_panel == ActivePanel::Settings {
                            self.update_feedback = Some(format!("Update check failed: {e}"));
                        }
                    }
                }
            }
            Message::InstallUpdate => {
                if !self.update_installing
                    && let Some(info) = self.update_info.clone()
                {
                    self.update_installing = true;
                    self.update_feedback = Some(format!(
                        "{} v{}...",
                        self.tr("Downloading & installing", "در حال دانلود و نصب"),
                        info.latest_version
                    ));
                    return Task::perform(
                        async move { install_update(&info).map_err(|e| e.to_string()) },
                        Message::UpdateInstallFinished,
                    );
                }
            }
            Message::UpdateInstallFinished(res) => {
                self.update_installing = false;
                match res {
                    Ok(outcome) => {
                        self.update_dismissed = true;
                        self.update_feedback = Some(outcome.message.clone());
                        self.status_msg = Some(outcome.message);
                    }
                    Err(e) => {
                        let msg = format!("Update install failed: {e}");
                        self.update_feedback = Some(msg.clone());
                        self.status_msg = Some(msg);
                    }
                }
            }
            Message::OpenReleasePage => {
                let url = self
                    .update_info
                    .as_ref()
                    .map(|u| u.release_url.as_str())
                    .unwrap_or("https://github.com/HamidRezaAshkiyan/folio/releases");
                let _ = open::that(url);
            }
            Message::DismissUpdateBanner => {
                self.update_dismissed = true;
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let p = self.palette();
        let ui_rtl = self.is_ui_rtl();
        let ui_font = self.active_ui_font();

        let top_bar = self.view_top_bar(p, ui_rtl, ui_font);
        let left_nav = self.view_nav_pane(p, ui_rtl, ui_font);
        let center_log = self.view_rapid_log_pane(p, ui_rtl, ui_font);
        let right_inspector = self.view_inspector_pane(p, ui_rtl, ui_font);

        let body: Row<'_, Message> = if ui_rtl {
            row![right_inspector, center_log, left_nav]
                .spacing(10)
                .height(Length::Fill)
        } else {
            row![left_nav, center_log, right_inspector]
                .spacing(10)
                .height(Length::Fill)
        };

        let mut root_col = column![top_bar].spacing(8);

        // Non-intrusive Update Available banner
        if let Some(info) = &self.update_info
            && info.has_update
            && !self.update_dismissed
        {
            let msg_text = if self.update_installing {
                format!(
                    "{} v{}...",
                    self.tr("Installing Folio", "در حال نصب فولیو"),
                    info.latest_version
                )
            } else {
                format!(
                    "{} v{} ({} v{})",
                    self.tr("Folio update available:", "نسخه جدید فولیو موجود است:"),
                    info.latest_version,
                    self.tr("current", "فعلی"),
                    info.current_version
                )
            };

            let install_btn = pill_button(
                if self.update_installing {
                    self.tr("Installing...", "در حال نصب...")
                } else {
                    self.tr("Install Update", "نصب به‌روزرسانی")
                },
                true,
                p,
                ui_font,
                Message::InstallUpdate,
            );
            let notes_btn = pill_button(
                self.tr("Release Notes", "تغییرات نسخه"),
                false,
                p,
                ui_font,
                Message::OpenReleasePage,
            );
            let dismiss_btn = pill_button(
                "✕",
                false,
                p,
                ui_font,
                Message::DismissUpdateBanner,
            );

            let banner_row = if ui_rtl {
                row![
                    dismiss_btn,
                    notes_btn,
                    install_btn,
                    Space::new().width(Length::Fill),
                    text(msg_text)
                        .size(11)
                        .color(p.primary)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            } else {
                row![
                    text(msg_text)
                        .size(11)
                        .color(p.primary)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                    Space::new().width(Length::Fill),
                    install_btn,
                    notes_btn,
                    dismiss_btn,
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            };

            let update_banner = container(banner_row)
                .width(Length::Fill)
                .padding([5, 10])
                .style(move |_| pane_style(p.surface_container, p.primary));
            root_col = root_col.push(update_banner);
        }

        // If Persian UI is active and neither Vazirmatn nor Dastkhat is installed,
        // offer a zero-bloat one-click installer powered by open-fonts-installer.
        if ui_rtl && !self.has_vazirmatn && !self.has_dastkhat {
            let install_banner = container(
                row![
                    pill_button(
                        "نصب فونت وزیرمتن / دست‌خط (open-fonts-installer)",
                        true,
                        p,
                        ui_font,
                        Message::InstallPersianFonts,
                    ),
                    Space::new().width(Length::Fill),
                    text("فونت فارسی وزیرمتن یا دست‌خط روی سیستم یافت نشد — برای نمایش بهتر بدون افزایش حجم برنامه نصب کنید:")
                        .size(11)
                        .color(p.tertiary)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([5, 10])
            .style(move |_| pane_style(p.surface_container, p.tertiary));
            root_col = root_col.push(install_banner);
        }

        if !self.search_hits.is_empty() {
            root_col = root_col.push(self.view_search_results(p, ui_rtl, ui_font));
        }

        match self.active_panel {
            ActivePanel::MigrationTriage => {
                root_col = root_col.push(self.view_triage_panel(p, ui_rtl, ui_font));
            }
            ActivePanel::GlobalPaperQueue => {
                root_col = root_col.push(self.view_global_paper_panel(p, ui_rtl, ui_font));
            }
            ActivePanel::Reflect => {
                root_col = root_col.push(self.view_reflect_panel(p, ui_rtl, ui_font));
            }
            ActivePanel::ImportExport => {
                root_col = root_col.push(self.view_import_export_panel(p, ui_rtl, ui_font));
            }
            ActivePanel::Settings => {
                root_col = root_col.push(self.view_settings_panel(p, ui_rtl, ui_font));
            }
            ActivePanel::None => {}
        }

        if let Some(msg) = &self.status_msg {
            root_col = root_col.push(
                container(
                    text(msg.clone())
                        .size(12)
                        .color(p.tertiary)
                        .font(ui_font)
                        .shaping(text::Shaping::Advanced)
                        .width(Length::Fill)
                        .align_x(if ui_rtl {
                            Horizontal::Right
                        } else {
                            Horizontal::Left
                        }),
                )
                .width(Length::Fill)
                .padding([4, 12])
                .style(move |_| pane_style(p.surface_container, p.outline_variant)),
            );
        }

        root_col = root_col.push(body);

        // Footer status bar with version, shortcuts hint, vault root, and direction mode
        let brand_footer = text(format!(
            "{} v{}",
            self.tr("Folio", "فولیو"),
            env!("CARGO_PKG_VERSION")
        ))
        .size(11)
        .color(p.primary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let vault_footer = text(format!(
            "· {}: {}",
            self.tr("Vault", "بایگانی"),
            self.vault.display_root()
        ))
        .size(11)
        .color(p.on_surface_variant)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let shortcuts_footer = text(self.tr(
            "Shortcuts: ⌘N Log · ⌘F Search · ⌘[ / ⌘] Day · ⌘T Today · ⌘I Index · ⌘M Triage · ⌘⇧R RTL",
            "میانبرها: ⌘N ثبت · ⌘F جستجو · ⌘[ / ⌘] روز · ⌘T امروز · ⌘I فهرست · ⌘M بررسی · ⌘⇧R زبان",
        ))
        .size(10)
        .color(p.on_surface_variant)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let mode_footer = text(if ui_rtl {
            format!(
                "· فارسی ({}) · {}",
                self.persian_font_label(),
                if self.is_dark { "تیره" } else { "روشن" }
            )
        } else {
            format!(
                "· Dir: {} · {}",
                self.rtl_mode.label(),
                if self.is_dark { "Dark" } else { "Light" }
            )
        })
        .size(11)
        .color(p.on_surface_variant)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let footer_row = if ui_rtl {
            row![
                mode_footer,
                shortcuts_footer,
                Space::new().width(Length::Fill),
                vault_footer,
                brand_footer,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        } else {
            row![
                brand_footer,
                vault_footer,
                Space::new().width(Length::Fill),
                shortcuts_footer,
                mode_footer,
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };

        let footer = container(footer_row)
            .width(Length::Fill)
            .padding([4, 10])
            .style(move |_| pane_style(p.surface_low, p.outline_variant));

        root_col = root_col.push(footer);

        container(root_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .style(move |_| container::Style {
                background: Some(p.surface.into()),
                ..Default::default()
            })
            .into()
    }

    fn view_top_bar(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        let icon_img = image(self.icon_handle.clone()).width(22).height(22);
        let brand_title = text(self.tr("FOLIO", "فولیو"))
            .size(14)
            .color(p.primary)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);
        let version_badge = container(
            text(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(11)
                .color(p.primary),
        )
        .padding([2, 6])
        .style(move |_| badge_style(p));

        let brand_group = if ui_rtl {
            row![version_badge, brand_title, icon_img]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![icon_img, brand_title, version_badge]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        let mut rtl_toggle = row![
            self.rtl_pill(p, ui_font, RtlMode::Ltr),
            self.rtl_pill(p, ui_font, RtlMode::Auto),
            self.rtl_pill(p, ui_font, RtlMode::Rtl),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        if ui_rtl && (self.has_vazirmatn && self.has_dastkhat) {
            rtl_toggle = rtl_toggle.push(pill_button(
                self.persian_font_label(),
                false,
                p,
                ui_font,
                Message::TogglePersianFont,
            ));
        }

        let theme_btn = pill_button(
            if self.is_dark {
                self.tr("Light", "روشن")
            } else {
                self.tr("Dark", "تیره")
            },
            false,
            p,
            ui_font,
            Message::ToggleTheme,
        );

        let day_nav = if ui_rtl {
            row![
                pill_button("‹", false, p, ui_font, Message::NavDay(1)),
                pill_button("امروز", false, p, ui_font, Message::NavDay(0)),
                pill_button("›", false, p, ui_font, Message::NavDay(-1)),
            ]
            .spacing(3)
        } else {
            row![
                pill_button("‹", false, p, ui_font, Message::NavDay(-1)),
                pill_button("Today", false, p, ui_font, Message::NavDay(0)),
                pill_button("›", false, p, ui_font, Message::NavDay(1)),
            ]
            .spacing(3)
        };

        let page_num = self.doc.page.unwrap_or(self.config.notebook_page);
        let page_ctrl = row![
            pill_button("-", false, p, ui_font, Message::AdjustPage(-1)),
            container(
                text(if ui_rtl {
                    format!("ص.{page_num}")
                } else {
                    format!("p.{page_num}")
                })
                .size(11)
                .color(p.tertiary)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
            )
            .padding([3, 6])
            .style(move |_| badge_style(p)),
            pill_button("+", false, p, ui_font, Message::AdjustPage(1)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        let triage_label = if self.open_triage.is_empty() {
            self.tr("Triage", "بررسی").to_string()
        } else {
            format!("{} ({})", self.tr("Triage", "بررسی"), self.open_triage.len())
        };

        let bujo_nav = row![
            pill_button(
                self.tr("Index", "فهرست"),
                self.doc.rel_path.eq_ignore_ascii_case("Index.md"),
                p,
                ui_font,
                Message::OpenMasterIndex,
            ),
            pill_button(
                self.tr("Monthly", "ماهانه"),
                self.doc.rel_path.ends_with("-monthly.md"),
                p,
                ui_font,
                Message::OpenMonthlyLog,
            ),
            pill_button(
                &triage_label,
                self.active_panel == ActivePanel::MigrationTriage,
                p,
                ui_font,
                Message::TogglePanel(ActivePanel::MigrationTriage),
            ),
            pill_button(
                self.tr("Reflect", "بازتاب"),
                self.active_panel == ActivePanel::Reflect,
                p,
                ui_font,
                Message::TogglePanel(ActivePanel::Reflect),
            ),
            pill_button(
                self.tr("Weekly", "هفتگی"),
                false,
                p,
                ui_font,
                Message::GenerateReview(true),
            ),
            pill_button(
                self.tr("M.Rev", "مرور ماه"),
                false,
                p,
                ui_font,
                Message::GenerateReview(false),
            ),
            pill_button(
                self.tr("I/O", "ورود/خروج"),
                self.active_panel == ActivePanel::ImportExport,
                p,
                ui_font,
                Message::TogglePanel(ActivePanel::ImportExport),
            ),
            pill_button(
                self.tr("Settings", "تنظیمات"),
                self.active_panel == ActivePanel::Settings,
                p,
                ui_font,
                Message::TogglePanel(ActivePanel::Settings),
            ),
        ]
        .spacing(3);

        let search_input = text_input(
            self.tr("Search vault or #tag...", "جستجو یا #تگ..."),
            &self.search_query,
        )
        .id(SEARCH_ID)
        .on_input(Message::SearchChanged)
        .size(11)
        .font(ui_font)
        .padding([4, 8])
        .width(Length::Fixed(165.0))
        .align_x(if ui_rtl {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .style(move |_, status| input_style(p, status));

        let controls = if ui_rtl {
            row![
                search_input,
                theme_btn,
                rtl_toggle,
                bujo_nav,
                page_ctrl,
                day_nav,
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        } else {
            row![
                day_nav,
                page_ctrl,
                bujo_nav,
                rtl_toggle,
                theme_btn,
                search_input,
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        };

        let bar_row = if ui_rtl {
            row![controls, Space::new().width(Length::Fill), brand_group]
                .spacing(10)
                .align_y(Alignment::Center)
        } else {
            row![brand_group, Space::new().width(Length::Fill), controls]
                .spacing(10)
                .align_y(Alignment::Center)
        };

        container(bar_row)
            .width(Length::Fill)
            .padding([7, 10])
            .style(move |_| pane_style(p.surface_low, p.outline_variant))
            .into()
    }

    fn rtl_pill(&self, p: Palette, ui_font: Font, mode: RtlMode) -> Element<'_, Message> {
        let active = self.rtl_mode == mode;
        let label = match mode {
            RtlMode::Ltr => "LTR",
            RtlMode::Auto => "AUTO",
            RtlMode::Rtl => "فارسی",
        };
        let pill_font = if mode == RtlMode::Rtl {
            self.persian_text_font()
        } else {
            ui_font
        };
        pill_button(label, active, p, pill_font, Message::SetRtlMode(mode))
    }

    fn view_triage_panel(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        let mut col = Column::new().spacing(6);
        let title = text(if ui_rtl {
            format!(
                "بررسی و انتقال کارها — {} کار باز از روزها و ماه‌های گذشته",
                self.open_triage.len()
            )
        } else {
            format!(
                "Batch Migration Triage — {} open actions from past Daily & Monthly logs",
                self.open_triage.len()
            )
        })
        .size(12)
        .color(p.tertiary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let close_btn = pill_button(
            self.tr("Close", "بستن"),
            false,
            p,
            ui_font,
            Message::ClosePanels,
        );
        let head = if ui_rtl {
            row![close_btn, Space::new().width(Length::Fill), title].align_y(Alignment::Center)
        } else {
            row![title, Space::new().width(Length::Fill), close_btn].align_y(Alignment::Center)
        };
        col = col.push(head);

        if self.open_triage.is_empty() {
            col = col.push(
                text(self.tr(
                    "All caught up! No open (.) actions left in past Daily Notes or Monthly Logs.",
                    "همه کارها بررسی شدند! هیچ کار بازی در یادداشت‌های قبلی باقی نمانده است.",
                ))
                .size(12)
                .color(p.on_surface_variant)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .width(Length::Fill)
                .align_x(if ui_rtl {
                    Horizontal::Right
                } else {
                    Horizontal::Left
                }),
            );
        } else {
            let mut list = Column::new().spacing(4);
            for item in &self.open_triage {
                let is_rtl = self.is_text_rtl(&item.entry.text);
                let text_font = if is_rtl {
                    self.persian_text_font()
                } else {
                    ui_font
                };
                let src = text(format!("[{}]", item.doc_title))
                    .size(11)
                    .color(p.on_surface_variant)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced);
                let body = text(item.entry.text.clone())
                    .size(12)
                    .color(p.on_surface)
                    .font(text_font)
                    .shaping(text::Shaping::Advanced)
                    .width(Length::Fill)
                    .align_x(if is_rtl || ui_rtl {
                        Horizontal::Right
                    } else {
                        Horizontal::Left
                    });
                let btns = row![
                    mini_btn(
                        self.tr("> Today", "> امروز"),
                        true,
                        p,
                        ui_font,
                        Message::TriageAction {
                            source_rel: item.rel_path.clone(),
                            entry_id: item.entry.id,
                            action: "today".into(),
                        },
                    ),
                    mini_btn(
                        self.tr("< Future", "< آینده"),
                        false,
                        p,
                        ui_font,
                        Message::TriageAction {
                            source_rel: item.rel_path.clone(),
                            entry_id: item.entry.id,
                            action: "future".into(),
                        },
                    ),
                    mini_btn(
                        self.tr("× Done", "× انجام"),
                        false,
                        p,
                        ui_font,
                        Message::TriageAction {
                            source_rel: item.rel_path.clone(),
                            entry_id: item.entry.id,
                            action: "done".into(),
                        },
                    ),
                    mini_btn(
                        self.tr("~ Cancel", "~ لغو"),
                        false,
                        p,
                        ui_font,
                        Message::TriageAction {
                            source_rel: item.rel_path.clone(),
                            entry_id: item.entry.id,
                            action: "cancel".into(),
                        },
                    ),
                ]
                .spacing(4);

                let item_row = if ui_rtl {
                    row![btns, body, src].spacing(8).align_y(Alignment::Center)
                } else {
                    row![src, body, btns].spacing(8).align_y(Alignment::Center)
                };

                list = list.push(
                    container(item_row)
                        .width(Length::Fill)
                        .padding([5, 8])
                        .style(move |_| pane_style(p.surface_container, p.outline_variant)),
                );
            }
            col = col.push(scrollable(list).height(Length::Fixed(185.0)));
        }

        container(col)
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.tertiary))
            .into()
    }

    fn view_global_paper_panel(
        &self,
        p: Palette,
        ui_rtl: bool,
        ui_font: Font,
    ) -> Element<'_, Message> {
        let mut col = Column::new().spacing(6);
        let title = text(if ui_rtl {
            format!(
                "صف انتقال به دفتر کاغذی — {} مورد در کل بایگانی در انتظار انتقال",
                self.global_paper.len()
            )
        } else {
            format!(
                "Global Carry-to-Paper Queue — {} entries across vault waiting for notebook",
                self.global_paper.len()
            )
        })
        .size(12)
        .color(p.secondary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let sync_all_btn = pill_button(
            self.tr("Mark Entire Vault Synced", "ثبت انتقال کل بایگانی"),
            true,
            p,
            ui_font,
            Message::GlobalPaperSyncAll,
        );
        let close_btn = pill_button(
            self.tr("Close", "بستن"),
            false,
            p,
            ui_font,
            Message::ClosePanels,
        );

        let head = if ui_rtl {
            row![close_btn, sync_all_btn, Space::new().width(Length::Fill), title]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![title, Space::new().width(Length::Fill), sync_all_btn, close_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };
        col = col.push(head);

        if self.global_paper.is_empty() {
            col = col.push(
                text(self.tr(
                    "Every entry in your vault is synced to paper ([paper:ok])!",
                    "همه یادداشت‌های بایگانی به دفتر کاغذی منتقل شده‌اند!",
                ))
                .size(12)
                .color(p.tertiary)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .width(Length::Fill)
                .align_x(if ui_rtl {
                    Horizontal::Right
                } else {
                    Horizontal::Left
                }),
            );
        } else {
            let mut list = Column::new().spacing(4);
            for item in &self.global_paper {
                let is_rtl = self.is_text_rtl(&item.entry.text);
                let text_font = if is_rtl {
                    self.persian_text_font()
                } else {
                    ui_font
                };
                let src = text(format!(
                    "[{}] {}",
                    item.doc_title,
                    item.entry.signifier.symbol()
                ))
                .size(11)
                .color(p.on_surface_variant)
                .font(ui_font)
                .shaping(text::Shaping::Advanced);
                let body = text(item.entry.text.clone())
                    .size(12)
                    .color(p.on_surface)
                    .font(text_font)
                    .shaping(text::Shaping::Advanced)
                    .width(Length::Fill)
                    .align_x(if is_rtl || ui_rtl {
                        Horizontal::Right
                    } else {
                        Horizontal::Left
                    });
                let ok_btn = mini_btn(
                    self.tr("✓ Copied", "✓ منتقل شد"),
                    true,
                    p,
                    ui_font,
                    Message::GlobalPaperSyncOne {
                        rel_path: item.rel_path.clone(),
                        entry_id: item.entry.id,
                    },
                );
                let item_row = if ui_rtl {
                    row![ok_btn, body, src].spacing(8).align_y(Alignment::Center)
                } else {
                    row![src, body, ok_btn].spacing(8).align_y(Alignment::Center)
                };
                list = list.push(
                    container(item_row)
                        .width(Length::Fill)
                        .padding([5, 8])
                        .style(move |_| pane_style(p.surface_container, p.outline_variant)),
                );
            }
            col = col.push(scrollable(list).height(Length::Fixed(185.0)));
        }

        container(col)
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.secondary))
            .into()
    }

    fn view_reflect_panel(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        let title = text(self.tr(
            "AM / PM Reflection (BuJo Daily Ritual)",
            "بازتاب صبح و شب (آیین روزانه بولت‌ژورنال)",
        ))
        .size(12)
        .color(p.primary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);
        let close_btn = pill_button(
            self.tr("Close", "بستن"),
            false,
            p,
            ui_font,
            Message::ClosePanels,
        );
        let head = if ui_rtl {
            row![close_btn, Space::new().width(Length::Fill), title].align_y(Alignment::Center)
        } else {
            row![title, Space::new().width(Length::Fill), close_btn].align_y(Alignment::Center)
        };

        let am_label = text(self.tr("AM Focus", "تمرکز صبح"))
            .size(11)
            .color(p.tertiary)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .width(Length::Fixed(72.0))
            .align_x(if ui_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            });
        let am_in = text_input(
            self.tr(
                "AM: What matters most today? (logs as priority `*. ` action in Today)",
                "صبح: مهم‌ترین کار امروز چیست؟ (با نشانه اولویت *. در امروز ثبت می‌شود)",
            ),
            &self.am_focus_input,
        )
        .on_input(Message::AmFocusChanged)
        .on_submit(Message::SubmitAmFocus)
        .size(12)
        .font(ui_font)
        .padding([5, 8])
        .align_x(if ui_rtl || self.is_text_rtl(&self.am_focus_input) {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .style(move |_, status| input_style(p, status));
        let am_btn = pill_button(
            self.tr("+ Priority", "+ اولویت"),
            true,
            p,
            ui_font,
            Message::SubmitAmFocus,
        );
        let am_row = if ui_rtl {
            row![am_btn, am_in, am_label]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![am_label, am_in, am_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        let pm_label = text(self.tr("PM Reflect", "بازتاب شب"))
            .size(11)
            .color(p.secondary)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .width(Length::Fixed(72.0))
            .align_x(if ui_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            });
        let pm_in = text_input(
            self.tr(
                "PM: How did today feel or what stood out? (logs as `= ` emotion in Today)",
                "شب: امروز چه حسی داشتی یا چه اتفاقی برجسته بود؟ (با نشانه = ثبت می‌شود)",
            ),
            &self.pm_emotion_input,
        )
        .on_input(Message::PmEmotionChanged)
        .on_submit(Message::SubmitPmEmotion)
        .size(12)
        .font(ui_font)
        .padding([5, 8])
        .align_x(if ui_rtl || self.is_text_rtl(&self.pm_emotion_input) {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .style(move |_, status| input_style(p, status));
        let pm_btn = pill_button(
            self.tr("+ Emotion", "+ احساس"),
            true,
            p,
            ui_font,
            Message::SubmitPmEmotion,
        );
        let pm_row = if ui_rtl {
            row![pm_btn, pm_in, pm_label]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![pm_label, pm_in, pm_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        let check_txt = text(if ui_rtl {
            format!(
                "چک‌لیست شبانه: {} کار باز برای بررسی · {} مورد در انتظار انتقال به دفتر",
                self.open_triage.len(),
                self.global_paper.len()
            )
        } else {
            format!(
                "Evening checklist: {} open tasks to triage · {} entries pending Carry to Paper",
                self.open_triage.len(),
                self.global_paper.len()
            )
        })
        .size(11)
        .color(p.on_surface_variant)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let triage_btn = pill_button(
            self.tr("Open Triage", "باز کردن بررسی"),
            false,
            p,
            ui_font,
            Message::TogglePanel(ActivePanel::MigrationTriage),
        );
        let paper_btn = pill_button(
            self.tr("Paper Queue", "صف دفتر کاغذی"),
            false,
            p,
            ui_font,
            Message::TogglePanel(ActivePanel::GlobalPaperQueue),
        );
        let checklist = if ui_rtl {
            row![paper_btn, triage_btn, Space::new().width(Length::Fill), check_txt]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![check_txt, Space::new().width(Length::Fill), triage_btn, paper_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        container(column![head, am_row, pm_row, checklist].spacing(6))
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.primary))
            .into()
    }

    fn view_import_export_panel(
        &self,
        p: Palette,
        ui_rtl: bool,
        ui_font: Font,
    ) -> Element<'_, Message> {
        let title = text(self.tr(
            "Import & Export (Obsidian / Markdown / Apple Notes / JSON / CSV)",
            "ورود و خروج یادداشت‌ها (Obsidian / Markdown / JSON / CSV)",
        ))
        .size(12)
        .color(p.primary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);
        let close_btn = pill_button(
            self.tr("Close", "بستن"),
            false,
            p,
            ui_font,
            Message::ClosePanels,
        );
        let head = if ui_rtl {
            row![close_btn, Space::new().width(Length::Fill), title].align_y(Alignment::Center)
        } else {
            row![title, Space::new().width(Length::Fill), close_btn].align_y(Alignment::Center)
        };

        let fmt_row = row![
            text(self.tr("Format:", "قالب:"))
                .size(11)
                .color(p.on_surface_variant)
                .font(ui_font)
                .shaping(text::Shaping::Advanced),
            pill_button(
                self.tr("Auto", "خودکار"),
                self.import_format == ImportFormat::Auto,
                p,
                ui_font,
                Message::SetImportFormat(ImportFormat::Auto),
            ),
            pill_button(
                "Markdown",
                self.import_format == ImportFormat::Markdown,
                p,
                ui_font,
                Message::SetImportFormat(ImportFormat::Markdown),
            ),
            pill_button(
                "JSON",
                self.import_format == ImportFormat::Json,
                p,
                ui_font,
                Message::SetImportFormat(ImportFormat::Json),
            ),
            pill_button(
                "CSV",
                self.import_format == ImportFormat::Csv,
                p,
                ui_font,
                Message::SetImportFormat(ImportFormat::Csv),
            ),
            pill_button(
                self.tr("Text", "متن"),
                self.import_format == ImportFormat::Text,
                p,
                ui_font,
                Message::SetImportFormat(ImportFormat::Text),
            ),
            Space::new().width(Length::Fill),
            text(self.tr("Export:", "خروجی:"))
                .size(11)
                .color(p.on_surface_variant)
                .font(ui_font)
                .shaping(text::Shaping::Advanced),
            pill_button(".md", false, p, ui_font, Message::RunExport(ExportFormat::Markdown)),
            pill_button(".json", false, p, ui_font, Message::RunExport(ExportFormat::Json)),
            pill_button(".csv", false, p, ui_font, Message::RunExport(ExportFormat::Csv)),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let file_in = text_input(
            self.tr(
                "Import from file path (e.g. ~/Desktop/notes.md)...",
                "مسیر فایل برای ورود (مثلاً ~/Desktop/notes.md)...",
            ),
            &self.import_path_input,
        )
        .on_input(Message::ImportPathChanged)
        .on_submit(Message::RunImportFile)
        .size(12)
        .font(ui_font)
        .padding([5, 8])
        .style(move |_, status| input_style(p, status));
        let file_btn = pill_button(
            self.tr("Import File", "ورود از فایل"),
            true,
            p,
            ui_font,
            Message::RunImportFile,
        );
        let file_row = if ui_rtl {
            row![file_btn, file_in].spacing(6).align_y(Alignment::Center)
        } else {
            row![file_in, file_btn].spacing(6).align_y(Alignment::Center)
        };

        let paste_in = text_input(
            self.tr(
                "Or paste Markdown / checklists / JSON / CSV lines here...",
                "یا خطوط Markdown / چک‌لیست / JSON / CSV را اینجا جای‌گذاری کنید...",
            ),
            &self.import_paste_input,
        )
        .on_input(Message::ImportPasteChanged)
        .on_submit(Message::RunImportPaste)
        .size(12)
        .font(ui_font)
        .padding([5, 8])
        .align_x(if ui_rtl {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .style(move |_, status| input_style(p, status));
        let paste_btn = pill_button(
            self.tr("Import Pasted", "ورود متن"),
            true,
            p,
            ui_font,
            Message::RunImportPaste,
        );
        let paste_row = if ui_rtl {
            row![paste_btn, paste_in]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![paste_in, paste_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        container(column![head, fmt_row, file_row, paste_row].spacing(6))
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.primary))
            .into()
    }

    fn view_settings_panel(
        &self,
        p: Palette,
        ui_rtl: bool,
        ui_font: Font,
    ) -> Element<'_, Message> {
        let title = text(self.tr(
            "Settings — Vault, Notebook & Preferences (⌘,)",
            "تنظیمات — پوشه بایگانی، دفترچه و ترجیحات (⌘,)",
        ))
        .size(12)
        .color(p.primary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let close_btn = pill_button(
            self.tr("Close", "بستن"),
            false,
            p,
            ui_font,
            Message::ClosePanels,
        );

        let head = if ui_rtl {
            row![close_btn, Space::new().width(Length::Fill), title].align_y(Alignment::Center)
        } else {
            row![title, Space::new().width(Length::Fill), close_btn].align_y(Alignment::Center)
        };

        // 1. Vault Folder row
        let vault_label = text(self.tr("Vault Folder:", "پوشه بایگانی:"))
            .size(11)
            .color(p.on_surface_variant)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let vault_input = text_input(
            self.tr("~/Obsidian/BuJo-Vault", "~/Obsidian/BuJo-Vault"),
            &self.settings_vault_input,
        )
        .on_input(Message::SettingsVaultChanged)
        .on_submit(Message::SaveSettings)
        .size(12)
        .font(Font::DEFAULT)
        .padding([5, 8])
        .width(Length::Fill)
        .style(move |_, status| input_style(p, status));

        let browse_btn = pill_button(
            self.tr("Browse...", "انتخاب پوشه..."),
            false,
            p,
            ui_font,
            Message::SettingsBrowseVault,
        );
        let open_vault_btn = pill_button(
            self.tr("Open Folder", "باز کردن"),
            false,
            p,
            ui_font,
            Message::SettingsOpenVaultFolder,
        );

        let vault_row = if ui_rtl {
            row![open_vault_btn, browse_btn, vault_input, vault_label]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![vault_label, vault_input, browse_btn, open_vault_btn]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        // 2. Default Notebook Page & Default Spaces row
        let page_label = text(self.tr("Default Page:", "صفحه پیش‌فرض:"))
            .size(11)
            .color(p.on_surface_variant)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let page_input = text_input("1", &self.settings_page_input)
            .on_input(Message::SettingsPageChanged)
            .on_submit(Message::SaveSettings)
            .size(12)
            .font(Font::DEFAULT)
            .padding([5, 8])
            .width(Length::Fixed(72.0))
            .style(move |_, status| input_style(p, status));

        let spaces_label = text(self.tr("Default Spaces:", "فضاهای پیش‌فرض:"))
            .size(11)
            .color(p.on_surface_variant)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let spaces_input = text_input(
            "Studio & Work, Reading & Study, Personal",
            &self.settings_spaces_input,
        )
        .on_input(Message::SettingsSpacesChanged)
        .on_submit(Message::SaveSettings)
        .size(12)
        .font(ui_font)
        .padding([5, 8])
        .width(Length::Fill)
        .style(move |_, status| input_style(p, status));

        let meta_row = if ui_rtl {
            row![spaces_input, spaces_label, page_input, page_label]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![page_label, page_input, spaces_label, spaces_input]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        // 3. Appearance, Direction & Persian Font row
        let theme_label = text(self.tr("Theme:", "پوسته:"))
            .size(11)
            .color(p.on_surface_variant)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let dark_btn = pill_button(
            self.tr("Dark", "تیره"),
            self.is_dark,
            p,
            ui_font,
            if self.is_dark {
                Message::TogglePanel(ActivePanel::Settings)
            } else {
                Message::ToggleTheme
            },
        );
        let light_btn = pill_button(
            self.tr("Light", "روشن"),
            !self.is_dark,
            p,
            ui_font,
            if !self.is_dark {
                Message::TogglePanel(ActivePanel::Settings)
            } else {
                Message::ToggleTheme
            },
        );
        let dark_btn = if self.is_dark {
            container(
                text(self.tr("Dark", "تیره"))
                    .size(11)
                    .color(p.on_primary)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced),
            )
            .padding([4, 9])
            .style(move |_| container::Style {
                background: Some(p.primary.into()),
                border: Border {
                    color: p.primary,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .into()
        } else {
            dark_btn
        };
        let light_btn = if !self.is_dark {
            container(
                text(self.tr("Light", "روشن"))
                    .size(11)
                    .color(p.on_primary)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced),
            )
            .padding([4, 9])
            .style(move |_| container::Style {
                background: Some(p.primary.into()),
                border: Border {
                    color: p.primary,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
            .into()
        } else {
            light_btn
        };

        let dir_label = text(self.tr("Direction:", "جهت:"))
            .size(11)
            .color(p.on_surface_variant)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let font_label = text(self.tr("Persian Font:", "فونت فارسی:"))
            .size(11)
            .color(p.on_surface_variant)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let font_btn = pill_button(
            self.persian_font_label(),
            false,
            p,
            ui_font,
            Message::TogglePersianFont,
        );

        let prefs_row = row![
            theme_label,
            dark_btn,
            light_btn,
            Space::new().width(Length::Fixed(10.0)),
            dir_label,
            self.rtl_pill(p, ui_font, RtlMode::Ltr),
            self.rtl_pill(p, ui_font, RtlMode::Auto),
            self.rtl_pill(p, ui_font, RtlMode::Rtl),
            Space::new().width(Length::Fixed(10.0)),
            font_label,
            font_btn,
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        // 4. Updates row
        let auto_update_btn = pill_button(
            if self.config.auto_check_updates {
                self.tr("Auto-check Updates: ON", "بررسی خودکار به‌روزرسانی: روشن")
            } else {
                self.tr("Auto-check Updates: OFF", "بررسی خودکار به‌روزرسانی: خاموش")
            },
            self.config.auto_check_updates,
            p,
            ui_font,
            Message::ToggleAutoCheckUpdates,
        );

        let check_now_btn = pill_button(
            if self.update_checking {
                self.tr("Checking...", "در حال بررسی...")
            } else {
                self.tr("Check for Updates", "بررسی به‌روزرسانی")
            },
            false,
            p,
            ui_font,
            Message::CheckForUpdates,
        );

        let update_status_str = self
            .update_feedback
            .clone()
            .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));
        let update_status = text(update_status_str)
            .size(11)
            .color(p.tertiary)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);

        let mut updates_row = row![auto_update_btn, check_now_btn]
            .spacing(6)
            .align_y(Alignment::Center);

        if let Some(info) = &self.update_info
            && info.has_update
        {
            updates_row = updates_row.push(pill_button(
                if self.update_installing {
                    self.tr("Installing...", "در حال نصب...")
                } else {
                    self.tr("Install Update", "نصب به‌روزرسانی")
                },
                true,
                p,
                ui_font,
                Message::InstallUpdate,
            ));
        }

        updates_row = updates_row
            .push(Space::new().width(Length::Fixed(8.0)))
            .push(update_status);

        // 5. Config path + Save / Reset actions row
        let cfg_display = FolioConfig::config_path().to_string_lossy().to_string();
        let cfg_info = text(format!(
            "{} {cfg_display}",
            self.tr("Config file:", "فایل تنظیمات:")
        ))
        .size(10)
        .color(p.on_surface_variant)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let open_cfg_btn = pill_button(
            self.tr("Open Config Dir", "پوشه تنظیمات"),
            false,
            p,
            ui_font,
            Message::SettingsOpenConfigDir,
        );
        let reset_btn = pill_button(
            self.tr("Reset Defaults", "بازنشانی"),
            false,
            p,
            ui_font,
            Message::ResetSettingsDefaults,
        );
        let save_btn = pill_button(
            self.tr("Save & Apply", "ذخیره و اعمال"),
            true,
            p,
            ui_font,
            Message::SaveSettings,
        );

        let actions_row = if ui_rtl {
            row![
                save_btn,
                reset_btn,
                open_cfg_btn,
                Space::new().width(Length::Fill),
                cfg_info,
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        } else {
            row![
                cfg_info,
                Space::new().width(Length::Fill),
                open_cfg_btn,
                reset_btn,
                save_btn,
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        };

        container(column![head, vault_row, meta_row, prefs_row, updates_row, actions_row].spacing(8))
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.primary))
            .into()
    }

    fn view_search_results(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        let mut col = Column::new().spacing(4);
        let title = text(if ui_rtl {
            format!("نتایج جستجو ({})", self.search_hits.len())
        } else {
            format!("Search results ({})", self.search_hits.len())
        })
        .size(12)
        .color(p.primary)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);
        let close_btn = pill_button(
            self.tr("Close", "بستن"),
            false,
            p,
            ui_font,
            Message::ClearSearch,
        );
        let header = if ui_rtl {
            row![close_btn, Space::new().width(Length::Fill), title].align_y(Alignment::Center)
        } else {
            row![title, Space::new().width(Length::Fill), close_btn].align_y(Alignment::Center)
        };
        col = col.push(header);

        for hit in &self.search_hits {
            let hit_rtl = self.is_text_rtl(&hit.snippet);
            let hit_font = if hit_rtl {
                self.persian_text_font()
            } else {
                ui_font
            };
            let label = format!("{} — {}", hit.doc_title, hit.snippet);
            let btn_content = text(label)
                .size(12)
                .color(p.on_surface)
                .font(hit_font)
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

    fn view_nav_pane(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        // [1] SPACES (with Manage Spaces: Rename + Delete)
        let mut spaces_col = Column::new().spacing(4);
        spaces_col = spaces_col.push(section_header(
            self.tr("[1] SPACES", "[۱] فضاها"),
            p.primary,
            ui_rtl,
            ui_font,
        ));

        for sp in &self.spaces {
            if self.renaming_space_rel.as_deref() == Some(sp.rel_path.as_str()) {
                let ren_in = text_input(
                    self.tr("Rename space...", "نام جدید فضا..."),
                    &self.renaming_space_text,
                )
                .on_input(Message::RenameSpaceChanged)
                .on_submit(Message::SaveRenameSpace)
                .size(11)
                .font(ui_font)
                .padding([3, 6])
                .align_x(if ui_rtl {
                    Horizontal::Right
                } else {
                    Horizontal::Left
                })
                .style(move |_, status| input_style(p, status));
                spaces_col = spaces_col.push(
                    row![
                        ren_in,
                        mini_btn(
                            self.tr("OK", "ثبت"),
                            true,
                            p,
                            ui_font,
                            Message::SaveRenameSpace
                        ),
                        mini_btn("×", false, p, ui_font, Message::CancelRenameSpace),
                    ]
                    .spacing(3)
                    .align_y(Alignment::Center),
                );
                continue;
            }

            let active = self.doc.rel_path == sp.rel_path;
            let count_txt = text(format!("{}", sp.entry_count))
                .size(10)
                .color(if active {
                    p.primary
                } else {
                    p.on_surface_variant
                });
            let sp_display_name = if ui_rtl && sp.is_daily {
                "یادداشت امروز".to_string()
            } else {
                sp.name.clone()
            };
            let name_font = if self.is_text_rtl(&sp_display_name) {
                self.persian_text_font()
            } else {
                ui_font
            };
            let name_txt = text(sp_display_name)
                .size(12)
                .color(if active { p.primary } else { p.on_surface })
                .font(name_font)
                .shaping(text::Shaping::Advanced);

            let item_row = if ui_rtl {
                row![count_txt, Space::new().width(Length::Fill), name_txt]
                    .align_y(Alignment::Center)
            } else {
                row![name_txt, Space::new().width(Length::Fill), count_txt]
                    .align_y(Alignment::Center)
            };

            let open_btn = button(item_row)
                .width(Length::Fill)
                .padding([4, 6])
                .style(move |_, status| list_item_style(p, active, status))
                .on_press(Message::OpenDoc(sp.rel_path.clone()));

            if sp.is_daily {
                spaces_col = spaces_col.push(open_btn);
            } else {
                let ren_btn = mini_btn(
                    "✎",
                    false,
                    p,
                    ui_font,
                    Message::StartRenameSpace(sp.rel_path.clone(), sp.name.clone()),
                );
                let del_btn = mini_btn("×", false, p, ui_font, Message::DeleteSpace(sp.rel_path.clone()));
                spaces_col = spaces_col.push(if ui_rtl {
                    row![del_btn, ren_btn, open_btn]
                        .spacing(2)
                        .align_y(Alignment::Center)
                } else {
                    row![open_btn, ren_btn, del_btn]
                        .spacing(2)
                        .align_y(Alignment::Center)
                });
            }
        }

        // New Space creator
        let new_space_in = text_input(
            self.tr("New space...", "فضای جدید..."),
            &self.new_space_name,
        )
        .on_input(Message::NewSpaceChanged)
        .on_submit(Message::CreateSpace)
        .size(11)
        .font(ui_font)
        .padding([4, 6])
        .align_x(if ui_rtl {
            Horizontal::Right
        } else {
            Horizontal::Left
        })
        .style(move |_, status| input_style(p, status));
        let add_space_btn = pill_button("+", false, p, ui_font, Message::CreateSpace);
        spaces_col = spaces_col.push(if ui_rtl {
            row![add_space_btn, new_space_in]
                .spacing(4)
                .align_y(Alignment::Center)
        } else {
            row![new_space_in, add_space_btn]
                .spacing(4)
                .align_y(Alignment::Center)
        });

        let spaces_card = container(spaces_col)
            .width(Length::Fill)
            .padding(10)
            .style(move |_| pane_style(p.surface_low, p.outline_variant));

        // [2] VAULT TREE
        let mut vault_col = Column::new().spacing(2);
        vault_col = vault_col.push(section_header(
            self.tr("[2] VAULT", "[۲] بایگانی"),
            p.primary,
            ui_rtl,
            ui_font,
        ));

        for node in &self.vault_tree.nodes {
            if node.is_dir {
                vault_col = vault_col.push(
                    container(
                        text(if ui_rtl {
                            format!("{} ◂", node.label)
                        } else {
                            format!("▸ {}", node.label)
                        })
                        .size(11)
                        .color(p.on_surface_variant)
                        .font(ui_font)
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
                let label_font = if self.is_text_rtl(&node.label) {
                    self.persian_text_font()
                } else {
                    ui_font
                };

                vault_col = vault_col.push(
                    button(
                        text(node.label.clone())
                            .size(11)
                            .color(if active { p.primary } else { p.on_surface })
                            .font(label_font)
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
            .width(Length::Fixed(248.0))
            .height(Length::Fill)
            .into()
    }

    fn view_rapid_log_pane(
        &self,
        p: Palette,
        ui_rtl: bool,
        ui_font: Font,
    ) -> Element<'_, Message> {
        let title_font = if self.is_text_rtl(&self.doc.title) {
            self.persian_text_font()
        } else {
            ui_font
        };
        let title_txt = text(self.doc.title.clone())
            .size(18)
            .color(p.primary)
            .font(title_font)
            .shaping(text::Shaping::Advanced);

        let pending = self.doc.paper_pending_count();
        let pages_str = self.doc.format_pages();
        let meta_txt = text(if ui_rtl {
            format!(
                "{} · {} مورد · {} در انتظار دفتر",
                pages_str,
                self.doc.entries.len(),
                pending
            )
        } else {
            format!(
                "{} · {} entries · {} pending paper",
                pages_str,
                self.doc.entries.len(),
                pending
            )
        })
        .size(11)
        .color(p.on_surface_variant)
        .font(ui_font)
        .shaping(text::Shaping::Advanced);

        let thread_in = text_input(self.tr("p.#", "ص.#"), &self.thread_page_input)
            .on_input(Message::ThreadPageChanged)
            .on_submit(Message::AddThreadedPage)
            .size(11)
            .font(ui_font)
            .padding([3, 6])
            .width(Length::Fixed(52.0))
            .style(move |_, status| input_style(p, status));
        let thread_btn = pill_button(
            self.tr("+ Thread", "+ پیوند"),
            false,
            p,
            ui_font,
            Message::AddThreadedPage,
        );
        let thread_ctrl = if ui_rtl {
            row![thread_btn, thread_in]
                .spacing(4)
                .align_y(Alignment::Center)
        } else {
            row![thread_in, thread_btn]
                .spacing(4)
                .align_y(Alignment::Center)
        };

        let doc_header = if ui_rtl {
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
        if ui_rtl {
            filter_row = filter_row.push(Space::new().width(Length::Fill));
        }
        let filters = if ui_rtl {
            [
                EntryFilter::Note,
                EntryFilter::Emotion,
                EntryFilter::Event,
                EntryFilter::Action,
                EntryFilter::All,
            ]
        } else {
            [
                EntryFilter::All,
                EntryFilter::Action,
                EntryFilter::Event,
                EntryFilter::Emotion,
                EntryFilter::Note,
            ]
        };
        for f in filters {
            filter_row = filter_row.push(pill_button(
                self.filter_label(f),
                self.filter == f,
                p,
                ui_font,
                Message::SetFilter(f),
            ));
        }

        let mut tags_row = Row::new().spacing(4).align_y(Alignment::Center);
        if ui_rtl {
            tags_row = tags_row.push(Space::new().width(Length::Fill));
        }
        for tag in &self.inline_tags {
            let active = self.tag_filter.as_deref() == Some(tag.as_str());
            let tag_font = if self.is_text_rtl(tag) {
                self.persian_text_font()
            } else {
                ui_font
            };
            tags_row = tags_row.push(pill_button(
                tag,
                active,
                p,
                tag_font,
                Message::ToggleTagFilter(tag.clone()),
            ));
        }

        // Composer
        let composer = self.view_composer(p, ui_rtl, ui_font);

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

        // Prose lines (for Reviews / Monthly Log / Future Log / Index)
        if !self.doc.prose_lines.is_empty() {
            let mut prose_box = Column::new().spacing(3);
            for line in &self.doc.prose_lines {
                if !line.trim().is_empty() {
                    let line_rtl = self.is_text_rtl(line);
                    let line_font = if line_rtl {
                        self.persian_text_font()
                    } else {
                        ui_font
                    };
                    prose_box = prose_box.push(
                        text(line.clone())
                            .size(12)
                            .color(p.on_surface_variant)
                            .font(line_font)
                            .shaping(text::Shaping::Advanced)
                            .width(Length::Fill)
                            .align_x(if line_rtl || ui_rtl {
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
                    text(self.tr(
                        "No entries yet. Press ⌘N / Ctrl+N or use the rapid-log bar above to add one.",
                        "هنوز موردی ثبت نشده. برای ثبت سریع کلید ⌘N / Ctrl+N را بزنید یا در کادر بالا بنویسید.",
                    ))
                    .size(12)
                    .color(p.on_surface_variant)
                    .font(ui_font)
                    .shaping(text::Shaping::Advanced)
                    .width(Length::Fill)
                    .align_x(if ui_rtl {
                        Horizontal::Right
                    } else {
                        Horizontal::Left
                    }),
                )
                .padding(16),
            );
        } else {
            for entry in filtered_entries {
                entries_col = entries_col.push(self.view_entry_row(entry, p, ui_rtl, ui_font));
                if self.migrating_entry_id == Some(entry.id) {
                    entries_col =
                        entries_col.push(self.view_migrate_bar(entry.id, p, ui_rtl, ui_font));
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

    fn view_composer(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        let sig_pills = row![
            pill_button(
                ".",
                self.composer_sig == Signifier::Action,
                p,
                ui_font,
                Message::SetComposerSig(Signifier::Action),
            ),
            pill_button(
                "o",
                self.composer_sig == Signifier::Event,
                p,
                ui_font,
                Message::SetComposerSig(Signifier::Event),
            ),
            pill_button(
                "=",
                self.composer_sig == Signifier::Emotion,
                p,
                ui_font,
                Message::SetComposerSig(Signifier::Emotion),
            ),
            pill_button(
                "-",
                self.composer_sig == Signifier::Note,
                p,
                ui_font,
                Message::SetComposerSig(Signifier::Note),
            ),
            pill_button(
                "*",
                self.composer_mod == EntryModifier::Priority,
                p,
                ui_font,
                Message::ToggleComposerMod(EntryModifier::Priority),
            ),
            pill_button(
                "!",
                self.composer_mod == EntryModifier::Inspiration,
                p,
                ui_font,
                Message::ToggleComposerMod(EntryModifier::Inspiration),
            ),
        ]
        .spacing(3);

        let composer_rtl = ui_rtl || self.is_text_rtl(&self.composer_text);
        let composer_font = if composer_rtl {
            self.persian_text_font()
        } else {
            ui_font
        };
        let placeholder = self.tr(
            "Rapid log (⌘N)... prefix `. `, `o `, `= `, `- `, `*. `, `!- `",
            "ثبت سریع (⌘N)... با پیشوند . برای کار، o برای رویداد، = برای احساس، - برای یادداشت",
        );

        let input = text_input(placeholder, &self.composer_text)
            .id(COMPOSER_ID)
            .on_input(Message::ComposerChanged)
            .on_submit(Message::SubmitEntry)
            .size(13)
            .font(composer_font)
            .padding([6, 10])
            .align_x(if composer_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            })
            .style(move |_, status| input_style(p, status));

        let submit_btn = button(
            text(self.tr("+ Log", "+ ثبت"))
                .size(12)
                .color(p.on_primary)
                .font(ui_font)
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

    fn view_entry_row(
        &self,
        entry: &Entry,
        p: Palette,
        ui_rtl: bool,
        ui_font: Font,
    ) -> Element<'_, Message> {
        let entry_rtl = self.is_text_rtl(&entry.text);
        let indent_px = (entry.indent.min(3) as f32) * 20.0;

        if self.editing_entry_id == Some(entry.id) {
            let edit_rtl = ui_rtl || self.is_text_rtl(&self.editing_text);
            let edit_font = if edit_rtl {
                self.persian_text_font()
            } else {
                ui_font
            };
            let edit_in = text_input(
                self.tr("Edit entry...", "ویرایش یادداشت..."),
                &self.editing_text,
            )
            .on_input(Message::EditTextChanged)
            .on_submit(Message::SaveEdit)
            .size(13)
            .font(edit_font)
            .padding([5, 8])
            .align_x(if edit_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            })
            .style(move |_, status| input_style(p, status));

            let save_btn = pill_button(self.tr("Save", "ذخیره"), true, p, ui_font, Message::SaveEdit);
            let cancel_btn =
                pill_button(self.tr("Cancel", "انصراف"), false, p, ui_font, Message::CancelEdit);

            let edit_row = if ui_rtl {
                row![cancel_btn, save_btn, edit_in]
                    .spacing(6)
                    .align_y(Alignment::Center)
            } else {
                row![edit_in, save_btn, cancel_btn]
                    .spacing(6)
                    .align_y(Alignment::Center)
            };

            return container(edit_row)
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

        let body_font = if entry_rtl {
            self.persian_text_font()
        } else {
            ui_font
        };
        let body_txt = text(body_display)
            .size(13)
            .color(text_color)
            .font(body_font)
            .shaping(text::Shaping::Advanced)
            .width(Length::Fill)
            .align_x(if entry_rtl || ui_rtl {
                Horizontal::Right
            } else {
                Horizontal::Left
            });

        let paper_label = if entry.paper_synced {
            self.tr("ok", "دفتر✓")
        } else {
            self.tr("paper", "دفتر")
        };
        let actions = row![
            mini_btn("↻", false, p, ui_font, Message::CycleEntrySignifier(entry.id)),
            mini_btn("←", false, p, ui_font, Message::IndentEntry(entry.id, -1)),
            mini_btn("→", false, p, ui_font, Message::IndentEntry(entry.id, 1)),
            mini_btn(
                "~",
                entry.signifier == Signifier::Cancelled,
                p,
                ui_font,
                Message::ToggleEntryCancelled(entry.id),
            ),
            mini_btn(
                "*",
                entry.modifier == EntryModifier::Priority,
                p,
                ui_font,
                Message::ToggleEntryPriority(entry.id),
            ),
            mini_btn(
                "!",
                entry.modifier == EntryModifier::Inspiration,
                p,
                ui_font,
                Message::ToggleEntryInspiration(entry.id),
            ),
            mini_btn(
                paper_label,
                entry.paper_synced,
                p,
                ui_font,
                Message::ToggleEntryPaper(entry.id),
            ),
            mini_btn(
                ">",
                self.migrating_entry_id == Some(entry.id),
                p,
                ui_font,
                Message::ToggleMigratePicker(entry.id),
            ),
            mini_btn(
                self.tr("Edit", "ویرایش"),
                false,
                p,
                ui_font,
                Message::StartEdit(entry.id, entry.text.clone()),
            ),
            mini_btn("×", false, p, ui_font, Message::DeleteEntry(entry.id)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        // Keep columns stable in AUTO/LTR mode; only mirror the row layout in full RTL (Persian) mode.
        let entry_row = if ui_rtl {
            row![actions, body_txt, sig_btn, mod_txt, time_txt]
                .spacing(6)
                .align_y(Alignment::Center)
        } else {
            row![time_txt, mod_txt, sig_btn, body_txt, actions]
                .spacing(6)
                .align_y(Alignment::Center)
        };

        let pad = if ui_rtl {
            Padding {
                top: 5.0,
                right: 8.0 + indent_px,
                bottom: 5.0,
                left: 8.0,
            }
        } else {
            Padding {
                top: 5.0,
                right: 8.0,
                bottom: 5.0,
                left: 8.0 + indent_px,
            }
        };

        container(entry_row)
            .width(Length::Fill)
            .padding(pad)
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

    fn view_migrate_bar(
        &self,
        entry_id: usize,
        p: Palette,
        ui_rtl: bool,
        ui_font: Font,
    ) -> Element<'_, Message> {
        let base_date = self.doc.date.unwrap_or_else(Vault::today_date);
        let tomorrow_rel = Vault::daily_rel_path(base_date + Duration::days(1));

        let mut bar = Row::new().spacing(4).align_y(Alignment::Center);
        bar = bar.push(
            text(self.tr("Migrate / Schedule:", "انتقال / زمان‌بندی:"))
                .size(11)
                .color(p.tertiary)
                .font(ui_font)
                .shaping(text::Shaping::Advanced),
        );
        bar = bar.push(pill_button(
            self.tr("Tomorrow (>)", "فردا (>)"),
            false,
            p,
            ui_font,
            Message::MigrateTo {
                entry_id,
                target_rel: tomorrow_rel,
                scheduled: false,
            },
        ));

        // Structured Future Log month buckets (next 3 months)
        for ym in Vault::upcoming_months(Vault::today_date(), 4)
            .into_iter()
            .skip(1)
        {
            bar = bar.push(pill_button(
                &format!("{ym} (<)"),
                false,
                p,
                ui_font,
                Message::ScheduleToMonth {
                    entry_id,
                    month_ym: ym,
                },
            ));
        }

        for sp in &self.spaces {
            if !sp.is_daily && sp.rel_path != self.doc.rel_path {
                let sp_font = if self.is_text_rtl(&sp.name) {
                    self.persian_text_font()
                } else {
                    ui_font
                };
                bar = bar.push(pill_button(
                    &sp.name,
                    false,
                    p,
                    sp_font,
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

    fn view_inspector_pane(&self, p: Palette, ui_rtl: bool, ui_font: Font) -> Element<'_, Message> {
        // [4] SIGNIFIERS
        let sig_lines = if ui_rtl {
            [
                (".", "کار (باز)"),
                ("×", "انجام‌شده"),
                ("~", "لغوشده / بی‌اهمیت"),
                (">", "منتقل‌شده"),
                ("<", "زمان‌بندی‌شده"),
                ("o", "رویداد"),
                ("=", "احساس"),
                ("-", "یادداشت"),
                ("*", "اولویت"),
                ("!", "الهام / ایده"),
            ]
        } else {
            [
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
            ]
        };

        let mut sig_col = Column::new().spacing(2);
        sig_col = sig_col.push(section_header(
            self.tr("[4] SIGNIFIERS", "[۴] نشانه‌ها"),
            p.primary,
            ui_rtl,
            ui_font,
        ));
        for (sym, desc) in sig_lines {
            let s = text(sym).size(12).color(p.primary).width(Length::Fixed(16.0));
            let d = text(desc)
                .size(11)
                .color(p.on_surface_variant)
                .font(ui_font)
                .shaping(text::Shaping::Advanced);
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

        // [5] CARRY TO PAPER (Current doc + Global Vault Queue)
        let pending = self.doc.paper_pending_count();
        let synced = self.doc.entries.len().saturating_sub(pending);
        let global_pending = self.global_paper.len();
        let paper_summary = text(if ui_rtl {
            format!(
                "این صفحه: {pending} در انتظار · {synced} منتقل‌شده\nکل بایگانی: {global_pending} در انتظار"
            )
        } else {
            format!("Doc: {pending} pending · {synced} ok\nVault total: {global_pending} pending")
        })
        .size(11)
        .color(if global_pending > 0 {
            p.secondary
        } else {
            p.tertiary
        })
        .font(ui_font)
        .shaping(text::Shaping::Advanced)
        .width(Length::Fill)
        .align_x(if ui_rtl {
            Horizontal::Right
        } else {
            Horizontal::Left
        });

        let mark_all_btn = button(
            text(self.tr("Mark Doc Synced", "ثبت انتقال این صفحه"))
                .size(11)
                .color(p.on_primary)
                .font(ui_font)
                .shaping(text::Shaping::Advanced)
                .width(Length::Fill)
                .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .padding([5, 8])
        .style(move |_, status| primary_btn_style(p, status))
        .on_press(Message::MarkAllPaperSynced);

        let global_queue_btn = button(
            text(if ui_rtl {
                format!("صف انتقال کل بایگانی ({global_pending})")
            } else {
                format!("Global Paper Queue ({global_pending})")
            })
            .size(11)
            .color(p.on_surface)
            .font(ui_font)
            .shaping(text::Shaping::Advanced)
            .width(Length::Fill)
            .align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .padding([5, 8])
        .style(move |_, status| secondary_btn_style(p, status))
        .on_press(Message::TogglePanel(ActivePanel::GlobalPaperQueue));

        let paper_card = container(
            column![
                section_header(
                    self.tr("[5] CARRY TO PAPER", "[۵] انتقال به دفتر"),
                    p.secondary,
                    ui_rtl,
                    ui_font,
                ),
                paper_summary,
                mark_all_btn,
                global_queue_btn,
            ]
            .spacing(6),
        )
        .width(Length::Fill)
        .padding(10)
        .style(move |_| pane_style(p.surface_low, p.outline_variant));

        // [6] .MD BUFFER
        let md_toggle = pill_button(
            if self.show_md_preview {
                self.tr("Hide", "پنهان")
            } else {
                self.tr("Show", "نمایش")
            },
            false,
            p,
            ui_font,
            Message::ToggleMdPreview,
        );
        let md_title = text(self.tr("[6] .MD BUFFER", "[۶] پیش‌نمایش .MD"))
            .size(11)
            .color(p.primary)
            .font(ui_font)
            .shaping(text::Shaping::Advanced);
        let md_head = if ui_rtl {
            row![md_toggle, Space::new().width(Length::Fill), md_title].align_y(Alignment::Center)
        } else {
            row![md_title, Space::new().width(Length::Fill), md_toggle].align_y(Alignment::Center)
        };

        let mut md_col = Column::new().spacing(6);
        md_col = md_col.push(md_head);
        if self.show_md_preview {
            md_col = md_col.push(
                scrollable(
                    text(self.raw_markdown.clone())
                        .size(11)
                        .color(p.on_surface_variant)
                        .font(ui_font)
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

fn section_header<'a>(
    title: &'a str,
    color: Color,
    ui_rtl: bool,
    font: Font,
) -> Element<'a, Message> {
    text(title)
        .size(11)
        .color(color)
        .font(font)
        .shaping(text::Shaping::Advanced)
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
    font: Font,
    msg: Message,
) -> Element<'a, Message> {
    button(
        text(label.to_string())
            .size(11)
            .color(if active { p.on_primary } else { p.on_surface })
            .font(font)
            .shaping(text::Shaping::Advanced),
    )
    .padding([4, 7])
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

fn mini_btn<'a>(
    label: &str,
    active: bool,
    p: Palette,
    font: Font,
    msg: Message,
) -> Element<'a, Message> {
    button(
        text(label.to_string())
            .size(10)
            .color(if active {
                p.on_primary
            } else {
                p.on_surface_variant
            })
            .font(font)
            .shaping(text::Shaping::Advanced)
            .align_y(Vertical::Center),
    )
    .padding([2, 4])
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

/// Detect whether Vazirmatn and/or Dastkhat are installed in standard OS font directories
/// (including subfolders created by `open-fonts-installer`). Adds 0 bytes to the binary.
fn detect_installed_persian_fonts() -> (bool, bool) {
    let mut dirs_to_scan: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs_to_scan.push(home.join("Library").join("Fonts"));
        dirs_to_scan.push(home.join(".local").join("share").join("fonts"));
        dirs_to_scan.push(home.join(".fonts"));
    }
    if let Some(local_data) = dirs::data_local_dir() {
        dirs_to_scan.push(local_data.join("Microsoft").join("Windows").join("Fonts"));
    }
    dirs_to_scan.push(PathBuf::from("/Library/Fonts"));
    dirs_to_scan.push(PathBuf::from("/usr/share/fonts"));
    dirs_to_scan.push(PathBuf::from("/usr/local/share/fonts"));
    dirs_to_scan.push(PathBuf::from(r"C:\Windows\Fonts"));

    let mut has_vazir = false;
    let mut has_dastkhat = false;
    for dir in dirs_to_scan {
        scan_font_dir(&dir, 0, &mut has_vazir, &mut has_dastkhat);
        if has_vazir && has_dastkhat {
            break;
        }
    }
    (has_vazir, has_dastkhat)
}

fn scan_font_dir(dir: &Path, depth: usize, has_vazir: &mut bool, has_dastkhat: &mut bool) {
    if depth > 3 || !dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_font_dir(&path, depth + 1, has_vazir, has_dastkhat);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".ttf") || lower.ends_with(".otf") {
                if lower.contains("vazir") {
                    *has_vazir = true;
                }
                if lower.contains("dastkhat") {
                    *has_dastkhat = true;
                }
            }
        }
    }
}

/// Open a native OS directory chooser dialog without pulling in extra native crates.
fn pick_folder_dialog(initial: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let start = initial.to_string_lossy().replace('"', "\\\"");
        let script = format!(
            "try\nPOSIX path of (choose folder with prompt \"Choose Folio Vault Folder\" default location POSIX file \"{start}\")\non error\nPOSIX path of (choose folder with prompt \"Choose Folio Vault Folder\")\nend try"
        );
        let out = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let chosen = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if chosen.is_empty() {
            None
        } else {
            Some(PathBuf::from(chosen.trim_end_matches('/')))
        }
    }
    #[cfg(target_os = "windows")]
    {
        let start = initial.to_string_lossy().replace('\'', "''");
        let ps = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = 'Choose Folio Vault Folder'; if (Test-Path '{start}') {{ $d.SelectedPath = '{start}' }}; if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.SelectedPath }}"
        );
        let out = Command::new("pwsh")
            .args(["-NoProfile", "-STA", "-Command", &ps])
            .output()
            .or_else(|_| {
                Command::new("powershell")
                    .args(["-NoProfile", "-STA", "-Command", &ps])
                    .output()
            })
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let chosen = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if chosen.is_empty() {
            None
        } else {
            Some(PathBuf::from(chosen))
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let start = initial.to_string_lossy().to_string();
        let out = Command::new("zenity")
            .args([
                "--file-selection",
                "--directory",
                "--title=Choose Folio Vault Folder",
                &format!("--filename={start}/"),
            ])
            .output()
            .or_else(|_| {
                Command::new("kdialog")
                    .args(["--getexistingdirectory", &start])
                    .output()
            })
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let chosen = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if chosen.is_empty() {
            None
        } else {
            Some(PathBuf::from(chosen))
        }
    }
}

/// Launch `open-fonts-installer` (https://github.com/HamidRezaAshkiyan/open-fonts-installer)
/// in the background to install Vazirmatn & Dastkhat without bundling font binaries in Folio.
fn install_persian_fonts_via_open_fonts_installer() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("pwsh")
            .args([
                "-NoProfile",
                "-Command",
                "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/HamidRezaAshkiyan/open-fonts-installer/main/install-fonts.ps1))) -Fonts vazirmatn,dastkhat",
            ])
            .spawn()?;
        Ok("در حال نصب وزیرمتن و دست‌خط با open-fonts-installer — پس از پایان برنامه را مجدداً باز کنید".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("bash")
            .args([
                "-c",
                "curl -fsSL https://raw.githubusercontent.com/HamidRezaAshkiyan/open-fonts-installer/main/install-fonts.sh | bash -s -- vazirmatn dastkhat",
            ])
            .spawn()?;
        Ok("در حال نصب وزیرمتن و دست‌خط با open-fonts-installer — پس از پایان برنامه را مجدداً باز کنید".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn scans_font_subfolders_for_vazirmatn_and_dastkhat() {
        let dir = tempdir().unwrap();
        let sub_v = dir.path().join("vazirmatn");
        let sub_d = dir.path().join("dastkhat");
        fs::create_dir_all(&sub_v).unwrap();
        fs::create_dir_all(&sub_d).unwrap();
        fs::write(sub_v.join("Vazirmatn-Regular.ttf"), b"fake").unwrap();
        fs::write(sub_d.join("Dastkhat[wght].ttf"), b"fake").unwrap();

        let (mut has_v, mut has_d) = (false, false);
        scan_font_dir(dir.path(), 0, &mut has_v, &mut has_d);
        assert!(has_v);
        assert!(has_d);
    }
}
