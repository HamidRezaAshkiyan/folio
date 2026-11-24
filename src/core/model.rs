use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bullet Journal signifiers supported by Folio.
/// Core rapid-log signifiers:
///   `.` Action (open task)
///   `o` Event
///   `=` Emotion
///   `-` Note
/// Action lifecycle states:
///   `×` Completed action
///   `>` Migrated action
///   `<` Scheduled action (Future Log)
///   `~` Irrelevant / cancelled action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Signifier {
    Action,
    Done,
    Migrated,
    Scheduled,
    Cancelled,
    Event,
    Emotion,
    Note,
}

impl Signifier {
    pub fn symbol(&self) -> &'static str {
        match self {
            Signifier::Action => ".",
            Signifier::Done => "×",
            Signifier::Migrated => ">",
            Signifier::Scheduled => "<",
            Signifier::Cancelled => "~",
            Signifier::Event => "o",
            Signifier::Emotion => "=",
            Signifier::Note => "-",
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Signifier::Action => "Action",
            Signifier::Done => "Done",
            Signifier::Migrated => "Migrated",
            Signifier::Scheduled => "Scheduled",
            Signifier::Cancelled => "Cancelled",
            Signifier::Event => "Event",
            Signifier::Emotion => "Emotion",
            Signifier::Note => "Note",
        }
    }

    pub fn from_symbol(s: &str) -> Option<Self> {
        match s.trim() {
            "." | "•" => Some(Signifier::Action),
            "×" | "x" | "X" => Some(Signifier::Done),
            ">" => Some(Signifier::Migrated),
            "<" => Some(Signifier::Scheduled),
            "~" => Some(Signifier::Cancelled),
            "o" | "O" | "○" => Some(Signifier::Event),
            "=" => Some(Signifier::Emotion),
            "-" | "—" => Some(Signifier::Note),
            _ => None,
        }
    }

    pub fn is_action_family(&self) -> bool {
        matches!(
            self,
            Signifier::Action
                | Signifier::Done
                | Signifier::Migrated
                | Signifier::Scheduled
                | Signifier::Cancelled
        )
    }

    pub fn toggle_done(self) -> Self {
        match self {
            Signifier::Action => Signifier::Done,
            Signifier::Done => Signifier::Action,
            Signifier::Migrated | Signifier::Scheduled | Signifier::Cancelled => Signifier::Action,
            other => other,
        }
    }

    /// Toggle Irrelevant / Cancelled (`~`) state.
    pub fn toggle_cancelled(self) -> Self {
        match self {
            Signifier::Cancelled => Signifier::Action,
            _ => Signifier::Cancelled,
        }
    }

    pub fn matches_filter(&self, filter: EntryFilter) -> bool {
        match filter {
            EntryFilter::All => true,
            EntryFilter::Action => self.is_action_family(),
            EntryFilter::Event => *self == Signifier::Event,
            EntryFilter::Emotion => *self == Signifier::Emotion,
            EntryFilter::Note => *self == Signifier::Note,
        }
    }
}

/// Classic Bullet Journal signifier modifiers:
///   `*` Priority (important)
///   `!` Inspiration / great idea / insight
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryModifier {
    #[default]
    None,
    Priority,
    Inspiration,
}

