use crate::core::model::{DocKind, Document, Entry, Signifier};
use chrono::NaiveDate;
use std::path::Path;

/// Parse a Folio / Obsidian Markdown file into a structured `Document`.
pub fn parse_markdown(path: &Path, rel_path: &str, raw: &str) -> Document {
    let mut date: Option<NaiveDate> = None;
    let mut page: Option<u32> = None;
    let mut space: Option<String> = None;
    let mut title_fm: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut extra_frontmatter: Vec<(String, String)> = Vec::new();

    let normalized = raw.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut idx = 0usize;

    // Parse optional YAML frontmatter
    if lines.first().map(|l| l.trim()) == Some("---") {
        idx = 1;
        let mut in_tags_list = false;
        while idx < lines.len() {
            let line = lines[idx];
            if line.trim() == "---" {
                idx += 1;
                break;
            }
            if in_tags_list {
                if let Some(item) = line.trim().strip_prefix("- ") {
                    let t = item.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !t.is_empty() {
                        tags.push(t);
                    }
                    idx += 1;
                    continue;
                } else {
                    in_tags_list = false;
                }
            }
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let val = v.trim().trim_matches('"').trim_matches('\'');
                match key {
                    "date" => {
                        date = NaiveDate::parse_from_str(val, "%Y-%m-%d").ok();
                    }
                    "page" => {
                        page = val.parse::<u32>().ok();
                    }
                    "space" => {
                        if !val.is_empty() {
                            space = Some(val.to_string());
                        }
                    }
                    "title" => {
                        if !val.is_empty() {
                            title_fm = Some(val.to_string());
                        }
                    }
                    "tags" => {
                        if val.is_empty() {
                            in_tags_list = true;
                        } else {
                            let stripped = val.trim_start_matches('[').trim_end_matches(']');
                            for t in stripped.split(',') {
                                let t = t.trim().trim_matches('"').trim_matches('\'');
                                if !t.is_empty() {
                                    tags.push(t.to_string());
                                }
                            }
                        }
                    }
                    _ => {
                        extra_frontmatter.push((key.to_string(), val.to_string()));
                    }
                }
            }
            idx += 1;
        }
    }

    let mut entries = Vec::new();
    let mut prose_lines = Vec::new();
    let mut next_id = 1usize;

    for line in &lines[idx..] {
        if let Some(entry) = parse_entry_line(line, next_id) {
            entries.push(entry);
            next_id += 1;
        } else {
            prose_lines.push((*line).to_string());
        }
    }

    // Trim leading/trailing empty prose lines
    while prose_lines.first().is_some_and(|l| l.trim().is_empty()) {
        prose_lines.remove(0);
    }
    while prose_lines.last().is_some_and(|l| l.trim().is_empty()) {
        prose_lines.pop();
    }

    let kind = infer_doc_kind(rel_path);
    if date.is_none() && kind == DocKind::Daily {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            date = NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok();
        }
    }

    let title = title_fm
        .or_else(|| space.clone())
        .or_else(|| {
            date.map(|d| format!("{} ({})", d.format("%Y-%m-%d"), d.format("%a")))
        })
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        });

    Document {
        path: path.to_path_buf(),
        rel_path: rel_path.to_string(),
        title,
        kind,
        date,
        page,
        space,
        tags,
        extra_frontmatter,
        entries,
        prose_lines,
    }
}

pub fn infer_doc_kind(rel_path: &str) -> DocKind {
    let norm = rel_path.replace('\\', "/");
    if norm.ends_with("-index.md") {
        DocKind::Index
    } else if norm.starts_with("Daily Notes/") {
        DocKind::Daily
    } else if norm.starts_with("Spaces/") {
        DocKind::Space
    } else if norm.starts_with("Reviews/") {
        DocKind::Review
    } else if norm.eq_ignore_ascii_case("Future Log.md") {
        DocKind::FutureLog
    } else {
        DocKind::Other
    }
}

/// Parse a single rapid-log line.
/// Supports:
/// - Folio native: `. [08:15] Task text [paper:no]`
/// - Folio without time: `. Task text [paper:ok]`
/// - Obsidian checklist: `- [ ] Task text` / `- [x] Task text`
pub fn parse_entry_line(line: &str, id: usize) -> Option<Entry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
        return None;
    }

    // Obsidian checklist compatibility: `- [ ] ...` or `- [x] ...`
    if let Some(rest) = trimmed
        .strip_prefix("- [ ] ")
        .or_else(|| trimmed.strip_prefix("* [ ] "))
    {
        let (text, paper_synced) = extract_paper_suffix(rest);
        let (time, clean_text) = extract_leading_time(text);
        return Some(Entry {
            id,
            time,
            signifier: Signifier::Action,
            text: clean_text,
            paper_synced,
        });
    }
    if let Some(rest) = trimmed
        .strip_prefix("- [x] ")
        .or_else(|| trimmed.strip_prefix("- [X] "))
        .or_else(|| trimmed.strip_prefix("* [x] "))
        .or_else(|| trimmed.strip_prefix("* [X] "))
    {
        let (text, paper_synced) = extract_paper_suffix(rest);
        let (time, clean_text) = extract_leading_time(text);
        return Some(Entry {
            id,
            time,
            signifier: Signifier::Done,
            text: clean_text,
            paper_synced,
        });
    }

    // Folio rapid-log format: `<sig> [HH:MM] <text> [paper:ok|no]`
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    let second = chars.next()?;
    if !second.is_whitespace() {
        return None;
    }

    let sig = Signifier::from_symbol(&first.to_string())?;
    let rest = trimmed[first.len_utf8()..].trim();
    if rest.is_empty() {
        return None;
    }

    let (without_paper, paper_synced) = extract_paper_suffix(rest);
    let (time, text) = extract_leading_time(without_paper);
    if text.is_empty() {
        return None;
    }

    Some(Entry {
        id,
        time,
        signifier: sig,
        text,
        paper_synced,
    })
}

