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

    /// Extract inline `#tags` (e.g. `#studio`, `#reading`) from entry text.
    pub fn inline_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        for token in self.text.split_whitespace() {
            if let Some(rest) = token.strip_prefix('#') {
                let clean: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !clean.is_empty() {
                    let tag = format!("#{}", clean.to_ascii_lowercase());
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
            tag.to_ascii_lowercase()
        } else {
            format!("#{}", tag.to_ascii_lowercase())
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

/// Convert a human space name like "Studio & Work" to a clean filename slug "studio-work".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
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
