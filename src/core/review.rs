use crate::core::model::{DocKind, Document, Entry, Signifier};
use crate::core::vault::Vault;
use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReviewSummary {
    pub rel_path: String,
    pub period_label: String,
    pub days_logged: usize,
    pub total_entries: usize,
    pub actions_open: usize,
    pub actions_done: usize,
    pub actions_migrated: usize,
    pub events_count: usize,
    pub emotions_count: usize,
    pub notes_count: usize,
    pub paper_pending: usize,
}

/// Generate (or refresh) a Weekly Review in `Reviews/YYYY-Www.md` for the ISO week containing `anchor`.
pub fn generate_weekly_review(vault: &Vault, anchor: NaiveDate) -> Result<(Document, ReviewSummary)> {
    let iso_week = anchor.iso_week();
    let weekday_offset = anchor.weekday().num_days_from_monday() as i64;
    let week_start = anchor - Duration::days(weekday_offset);
    let week_end = week_start + Duration::days(6);

    let week_tag = format!("{}-W{:02}", iso_week.year(), iso_week.week());
    let rel_path = format!("Reviews/{week_tag}.md");
    let title = format!("Weekly Review — {week_tag}");
    let range_label = format!(
        "{} to {}",
        week_start.format("%Y-%m-%d"),
        week_end.format("%Y-%m-%d")
    );

    build_review_doc(
        vault,
        week_start,
        week_end,
        &rel_path,
        &title,
        &range_label,
        "weekly-review",
    )
}

/// Generate (or refresh) a Monthly Review in `Reviews/YYYY-MM-review.md` for the month of `anchor`.
pub fn generate_monthly_review(
    vault: &Vault,
    anchor: NaiveDate,
) -> Result<(Document, ReviewSummary)> {
    let month_start = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).unwrap_or(anchor);
    let next_month = if anchor.month() == 12 {
        NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1).unwrap_or(anchor)
    } else {
        NaiveDate::from_ymd_opt(anchor.year(), anchor.month() + 1, 1).unwrap_or(anchor)
    };
    let month_end = next_month - Duration::days(1);

    let ym = anchor.format("%Y-%m").to_string();
    let rel_path = format!("Reviews/{ym}-review.md");
    let title = format!("Monthly Review — {ym}");
    let range_label = format!(
        "{} to {}",
        month_start.format("%Y-%m-%d"),
        month_end.format("%Y-%m-%d")
    );

    build_review_doc(
        vault,
        month_start,
        month_end,
        &rel_path,
        &title,
        &range_label,
        "monthly-review",
    )
}