fn extract_paper_suffix(s: &str) -> (&str, bool) {
    let t = s.trim();
    if let Some(stripped) = t.strip_suffix("[paper:ok]") {
        (stripped.trim_end(), true)
    } else if let Some(stripped) = t.strip_suffix("[paper:no]") {
        (stripped.trim_end(), false)
    } else {
        (t, false)
    }
}

fn extract_leading_time(s: &str) -> (String, String) {
    let t = s.trim();
    if t.starts_with('[') && t.len() >= 7 && t.as_bytes().get(6) == Some(&b']') {
        let candidate = &t[1..6];
        if candidate.len() == 5
            && candidate.as_bytes()[2] == b':'
            && candidate[0..2].chars().all(|c| c.is_ascii_digit())
            && candidate[3..5].chars().all(|c| c.is_ascii_digit())
        {
            return (candidate.to_string(), t[7..].trim().to_string());
        }
    }
    ("--:--".to_string(), t.to_string())
}

/// Serialize a `Document` back to clean Obsidian-compatible Markdown.
pub fn serialize_markdown(doc: &Document) -> String {
    let mut out = String::new();
    let has_fm = doc.date.is_some()
        || doc.page.is_some()
        || doc.space.is_some()
        || !doc.tags.is_empty()
        || !doc.extra_frontmatter.is_empty();

    if has_fm {
        out.push_str("---\n");
        if let Some(d) = doc.date {
            out.push_str(&format!("date: {}\n", d.format("%Y-%m-%d")));
        }
        if let Some(p) = doc.page {
            out.push_str(&format!("page: {p}\n"));
        }
        if let Some(s) = &doc.space {
            out.push_str(&format!("space: {s}\n"));
        }
        if !doc.tags.is_empty() {
            out.push_str("tags:\n");
            for tag in &doc.tags {
                out.push_str(&format!("  - {tag}\n"));
            }
        }
        for (k, v) in &doc.extra_frontmatter {
            out.push_str(&format!("{k}: {v}\n"));
        }
        out.push_str("---\n");
    }

    for entry in &doc.entries {
        let paper = if entry.paper_synced {
            "[paper:ok]"
        } else {
            "[paper:no]"
        };
        if entry.time != "--:--" && !entry.time.is_empty() {
            out.push_str(&format!(
                "{} [{}] {} {}\n",
                entry.signifier.symbol(),
                entry.time,
                entry.text,
                paper
            ));
        } else {
            out.push_str(&format!(
                "{} {} {}\n",
                entry.signifier.symbol(),
                entry.text,
                paper
            ));
        }
    }

    if !doc.prose_lines.is_empty() {
        if !doc.entries.is_empty() {
            out.push('\n');
        }
        for line in &doc.prose_lines {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn roundtrip_daily_markdown() {
        let sample = "\
---
date: 2026-11-21
page: 42
tags:
  - bujo
  - daily
---
. [08:15] Finalize studio lighting sketch before Monday critique [paper:no]
o [09:30] Coffee with Maya at Blue Bottle — discussed zine layout [paper:ok]
= [10:45] Calm and unhurried after walking without headphones [paper:no]
- [11:10] Christopher Alexander: living spaces come from small repairs [paper:no]
× [11:25] Order fountain pen ink refill [paper:ok]
";
        let doc = parse_markdown(
            &PathBuf::from("Daily Notes/2026-11-21.md"),
            "Daily Notes/2026-11-21.md",
            sample,
        );
        assert_eq!(doc.date, NaiveDate::from_ymd_opt(2026, 11, 21));
        assert_eq!(doc.page, Some(42));
        assert_eq!(doc.entries.len(), 5);
        assert_eq!(doc.entries[0].signifier, Signifier::Action);
        assert!(!doc.entries[0].paper_synced);
        assert_eq!(doc.entries[1].signifier, Signifier::Event);
        assert!(doc.entries[1].paper_synced);
        assert_eq!(doc.entries[2].signifier, Signifier::Emotion);
        assert_eq!(doc.entries[3].signifier, Signifier::Note);
        assert_eq!(doc.entries[4].signifier, Signifier::Done);
        assert_eq!(doc.paper_pending_count(), 3);

        let serialized = serialize_markdown(&doc);
        assert_eq!(serialized, sample);
    }

    #[test]
    fn parses_obsidian_checklists() {
        let sample = "- [ ] Ship release\n- [x] Write docs\n";
        let doc = parse_markdown(&PathBuf::from("Spaces/work.md"), "Spaces/work.md", sample);
        assert_eq!(doc.entries.len(), 2);
        assert_eq!(doc.entries[0].signifier, Signifier::Action);
        assert_eq!(doc.entries[0].text, "Ship release");
        assert_eq!(doc.entries[1].signifier, Signifier::Done);
        assert_eq!(doc.entries[1].text, "Write docs");
    }
}
