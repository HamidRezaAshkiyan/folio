use crate::core::markdown::{parse_entry_line, serialize_markdown};
use crate::core::model::{Document, Entry, Signifier};
use crate::core::vault::Vault;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Auto,
    Markdown,
    Json,
    Csv,
    Text,
}

impl ImportFormat {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "md" | "markdown" | "obsidian" | "notion" | "bear" => ImportFormat::Markdown,
            "json" => ImportFormat::Json,
            "csv" => ImportFormat::Csv,
            "txt" | "text" | "apple-notes" | "notes" => ImportFormat::Text,
            _ => ImportFormat::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    Csv,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "md" | "markdown" | "obsidian" => Ok(ExportFormat::Markdown),
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            other => Err(anyhow!(
                "unsupported export format '{other}' (use md, json, or csv)"
            )),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct JsonImportItem {
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    signifier: Option<String>,
    #[serde(alias = "content", alias = "title", alias = "task")]
    text: String,
    #[serde(default)]
    paper_synced: bool,
}

/// Import notes/tasks from a file on disk into `target_doc`, returning number of entries imported.
pub fn import_from_file(
    path: &Path,
    format: ImportFormat,
    target_doc: &mut Document,
) -> Result<usize> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read import file {}", path.display()))?;
    let effective = match format {
        ImportFormat::Auto => match path.extension().and_then(|s| s.to_str()) {
            Some("json") => ImportFormat::Json,
            Some("csv") => ImportFormat::Csv,
            Some("md") | Some("markdown") => ImportFormat::Markdown,
            _ => ImportFormat::Text,
        },
        other => other,
    };
    import_from_str(&raw, effective, target_doc)
}

/// Import notes/tasks from raw text/markdown/JSON/CSV into `target_doc`.
pub fn import_from_str(
    raw: &str,
    format: ImportFormat,
    target_doc: &mut Document,
) -> Result<usize> {
    let fmt = if format == ImportFormat::Auto {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') || trimmed.starts_with('{') {
            ImportFormat::Json
        } else {
            ImportFormat::Markdown
        }
    } else {
        format
    };

    let default_time = Vault::now_hhmm();
    let parsed: Vec<Entry> = match fmt {
        ImportFormat::Json => parse_json_import(raw, &default_time)?,
        ImportFormat::Csv => parse_csv_import(raw, &default_time),
        ImportFormat::Markdown | ImportFormat::Text | ImportFormat::Auto => {
            parse_markdown_or_text_import(raw, &default_time)
        }
    };

    let count = parsed.len();
    for mut entry in parsed {
        entry.id = target_doc.next_entry_id();
        target_doc.entries.push(entry);
    }
    Ok(count)
}

fn parse_json_import(raw: &str, default_time: &str) -> Result<Vec<Entry>> {
    let items: Vec<JsonImportItem> = if raw.trim_start().starts_with('[') {
        serde_json::from_str(raw).context("failed to parse JSON array")?
    } else {
        // Support `{ "entries": [...] }` wrapper too
        let val: serde_json::Value = serde_json::from_str(raw).context("invalid JSON")?;
        if let Some(arr) = val.get("entries") {
            serde_json::from_value(arr.clone()).context("invalid JSON entries array")?
        } else {
            vec![serde_json::from_value(val).context("invalid JSON entry object")?]
        }
    };

    let mut out = Vec::new();
    for item in items {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        let sig = item
            .signifier
            .as_deref()
            .and_then(|s| {
                Signifier::from_symbol(s).or_else(|| match s.to_ascii_lowercase().as_str() {
                    "action" | "task" | "todo" => Some(Signifier::Action),
                    "done" | "completed" => Some(Signifier::Done),
                    "event" => Some(Signifier::Event),
                    "emotion" | "mood" | "feeling" => Some(Signifier::Emotion),
                    "note" => Some(Signifier::Note),
                    _ => None,
                })
            })
            .unwrap_or(Signifier::Note);
        let mut entry = Entry::new(
            0,
            item.time.unwrap_or_else(|| default_time.to_string()),
            sig,
            text.to_string(),
        );
        entry.paper_synced = item.paper_synced;
        out.push(entry);
    }
    Ok(out)
}