fn build_review_doc(
    vault: &Vault,
    start: NaiveDate,
    end: NaiveDate,
    rel_path: &str,
    title: &str,
    range_label: &str,
    tag: &str,
) -> Result<(Document, ReviewSummary)> {
    let docs = vault.daily_docs_in_range(start, end)?;

    let mut open_actions: Vec<(String, Entry)> = Vec::new();
    let mut done_actions: Vec<(String, Entry)> = Vec::new();
    let mut migrated_count = 0usize;
    let mut events: Vec<(String, Entry)> = Vec::new();
    let mut emotions: Vec<(String, Entry)> = Vec::new();
    let mut notes: Vec<(String, Entry)> = Vec::new();
    let mut total_entries = 0usize;
    let mut paper_pending = 0usize;

    for doc in &docs {
        let day_stem = doc
            .date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| doc.title.clone());
        for entry in &doc.entries {
            total_entries += 1;
            if !entry.paper_synced {
                paper_pending += 1;
            }
            match entry.signifier {
                Signifier::Action => open_actions.push((day_stem.clone(), entry.clone())),
                Signifier::Done => done_actions.push((day_stem.clone(), entry.clone())),
                Signifier::Migrated | Signifier::Scheduled => migrated_count += 1,
                Signifier::Event => events.push((day_stem.clone(), entry.clone())),
                Signifier::Emotion => emotions.push((day_stem.clone(), entry.clone())),
                Signifier::Note => notes.push((day_stem.clone(), entry.clone())),
                Signifier::Cancelled => {}
            }
        }
    }

    let summary = ReviewSummary {
        rel_path: rel_path.to_string(),
        period_label: range_label.to_string(),
        days_logged: docs.len(),
        total_entries,
        actions_open: open_actions.len(),
        actions_done: done_actions.len(),
        actions_migrated: migrated_count,
        events_count: events.len(),
        emotions_count: emotions.len(),
        notes_count: notes.len(),
        paper_pending,
    };

    // Build structured review entries (open actions surfaced as actionable rapid-log items)
    let mut review_entries = Vec::new();
    for (idx, (day, e)) in open_actions.iter().enumerate() {
        let mut copy = Entry::new(
            idx + 1,
            e.time.clone(),
            Signifier::Action,
            format!("{} (from [[{}]])", e.text, day),
        );
        copy.paper_synced = e.paper_synced;
        review_entries.push(copy);
    }

    let mut prose = vec![
        format!("# {title}"),
        format!("> Period: **{range_label}** · Days logged: **{}**", docs.len()),
        String::new(),
        "## Summary".to_string(),
        format!("- Actions completed (`×`): **{}**", done_actions.len()),
        format!("- Open actions to migrate (`.`): **{}**", open_actions.len()),
        format!("- Migrated / Scheduled (`>` / `<`): **{}**", migrated_count),
        format!("- Events (`o`): **{}**", events.len()),
        format!("- Emotions (`=`): **{}**", emotions.len()),
        format!("- Notes (`-`): **{}**", notes.len()),
        format!("- Pending carry-to-paper: **{}**", paper_pending),
        String::new(),
    ];

    if !events.is_empty() {
        prose.push("## Events Timeline (`o`)".to_string());
        for (day, e) in &events {
            prose.push(format!("- [[{day}]] [{}] {}", e.time, e.text));
        }
        prose.push(String::new());
    }

    if !emotions.is_empty() {
        prose.push("## Emotional Pulse (`=`)".to_string());
        for (day, e) in &emotions {
            prose.push(format!("- [[{day}]] [{}] {}", e.time, e.text));
        }
        prose.push(String::new());
    }

    if !notes.is_empty() {
        prose.push("## Key Notes (`-`)".to_string());
        for (day, e) in &notes {
            prose.push(format!("- [[{day}]] {}", e.text));
        }
        prose.push(String::new());
    }

    prose.push("## Reflection".to_string());
    prose.push("- What worked well?".to_string());
    prose.push("- What should be migrated or dropped?".to_string());
    prose.push("- Focus for the next period:".to_string());

    let doc = Document {
        path: vault.root.join(rel_path),
        rel_path: rel_path.to_string(),
        title: title.to_string(),
        kind: DocKind::Review,
        date: Some(end),
        page: None,
        space: None,
        tags: vec!["bujo".into(), tag.into()],
        extra_frontmatter: vec![
            ("period".into(), range_label.to_string()),
            ("days_logged".into(), docs.len().to_string()),
        ],
        entries: review_entries,
        prose_lines: prose,
    };

    vault.save_document(&doc)?;
    Ok((doc, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn generates_weekly_and_monthly_reviews() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(dir.path().to_path_buf(), &[]).unwrap();

        let d1 = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let mut doc1 = vault.open_or_create_daily(d1, 41).unwrap();
        doc1.add_entry("09:00".into(), Signifier::Action, "Finish sketch".into());
        doc1.add_entry("10:00".into(), Signifier::Done, "Buy ink".into());
        doc1.add_entry("12:00".into(), Signifier::Event, "Lunch with Maya".into());
        doc1.add_entry("18:00".into(), Signifier::Emotion, "Focused and calm".into());
        vault.save_document(&doc1).unwrap();

        let d2 = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let mut doc2 = vault.open_or_create_daily(d2, 42).unwrap();
        doc2.add_entry("08:30".into(), Signifier::Note, "Alexander quote".into());
        vault.save_document(&doc2).unwrap();

        let (weekly_doc, weekly_sum) = generate_weekly_review(&vault, d2).unwrap();
        assert_eq!(weekly_sum.days_logged, 2);
        assert_eq!(weekly_sum.actions_open, 1);
        assert_eq!(weekly_sum.actions_done, 1);
        assert_eq!(weekly_sum.events_count, 1);
        assert_eq!(weekly_sum.emotions_count, 1);
        assert_eq!(weekly_sum.notes_count, 1);
        assert!(weekly_doc.path.exists());

        let (monthly_doc, monthly_sum) = generate_monthly_review(&vault, d2).unwrap();
        assert_eq!(monthly_sum.total_entries, 5);
        assert!(monthly_doc.path.exists());
    }
}