impl EntryModifier {
    pub fn symbol(&self) -> &'static str {
        match self {
            EntryModifier::None => "",
            EntryModifier::Priority => "*",
            EntryModifier::Inspiration => "!",
        }
    }

    pub fn toggle_priority(self) -> Self {
        if self == EntryModifier::Priority {
            EntryModifier::None
        } else {
            EntryModifier::Priority
        }
    }

    pub fn toggle_inspiration(self) -> Self {
        if self == EntryModifier::Inspiration {
            EntryModifier::None
        } else {
            EntryModifier::Inspiration
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryFilter {
    All,
    Action,
    Event,
    Emotion,
    Note,
}

impl EntryFilter {
    pub fn label(&self) -> &'static str {
        match self {
            EntryFilter::All => "ALL",
            EntryFilter::Action => "ACTION (.)",
            EntryFilter::Event => "EVENT (o)",
            EntryFilter::Emotion => "EMOTION (=)",
            EntryFilter::Note => "NOTE (-)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: usize,
    pub time: String,
    #[serde(default)]
    pub modifier: EntryModifier,
    pub signifier: Signifier,
    pub text: String,
    pub paper_synced: bool,
}

impl Entry {
    pub fn new(
        id: usize,
        time: impl Into<String>,
        signifier: Signifier,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id,
            time: time.into(),
            modifier: EntryModifier::None,
            signifier,
            text: text.into(),
            paper_synced: false,
        }
    }

    pub fn with_modifier(mut self, modifier: EntryModifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Extract inline `#tags` (e.g. `#studio`, `#reading`, `#یادداشت`) from entry text.
    pub fn inline_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        for token in self.text.split_whitespace() {
            if let Some(rest) = token.strip_prefix('#') {
                let clean: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '\u{200C}')
                    .collect();
                if !clean.is_empty() {
                    let tag = format!("#{}", clean.to_lowercase());
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
            }
        }
        tags
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        let needle = if tag.starts_with('#') {
            tag.to_lowercase()
        } else {
            format!("#{}", tag.to_lowercase())
        };
        self.inline_tags().iter().any(|t| t == &needle)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocKind {
    Daily,
    Space,
    Review,
    FutureLog,
    Index,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub path: PathBuf,
    pub rel_path: String,
    pub title: String,
    pub kind: DocKind,
    pub date: Option<NaiveDate>,
    pub page: Option<u32>,
    /// Additional physical notebook pages for BuJo Threading (e.g. `p. 12, 28, 45`).
    #[serde(default)]
    pub threaded_pages: Vec<u32>,
    pub space: Option<String>,
    pub tags: Vec<String>,
    pub extra_frontmatter: Vec<(String, String)>,
    pub entries: Vec<Entry>,
    /// Freeform markdown lines (headings, review prose, etc.) outside rapid-log lines.
    pub prose_lines: Vec<String>,
}

impl Document {
    pub fn paper_pending_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.paper_synced).count()
    }

    pub fn next_entry_id(&self) -> usize {
        self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }

    #[allow(dead_code)]
    pub fn add_entry(&mut self, time: String, signifier: Signifier, text: String) -> usize {
        self.add_entry_with_modifier(time, EntryModifier::None, signifier, text)
    }

    pub fn add_entry_with_modifier(
        &mut self,
        time: String,
        modifier: EntryModifier,
        signifier: Signifier,
        text: String,
    ) -> usize {
        let id = self.next_entry_id();
        self.entries
            .push(Entry::new(id, time, signifier, text).with_modifier(modifier));
        id
    }

    /// Collect all unique inline `#tags` present in this document's entries.
    pub fn collect_inline_tags(&self) -> Vec<String> {
        let mut all = Vec::new();
        for e in &self.entries {
            for t in e.inline_tags() {
                if !all.contains(&t) {
                    all.push(t);
                }
            }
        }
        all.sort();
        all
    }

    /// Return all unique physical notebook page numbers associated with this document
    /// (primary `page` + `threaded_pages`), sorted ascending.
    pub fn all_pages(&self) -> Vec<u32> {
        let mut pages = Vec::new();
        if let Some(p) = self.page {
            pages.push(p);
        }
        for &p in &self.threaded_pages {
            if !pages.contains(&p) {
                pages.push(p);
            }
        }
        pages.sort_unstable();
        pages
    }

    /// Add a threaded physical notebook page (BuJo Threading) to this document.
    pub fn add_threaded_page(&mut self, page: u32) {
        if page == 0 {
            return;
        }
        if self.page.is_none() {
            self.page = Some(page);
            return;
        }
        if self.page != Some(page) && !self.threaded_pages.contains(&page) {
            self.threaded_pages.push(page);
            self.threaded_pages.sort_unstable();
        }
    }

    /// Format physical notebook pages as `p. 12, 28, 45` or `—`.
    pub fn format_pages(&self) -> String {
        let pages = self.all_pages();
        if pages.is_empty() {
            "—".to_string()
        } else {
            let joined = pages
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("p. {joined}")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceInfo {
    pub name: String,
    pub slug: String,
    pub is_daily: bool,
    pub rel_path: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFileNode {
    pub label: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultTree {
    pub vault_name: String,
    pub vault_root: String,
    pub nodes: Vec<VaultFileNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub rel_path: String,
    pub doc_title: String,
    pub entry_id: Option<usize>,
    pub time: Option<String>,
    pub signifier: Option<Signifier>,
    pub modifier: EntryModifier,
    pub snippet: String,
}

/// Convert a human space name like "Studio & Work" or "مطالعه و کار" to a clean filename slug.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// Returns true if a character is a strong Right-to-Left (RTL) Unicode letter
/// (Hebrew, Arabic, Persian, Syriac, Thaana, NKo, etc.).
pub fn is_rtl_char(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic & Persian
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x07C0..=0x07FF // NKo
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB1D..=0xFB4F // Hebrew Presentation Forms
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
    )
}

/// Detect whether text should be treated as Right-to-Left using the Unicode BiDi
/// "first strong directional character" heuristic (ignoring digits, punctuation,
/// whitespace, and signifiers).
pub fn is_rtl_text(text: &str) -> bool {
    for c in text.chars() {
        if is_rtl_char(c) {
            return true;
        }
        if c.is_alphabetic() {
            return false;
        }
    }
    false
}

/// Parse an optional leading rapid-log modifier + signifier prefix such as
/// `. `, `o `, `= `, `- `, `*. `, `!. `, `!- `, `*o `.
pub fn parse_rapid_input_full(
    raw: &str,
    default_sig: Signifier,
) -> (EntryModifier, Signifier, String) {
    let trimmed = raw.trim();

    // Check 2-char modifier+signifier prefix like `*. `, `!- `, `*o `
    let mut chars = trimmed.chars();
    if let (Some(c1), Some(c2), Some(c3)) = (chars.next(), chars.next(), chars.next())
        && (c1 == '*' || c1 == '!')
        && c3.is_whitespace()
        && let Some(sig) = Signifier::from_symbol(&c2.to_string())
    {
        let modifier = if c1 == '*' {
            EntryModifier::Priority
        } else {
            EntryModifier::Inspiration
        };
        let rest = trimmed[c1.len_utf8() + c2.len_utf8()..].trim().to_string();
        return (modifier, sig, rest);
    }

    // Check 1-char modifier only like `* text` or `! text`
    let mut chars = trimmed.chars();
    if let (Some(c1), Some(c2)) = (chars.next(), chars.next())
        && c2.is_whitespace()
    {
        if c1 == '*' {
            return (
                EntryModifier::Priority,
                default_sig,
                trimmed[1..].trim().to_string(),
            );
        }
        if c1 == '!' {
            return (
                EntryModifier::Inspiration,
                default_sig,
                trimmed[1..].trim().to_string(),
            );
        }
        if let Some(sig) = Signifier::from_symbol(&c1.to_string()) {
            return (
                EntryModifier::None,
                sig,
                trimmed[c1.len_utf8()..].trim().to_string(),
            );
        }
    }

    (EntryModifier::None, default_sig, trimmed.to_string())
}

#[allow(dead_code)]
pub fn parse_rapid_input(raw: &str, default_sig: Signifier) -> (Signifier, String) {
    let (_, sig, text) = parse_rapid_input_full(raw, default_sig);
    (sig, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rtl_text_accurately() {
        assert!(is_rtl_text("سلام دنیا"));
        assert!(is_rtl_text("1234 مطالعه کتاب #کار"));
        assert!(is_rtl_text("مرحبا بالعالم"));
        assert!(is_rtl_text("שלום עולם"));
        assert!(!is_rtl_text("Finish studio lighting sketch"));
        assert!(!is_rtl_text("Meeting about سلام at 10:00"));
        assert!(!is_rtl_text("12345 ---"));
    }

    #[test]
    fn slugifies_and_extracts_unicode_tags() {
        assert_eq!(slugify("Studio & Work"), "studio-work");
        assert_eq!(slugify("کار و مطالعه"), "کار-و-مطالعه");

        let entry = Entry::new(1, "09:00", Signifier::Action, "برنامه‌ریزی #کار و #studio");
        let tags = entry.inline_tags();
        assert_eq!(tags, vec!["#کار".to_string(), "#studio".to_string()]);
        assert!(entry.has_tag("#کار"));
        assert!(entry.has_tag("studio"));
    }

    #[test]
    fn toggles_cancelled_and_threads_pages() {
        assert_eq!(Signifier::Action.toggle_cancelled(), Signifier::Cancelled);
        assert_eq!(Signifier::Cancelled.toggle_cancelled(), Signifier::Action);

        let mut doc = Document {
            path: PathBuf::from("Spaces/studio-work.md"),
            rel_path: "Spaces/studio-work.md".into(),
            title: "Studio & Work".into(),
            kind: DocKind::Space,
            date: None,
            page: Some(12),
            threaded_pages: vec![],
            space: Some("Studio & Work".into()),
            tags: vec![],
            extra_frontmatter: vec![],
            entries: vec![],
            prose_lines: vec![],
        };
        doc.add_threaded_page(45);
        doc.add_threaded_page(28);
        doc.add_threaded_page(12); // duplicate ignored
        assert_eq!(doc.all_pages(), vec![12, 28, 45]);
        assert_eq!(doc.format_pages(), "p. 12, 28, 45");
    }
}