fn parse_csv_import(raw: &str, default_time: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = split_csv_line(trimmed);
        // Skip header row if it looks like column names
        if idx == 0
            && cols
                .iter()
                .any(|c| matches!(c.to_ascii_lowercase().as_str(), "text" | "title" | "task" | "name" | "content"))
        {
            continue;
        }
        match cols.as_slice() {
            [text] => {
                if !text.is_empty() {
                    out.push(Entry::new(0, default_time, Signifier::Note, *text));
                }
            }
            [sig_str, text] => {
                let sig = Signifier::from_symbol(sig_str).unwrap_or(Signifier::Action);
                if !text.is_empty() {
                    out.push(Entry::new(0, default_time, sig, *text));
                }
            }
            [time, sig_str, text, ..] => {
                let sig = Signifier::from_symbol(sig_str).unwrap_or(Signifier::Note);
                let t = if time.is_empty() { default_time } else { time };
                if !text.is_empty() {
                    out.push(Entry::new(0, t, sig, *text));
                }
            }
            _ => {}
        }
    }
    out
}

fn split_csv_line(line: &str) -> Vec<&str> {
    line.split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect()
}

fn parse_markdown_or_text_import(raw: &str, default_time: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let normalized = raw.replace("\r\n", "\n");
    let mut lines = normalized.lines().peekable();

    // Skip YAML frontmatter if present
    if lines.peek().map(|l| l.trim()) == Some("---") {
        lines.next();
        for l in lines.by_ref() {
            if l.trim() == "---" {
                break;
            }
        }
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
            continue;
        }
        // Native Folio or Obsidian checklist line
        if let Some(mut entry) = parse_entry_line(trimmed, 0) {
            if entry.time == "--:--" {
                entry.time = default_time.to_string();
            }
            out.push(entry);
            continue;
        }
        // Bullet list (`* item` or `• item`) or plain text line from Apple Notes / Notion
        let clean = trimmed
            .trim_start_matches("* ")
            .trim_start_matches("• ")
            .trim();
        if !clean.is_empty() {
            out.push(Entry::new(0, default_time, Signifier::Note, clean));
        }
    }

    out
}

/// Export a document to Markdown, JSON, or CSV string.
pub fn export_document(doc: &Document, format: ExportFormat) -> Result<String> {
    match format {
        ExportFormat::Markdown => Ok(serialize_markdown(doc)),
        ExportFormat::Json => Ok(serde_json::to_string_pretty(doc)?),
        ExportFormat::Csv => {
            let mut out = String::from("time,signifier,text,paper_synced\n");
            for e in &doc.entries {
                let escaped = e.text.replace('"', "\"\"");
                out.push_str(&format!(
                    "{},{},\"{}\",{}\n",
                    e.time,
                    e.signifier.symbol(),
                    escaped,
                    e.paper_synced
                ));
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::DocKind;
    use std::path::PathBuf;

    fn empty_doc() -> Document {
        Document {
            path: PathBuf::from("Daily Notes/2026-11-21.md"),
            rel_path: "Daily Notes/2026-11-21.md".into(),
            title: "2026-11-21".into(),
            kind: DocKind::Daily,
            date: None,
            page: Some(42),
            threaded_pages: vec![],
            space: None,
            tags: vec![],
            extra_frontmatter: vec![],
            entries: vec![],
            prose_lines: vec![],
        }
    }

    #[test]
    fn imports_obsidian_and_apple_notes() {
        let mut doc = empty_doc();
        let raw = "\
# Imported Notes
- [ ] Prepare slides
- [x] Send email
o Team standup
= Energized after walk
Plain line from Apple Notes
";
        let count = import_from_str(raw, ImportFormat::Markdown, &mut doc).unwrap();
        assert_eq!(count, 5);
        assert_eq!(doc.entries[0].signifier, Signifier::Action);
        assert_eq!(doc.entries[1].signifier, Signifier::Done);
        assert_eq!(doc.entries[2].signifier, Signifier::Event);
        assert_eq!(doc.entries[3].signifier, Signifier::Emotion);
        assert_eq!(doc.entries[4].signifier, Signifier::Note);
    }

    #[test]
    fn imports_json_and_csv_and_exports() {
        let mut doc = empty_doc();
        let json = r#"[{"signifier":".","text":"Draft zine"},{"signifier":"=","text":"Grateful"}]"#;
        assert_eq!(import_from_str(json, ImportFormat::Json, &mut doc).unwrap(), 2);

        let csv = "time,signifier,text\n09:00,o,Gallery opening\n";
        assert_eq!(import_from_str(csv, ImportFormat::Csv, &mut doc).unwrap(), 1);
        assert_eq!(doc.entries.len(), 3);

        let csv_out = export_document(&doc, ExportFormat::Csv).unwrap();
        assert!(csv_out.contains("Gallery opening"));
        let json_out = export_document(&doc, ExportFormat::Json).unwrap();
        assert!(json_out.contains("Draft zine"));
    }
}
