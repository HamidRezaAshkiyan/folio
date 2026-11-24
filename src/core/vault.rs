use crate::core::markdown::{parse_markdown, serialize_markdown};
use crate::core::model::{
    DocKind, Document, Entry, EntryModifier, SearchHit, Signifier, SpaceInfo, VaultEntryRef,
    VaultFileNode, VaultTree, slugify,
};
use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Duration, Local, NaiveDate};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Vault {
    pub root: PathBuf,
}

impl Vault {
    pub fn open_or_init(root: PathBuf, default_spaces: &[String]) -> Result<Self> {
        let vault = Self { root };
        vault.ensure_structure(default_spaces)?;
        Ok(vault)
    }

    pub fn ensure_structure(&self, default_spaces: &[String]) -> Result<()> {
        let daily_dir = self.root.join("Daily Notes");
        let spaces_dir = self.root.join("Spaces");
        let reviews_dir = self.root.join("Reviews");

        fs::create_dir_all(&daily_dir)
            .with_context(|| format!("failed to create {}", daily_dir.display()))?;
        fs::create_dir_all(&spaces_dir)
            .with_context(|| format!("failed to create {}", spaces_dir.display()))?;
        fs::create_dir_all(&reviews_dir)
            .with_context(|| format!("failed to create {}", reviews_dir.display()))?;

        // Initialize Future Log with 6-month structured buckets if missing
        let future_log = self.root.join("Future Log.md");
        if !future_log.exists() {
            let mut fl_prose = vec![
                "# Future Log".to_string(),
                "Scheduled tasks (`<`) and upcoming events across future months.".to_string(),
                String::new(),
            ];
            for ym in Self::upcoming_months(Self::today_date(), 6) {
                fl_prose.push(format!("## {ym}"));
            }
            let doc = Document {
                path: future_log.clone(),
                rel_path: "Future Log.md".to_string(),
                title: "Future Log".to_string(),
                kind: DocKind::FutureLog,
                date: None,
                page: None,
                threaded_pages: vec![],
                space: Some("Future Log".to_string()),
                tags: vec!["bujo".into(), "future-log".into()],
                extra_frontmatter: vec![],
                entries: vec![],
                prose_lines: fl_prose,
            };
            fs::write(&future_log, serialize_markdown(&doc))?;
        }

        // Seed default spaces only if Spaces/ is currently empty
        let has_spaces = fs::read_dir(&spaces_dir)?
            .filter_map(|e| e.ok())
            .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"));

        if !has_spaces {
            for space_name in default_spaces {
                let _ = self.create_space(space_name)?;
            }
        }

        // Maintain Master Vault Index (`Index.md`)
        let _ = self.update_master_index();

        Ok(())
    }

    pub fn today_date() -> NaiveDate {
        Local::now().date_naive()
    }

    pub fn now_hhmm() -> String {
        Local::now().format("%H:%M").to_string()
    }

    pub fn daily_rel_path(date: NaiveDate) -> String {
        format!("Daily Notes/{}.md", date.format("%Y-%m-%d"))
    }

    pub fn open_or_create_daily(&self, date: NaiveDate, default_page: u32) -> Result<Document> {
        let rel = Self::daily_rel_path(date);
        let full = self.root.join(&rel);
        if full.exists() {
            return self.load_document(&rel);
        }

        // Inherit latest notebook page number from most recent daily note if available
        let page = self.latest_notebook_page().unwrap_or(default_page);

        let doc = Document {
            path: full.clone(),
            rel_path: rel.clone(),
            title: format!("{} ({})", date.format("%Y-%m-%d"), date.format("%a")),
            kind: DocKind::Daily,
            date: Some(date),
            page: Some(page),
            threaded_pages: vec![],
            space: None,
            tags: vec!["bujo".into(), "daily".into()],
            extra_frontmatter: vec![],
            entries: vec![],
            prose_lines: vec![],
        };
        self.save_document(&doc)?;
        self.update_monthly_index(date)?;
        Ok(doc)
    }

    pub fn load_document(&self, rel_path: &str) -> Result<Document> {
        let clean_rel = rel_path.replace('\\', "/");
        let full = self.root.join(&clean_rel);
        let raw = fs::read_to_string(&full)
            .with_context(|| format!("failed to read {}", full.display()))?;
        Ok(parse_markdown(&full, &clean_rel, &raw))
    }

    pub fn save_document(&self, doc: &Document) -> Result<()> {
        if let Some(parent) = doc.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serialize_markdown(doc);
        fs::write(&doc.path, content)
            .with_context(|| format!("failed to write {}", doc.path.display()))?;
        if doc.kind == DocKind::Daily
            && let Some(date) = doc.date
        {
            let _ = self.update_monthly_index(date);
        }
        if !doc.rel_path.eq_ignore_ascii_case("Index.md") {
            let _ = self.update_master_index();
        }
        Ok(())
    }

    pub fn create_space(&self, name: &str) -> Result<Document> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("space name cannot be empty"));
        }
        let slug = slugify(trimmed);
        if slug.is_empty() {
            return Err(anyhow!("space name must contain alphanumeric characters"));
        }
        let rel = format!("Spaces/{slug}.md");
        let full = self.root.join(&rel);
        if full.exists() {
            return self.load_document(&rel);
        }
        let doc = Document {
            path: full,
            rel_path: rel,
            title: trimmed.to_string(),
            kind: DocKind::Space,
            date: None,
            page: None,
            threaded_pages: vec![],
            space: Some(trimmed.to_string()),
            tags: vec!["bujo".into(), "space".into(), slug],
            extra_frontmatter: vec![],
            entries: vec![],
            prose_lines: vec![],
        };
        self.save_document(&doc)?;
        Ok(doc)
    }

    pub fn list_spaces(&self, today: NaiveDate) -> Result<Vec<SpaceInfo>> {
        let mut spaces = Vec::new();

        // First space is always Daily Log
        let daily_rel = Self::daily_rel_path(today);
        let daily_count = self
            .load_document(&daily_rel)
            .map(|d| d.entries.len())
            .unwrap_or(0);
        spaces.push(SpaceInfo {
            name: "Daily Log".to_string(),
            slug: "daily-log".to_string(),
            is_daily: true,
            rel_path: daily_rel,
            entry_count: daily_count,
        });

        let spaces_dir = self.root.join("Spaces");
        if spaces_dir.exists() {
            let mut files: Vec<PathBuf> = fs::read_dir(&spaces_dir)?
                .filter_map(|e| e.ok().map(|x| x.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                .collect();
            files.sort();
            for file in files {
                if let Some(fname) = file.file_name().and_then(|s| s.to_str()) {
                    let rel = format!("Spaces/{fname}");
                    if let Ok(doc) = self.load_document(&rel) {
                        let slug = file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("space")
                            .to_string();
                        spaces.push(SpaceInfo {
                            name: doc.space.clone().unwrap_or(doc.title.clone()),
                            slug,
                            is_daily: false,
                            rel_path: rel,
                            entry_count: doc.entries.len(),
                        });
                    }
                }
            }
        }

        Ok(spaces)
    }

    pub fn vault_tree(&self) -> Result<VaultTree> {
        let mut nodes = Vec::new();

        // Daily Notes/ (most recent first)
        nodes.push(VaultFileNode {
            label: "Daily Notes/".to_string(),
            rel_path: "Daily Notes".to_string(),
            is_dir: true,
            depth: 0,
        });
        let mut daily_files = list_md_files(&self.root.join("Daily Notes"))?;
        daily_files.sort();
        daily_files.reverse();
        for name in daily_files {
            nodes.push(VaultFileNode {
                label: name.clone(),
                rel_path: format!("Daily Notes/{name}"),
                is_dir: false,
                depth: 1,
            });
        }

        // Spaces/
        nodes.push(VaultFileNode {
            label: "Spaces/".to_string(),
            rel_path: "Spaces".to_string(),
            is_dir: true,
            depth: 0,
        });
        let mut space_files = list_md_files(&self.root.join("Spaces"))?;
        space_files.sort();
        for name in space_files {
            nodes.push(VaultFileNode {
                label: name.clone(),
                rel_path: format!("Spaces/{name}"),
                is_dir: false,
                depth: 1,
            });
        }

        // Reviews/
        nodes.push(VaultFileNode {
            label: "Reviews/".to_string(),
            rel_path: "Reviews".to_string(),
            is_dir: true,
            depth: 0,
        });
        let mut review_files = list_md_files(&self.root.join("Reviews"))?;
        review_files.sort();
        review_files.reverse();
        for name in review_files {
            nodes.push(VaultFileNode {
                label: name.clone(),
                rel_path: format!("Reviews/{name}"),
                is_dir: false,
                depth: 1,
            });
        }

        // Root Index.md & Future Log.md
        if self.root.join("Index.md").exists() {
            nodes.push(VaultFileNode {
                label: "Index.md".to_string(),
                rel_path: "Index.md".to_string(),
                is_dir: false,
                depth: 0,
            });
        }
        if self.root.join("Future Log.md").exists() {
            nodes.push(VaultFileNode {
                label: "Future Log.md".to_string(),
                rel_path: "Future Log.md".to_string(),
                is_dir: false,
                depth: 0,
            });
        }

        let vault_name = self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("BuJo-Vault")
            .to_string();

        Ok(VaultTree {
            vault_name,
            vault_root: self.display_root(),
            nodes,
        })
    }

    pub fn display_root(&self) -> String {
        if let Some(home) = dirs::home_dir()
            && let Ok(stripped) = self.root.strip_prefix(&home)
        {
            return format!("~/{}", stripped.to_string_lossy().replace('\\', "/"));
        }
        self.root.to_string_lossy().replace('\\', "/")
    }

    pub fn display_doc_path(&self, rel_path: &str) -> String {
        format!("{}/{}", self.display_root(), rel_path.replace('\\', "/"))
    }

    pub fn latest_notebook_page(&self) -> Option<u32> {
        let mut files = list_md_files(&self.root.join("Daily Notes")).ok()?;
        files.sort();
        files.reverse();
        for f in files {
            if f.ends_with("-index.md") {
                continue;
            }
            if let Ok(doc) = self.load_document(&format!("Daily Notes/{f}"))
                && let Some(p) = doc.page
            {
                return Some(p);
            }
        }
        None
    }

    /// Load all daily documents within an inclusive date range `[start, end]`.
    pub fn daily_docs_in_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<Document>> {
        let mut out = Vec::new();
        let mut files = list_md_files(&self.root.join("Daily Notes"))?;
        files.sort();
        for fname in files {
            if fname.ends_with("-index.md") {
                continue;
            }
            let stem = fname.trim_end_matches(".md");
            if let Ok(d) = NaiveDate::parse_from_str(stem, "%Y-%m-%d")
                && d >= start
                && d <= end
                && let Ok(doc) = self.load_document(&format!("Daily Notes/{fname}"))
            {
                out.push(doc);
            }
        }
        Ok(out)
    }

    /// Auto-maintain `Daily Notes/YYYY-MM-index.md` with Obsidian wikilinks & notebook page numbers.
    pub fn update_monthly_index(&self, date: NaiveDate) -> Result<()> {
        let ym = date.format("%Y-%m").to_string();
        let index_rel = format!("Daily Notes/{ym}-index.md");
        let index_full = self.root.join(&index_rel);

        let mut rows: BTreeMap<String, (Option<u32>, usize, usize)> = BTreeMap::new();
        let files = list_md_files(&self.root.join("Daily Notes"))?;
        for f in files {
            if f.ends_with("-index.md") || f.ends_with("-monthly.md") || !f.starts_with(&ym) {
                continue;
            }
            let stem = f.trim_end_matches(".md").to_string();
            if let Ok(doc) = self.load_document(&format!("Daily Notes/{f}")) {
                let open_actions = doc
                    .entries
                    .iter()
                    .filter(|e| e.signifier == Signifier::Action)
                    .count();
                let done_actions = doc
                    .entries
                    .iter()
                    .filter(|e| e.signifier == Signifier::Done)
                    .count();
                rows.insert(stem, (doc.page, open_actions, done_actions));
            }
        }

        let mut prose = vec![
            format!("# {ym} — Monthly Index"),
            String::new(),
            "| Daily Log | Notebook Page | Open (.) | Done (×) |".to_string(),
            "| --- | --- | --- | --- |".to_string(),
        ];
        for (stem, (page, open, done)) in rows {
            let p_str = page.map(|p| format!("p. {p}")).unwrap_or_else(|| "—".into());
            prose.push(format!("| [[{stem}]] | {p_str} | {open} | {done} |"));
        }

        let doc = Document {
            path: index_full,
            rel_path: index_rel,
            title: format!("{ym} Monthly Index"),
            kind: DocKind::Index,
            date: None,
            page: None,
            threaded_pages: vec![],
            space: None,
            tags: vec!["bujo".into(), "index".into()],
            extra_frontmatter: vec![("month".into(), ym)],
            entries: vec![],
            prose_lines: prose,
        };
        let content = serialize_markdown(&doc);
        fs::write(&doc.path, content)?;
        Ok(())
    }

    /// Auto-maintain the Master Vault Index (`Index.md`) with Obsidian wikilinks
    /// and Bullet Journal Threading (physical notebook page numbers across collections & months).
    pub fn update_master_index(&self) -> Result<Document> {
        let index_rel = "Index.md".to_string();
        let index_full = self.root.join(&index_rel);

        let mut prose = vec![
            "# Master Index".to_string(),
            "Auto-maintained Bullet Journal Master Index & Page Threading.".to_string(),
            String::new(),
        ];

        // Future Log
        if self.root.join("Future Log.md").exists() {
            let fl_pages = self
                .load_document("Future Log.md")
                .map(|d| d.format_pages())
                .unwrap_or_else(|_| "—".to_string());
            let fl_count = self
                .load_document("Future Log.md")
                .map(|d| d.entries.len())
                .unwrap_or(0);
            prose.push("## Future Log".to_string());
            prose.push("| Log | Notebook Pages | Entries |".to_string());
            prose.push("| --- | --- | --- |".to_string());
            prose.push(format!("| [[Future Log]] | {fl_pages} | {fl_count} |"));
            prose.push(String::new());
        }

        // Collections / Spaces (with Threading)
        prose.push("## Collections (Spaces & Threading)".to_string());
        prose.push("| Collection | Threaded Pages | Entries |".to_string());
        prose.push("| --- | --- | --- |".to_string());
        let mut space_files = list_md_files(&self.root.join("Spaces"))?;
        space_files.sort();
        for f in space_files {
            let stem = f.trim_end_matches(".md");
            let rel = format!("Spaces/{f}");
            if let Ok(doc) = self.load_document(&rel) {
                let pages = doc.format_pages();
                let count = doc.entries.len();
                prose.push(format!("| [[{stem}\\|{}]] | {pages} | {count} |", doc.title));
            }
        }
        prose.push(String::new());

        // Monthly Logs / Daily Notes grouped by YYYY-MM
        let mut months: BTreeMap<String, (Vec<u32>, usize)> = BTreeMap::new();
        let daily_files = list_md_files(&self.root.join("Daily Notes"))?;
        for f in daily_files {
            if f.ends_with("-index.md") || f.ends_with("-monthly.md") || f.len() < 7 {
                continue;
            }
            let ym = f[0..7].to_string();
            let entry = months.entry(ym).or_insert_with(|| (Vec::new(), 0));
            entry.1 += 1;
            if let Ok(doc) = self.load_document(&format!("Daily Notes/{f}")) {
                for p in doc.all_pages() {
                    if !entry.0.contains(&p) {
                        entry.0.push(p);
                    }
                }
            }
        }
        prose.push("## Monthly Logs".to_string());
        prose.push("| Month | Notebook Page Range | Daily Notes |".to_string());
        prose.push("| --- | --- | --- |".to_string());
        for (ym, (mut pages, day_count)) in months.into_iter().rev() {
            pages.sort_unstable();
            let range_str = match (pages.first(), pages.last()) {
                (Some(a), Some(b)) if a == b => format!("p. {a}"),
                (Some(a), Some(b)) => format!("p. {a}–{b}"),
                _ => "—".to_string(),
            };
            prose.push(format!("| [[{ym}-index\\|{ym}]] | {range_str} | {day_count} |"));
        }
        prose.push(String::new());

        // Reviews
        let mut review_files = list_md_files(&self.root.join("Reviews"))?;
        review_files.sort();
        review_files.reverse();
        if !review_files.is_empty() {
            prose.push("## Reviews".to_string());
            for f in review_files {
                let stem = f.trim_end_matches(".md");
                prose.push(format!("- [[{stem}]]"));
            }
        }

        let doc = Document {
            path: index_full,
            rel_path: index_rel,
            title: "Master Index".to_string(),
            kind: DocKind::Index,
            date: None,
            page: None,
            threaded_pages: vec![],
            space: None,
            tags: vec!["bujo".into(), "master-index".into()],
            extra_frontmatter: vec![],
            entries: vec![],
            prose_lines: prose,
        };
        let content = serialize_markdown(&doc);
        fs::write(&doc.path, content)?;
        Ok(doc)
    }

    /// Migrate an entry from `source_doc` to `target_rel_path` (e.g. tomorrow's daily note, a space, or `Future Log.md`).
    /// Marks source entry as `>` (Migrated) or `<` (Scheduled) and appends an open `.` action to the target.
    pub fn migrate_entry(
        &self,
        source_doc: &mut Document,
        entry_id: usize,
        target_rel_path: &str,
        scheduled: bool,
    ) -> Result<()> {
        let entry_idx = source_doc
            .entries
            .iter()
            .position(|e| e.id == entry_id)
            .ok_or_else(|| anyhow!("entry {entry_id} not found"))?;

        let original_text = source_doc.entries[entry_idx].text.clone();
        let target_stem = Path::new(target_rel_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(target_rel_path);

        // Update source entry signifier
        source_doc.entries[entry_idx].signifier = if scheduled {
            Signifier::Scheduled
        } else {
            Signifier::Migrated
        };
        if !source_doc.entries[entry_idx].text.contains("[[") {
            source_doc.entries[entry_idx].text = format!("{original_text} → [[{target_stem}]]");
        }
        self.save_document(source_doc)?;

        // Load or create target document
        let target_full = self.root.join(target_rel_path);
        let mut target_doc = if target_full.exists() {
            self.load_document(target_rel_path)?
        } else if let Some(stem) = Path::new(target_rel_path)
            .file_stem()
            .and_then(|s| s.to_str())
            && let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d")
        {
            self.open_or_create_daily(date, source_doc.page.unwrap_or(1))?
        } else {
            self.create_space(target_stem)?
        };

        let new_id = target_doc.next_entry_id();
        let source_stem = source_doc
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("source");
        target_doc.entries.push(Entry::new(
            new_id,
            Self::now_hhmm(),
            Signifier::Action,
            format!("{original_text} (from [[{source_stem}]])"),
        ));
        self.save_document(&target_doc)?;
        Ok(())
    }

    /// Return the next `count` months formatted as `YYYY-MM` starting from `anchor`'s month.
    pub fn upcoming_months(anchor: NaiveDate, count: usize) -> Vec<String> {
        let mut out = Vec::with_capacity(count);
        let mut y = anchor.year();
        let mut m = anchor.month();
        for _ in 0..count {
            out.push(format!("{y:04}-{m:02}"));
            if m == 12 {
                y += 1;
                m = 1;
            } else {
                m += 1;
            }
        }
        out
    }

    /// Rename a custom Space file and update its frontmatter + Master Index.
    pub fn rename_space(&self, old_rel_path: &str, new_name: &str) -> Result<Document> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("new space name cannot be empty"));
        }
        if !old_rel_path.starts_with("Spaces/") {
            return Err(anyhow!("only custom Spaces can be renamed"));
        }
        let new_slug = slugify(trimmed);
        if new_slug.is_empty() {
            return Err(anyhow!("space name must contain alphanumeric characters"));
        }
        let mut doc = self.load_document(old_rel_path)?;
        let new_rel = format!("Spaces/{new_slug}.md");
        let old_full = self.root.join(old_rel_path);
        let new_full = self.root.join(&new_rel);

        if old_full != new_full && new_full.exists() {
            return Err(anyhow!("a space named '{trimmed}' already exists"));
        }

        doc.title = trimmed.to_string();
        doc.space = Some(trimmed.to_string());
        doc.rel_path = new_rel.clone();
        doc.path = new_full.clone();
        self.save_document(&doc)?;

        if old_full != new_full && old_full.exists() {
            let _ = fs::remove_file(&old_full);
            let _ = self.update_master_index();
        }
        Ok(doc)
    }

    /// Delete a custom Space file and refresh the Master Index.
    pub fn delete_space(&self, rel_path: &str) -> Result<()> {
        if !rel_path.starts_with("Spaces/") {
            return Err(anyhow!("only custom Spaces can be deleted"));
        }
        let full = self.root.join(rel_path);
        if full.exists() {
            fs::remove_file(&full)
                .with_context(|| format!("failed to delete {}", full.display()))?;
        }
        let _ = self.update_master_index();
        Ok(())
    }

    /// Open or create a classic BuJo Monthly Log (`Daily Notes/YYYY-MM-monthly.md`)
    /// containing a Calendar Page overview (`01 Mon`..`31 Wed`) + Monthly Task List.
    pub fn open_or_create_monthly_log(&self, anchor: NaiveDate) -> Result<Document> {
        let ym = anchor.format("%Y-%m").to_string();
        let rel = format!("Daily Notes/{ym}-monthly.md");
        let full = self.root.join(&rel);
        if full.exists() {
            return self.load_document(&rel);
        }

        let month_start = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).unwrap_or(anchor);
        let next_month = if anchor.month() == 12 {
            NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1).unwrap_or(anchor)
        } else {
            NaiveDate::from_ymd_opt(anchor.year(), anchor.month() + 1, 1).unwrap_or(anchor)
        };
        let month_end = next_month - Duration::days(1);

        let mut prose = vec![
            format!("# {ym} — Monthly Log"),
            String::new(),
            "## Calendar Page".to_string(),
        ];
        let mut day = month_start;
        while day <= month_end {
            let stem = day.format("%Y-%m-%d");
            let label = day.format("%d %a");
            prose.push(format!("- {label} · [[{stem}]]"));
            day += Duration::days(1);
        }
        prose.push(String::new());
        prose.push("## Monthly Task List".to_string());
        prose.push("Rapid-log tasks for this month below; migrate to Daily Notes as you schedule them.".to_string());

        let doc = Document {
            path: full,
            rel_path: rel,
            title: format!("{ym} Monthly Log"),
            kind: DocKind::MonthlyLog,
            date: Some(month_start),
            page: self.latest_notebook_page(),
            threaded_pages: vec![],
            space: None,
            tags: vec!["bujo".into(), "monthly-log".into()],
            extra_frontmatter: vec![("month".into(), ym)],
            entries: vec![],
            prose_lines: prose,
        };
        self.save_document(&doc)?;
        Ok(doc)
    }

    /// Schedule an entry into a specific month bucket inside `Future Log.md` (`[YYYY-MM]`).
    pub fn schedule_to_future_month(
        &self,
        source_doc: &mut Document,
        entry_id: usize,
        month_ym: &str,
    ) -> Result<()> {
        let entry_idx = source_doc
            .entries
            .iter()
            .position(|e| e.id == entry_id)
            .ok_or_else(|| anyhow!("entry {entry_id} not found"))?;

        let original_text = source_doc.entries[entry_idx].text.clone();
        source_doc.entries[entry_idx].signifier = Signifier::Scheduled;
        if !source_doc.entries[entry_idx].text.contains("[[Future Log]]") {
            source_doc.entries[entry_idx].text =
                format!("{original_text} → [[Future Log]] [{month_ym}]");
        }
        self.save_document(source_doc)?;

        let mut fl_doc = self.load_document("Future Log.md")?;
        let heading = format!("## {month_ym}");
        if !fl_doc.prose_lines.iter().any(|l| l.trim() == heading) {
            fl_doc.prose_lines.push(heading);
        }
        let new_id = fl_doc.next_entry_id();
        let source_stem = source_doc
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("source");
        fl_doc.entries.push(Entry::new(
            new_id,
            Self::now_hhmm(),
            Signifier::Action,
            format!("[{month_ym}] {original_text} (from [[{source_stem}]])"),
        ));
        self.save_document(&fl_doc)?;
        Ok(())
    }

    /// Collect all open `.` Actions from Daily Notes strictly before `before_date`
    /// (plus Monthly Logs) for the BuJo Batch Migration Triage ritual.
    pub fn collect_open_actions(&self, before_date: NaiveDate) -> Result<Vec<VaultEntryRef>> {
        let mut out = Vec::new();
        let mut files = list_md_files(&self.root.join("Daily Notes"))?;
        files.sort();
        files.reverse();
        for f in files {
            if f.ends_with("-index.md") {
                continue;
            }
            let rel = format!("Daily Notes/{f}");
            if let Ok(doc) = self.load_document(&rel) {
                let include = match doc.kind {
                    DocKind::Daily => doc.date.map(|d| d < before_date).unwrap_or(false),
                    DocKind::MonthlyLog => true,
                    _ => false,
                };
                if !include {
                    continue;
                }
                for entry in doc.entries {
                    if entry.signifier == Signifier::Action {
                        out.push(VaultEntryRef {
                            rel_path: doc.rel_path.clone(),
                            doc_title: doc.title.clone(),
                            entry,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// Apply a single triage action (`today`, `future`, `done`, `cancel`) to an open action
    /// in `source_rel`.
    pub fn triage_open_action(
        &self,
        source_rel: &str,
        entry_id: usize,
        action: &str,
        today: NaiveDate,
        default_page: u32,
    ) -> Result<()> {
        let mut source_doc = self.load_document(source_rel)?;
        match action {
            "today" => {
                let _ = self.open_or_create_daily(today, default_page)?;
                let target = Self::daily_rel_path(today);
                self.migrate_entry(&mut source_doc, entry_id, &target, false)
            }
            "future" => {
                let next_month = Self::upcoming_months(today, 2)
                    .into_iter()
                    .nth(1)
                    .unwrap_or_else(|| today.format("%Y-%m").to_string());
                self.schedule_to_future_month(&mut source_doc, entry_id, &next_month)
            }
            "done" => {
                if let Some(e) = source_doc.entries.iter_mut().find(|e| e.id == entry_id) {
                    e.signifier = Signifier::Done;
                    self.save_document(&source_doc)?;
                }
                Ok(())
            }
            "cancel" => {
                if let Some(e) = source_doc.entries.iter_mut().find(|e| e.id == entry_id) {
                    e.signifier = Signifier::Cancelled;
                    self.save_document(&source_doc)?;
                }
                Ok(())
            }
            other => Err(anyhow!("unknown triage action '{other}'")),
        }
    }

    /// Collect all `[paper:no]` entries across the entire vault (Daily Notes, Monthly Logs,
    /// Spaces, and Future Log) for the Global Carry-to-Paper Queue.
    pub fn collect_paper_pending(&self) -> Result<Vec<VaultEntryRef>> {
        let mut out = Vec::new();
        let mut rels = Vec::new();

        let mut daily = list_md_files(&self.root.join("Daily Notes"))?;
        daily.sort();
        daily.reverse();
        for f in daily {
            if !f.ends_with("-index.md") {
                rels.push(format!("Daily Notes/{f}"));
            }
        }
        let mut spaces = list_md_files(&self.root.join("Spaces"))?;
        spaces.sort();
        for f in spaces {
            rels.push(format!("Spaces/{f}"));
        }
        if self.root.join("Future Log.md").exists() {
            rels.push("Future Log.md".to_string());
        }

        for rel in rels {
            if let Ok(doc) = self.load_document(&rel) {
                for entry in doc.entries {
                    if !entry.paper_synced {
                        out.push(VaultEntryRef {
                            rel_path: doc.rel_path.clone(),
                            doc_title: doc.title.clone(),
                            entry,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// Toggle or set `paper_synced` on a specific entry in any vault document.
    pub fn mark_entry_paper_synced(
        &self,
        rel_path: &str,
        entry_id: usize,
        synced: bool,
    ) -> Result<()> {
        let mut doc = self.load_document(rel_path)?;
        if let Some(e) = doc.entries.iter_mut().find(|e| e.id == entry_id) {
            e.paper_synced = synced;
            self.save_document(&doc)?;
        }
        Ok(())
    }

    /// Mark every `[paper:no]` entry across the entire vault as `[paper:ok]`.
    pub fn mark_all_vault_paper_synced(&self) -> Result<usize> {
        let pending = self.collect_paper_pending()?;
        let mut by_doc: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for item in &pending {
            by_doc
                .entry(item.rel_path.clone())
                .or_default()
                .push(item.entry.id);
        }
        let mut count = 0usize;
        for (rel, ids) in by_doc {
            if let Ok(mut doc) = self.load_document(&rel) {
                for e in &mut doc.entries {
                    if ids.contains(&e.id) && !e.paper_synced {
                        e.paper_synced = true;
                        count += 1;
                    }
                }
                let _ = self.save_document(&doc);
            }
        }
        Ok(count)
    }

    /// Full-text search across all Markdown files in the vault (Daily Notes, Spaces, Reviews, Future Log).
    pub fn search_vault(&self, query: &str) -> Result<Vec<SearchHit>> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Ok(vec![]);
        }

        let tree = self.vault_tree()?;
        let mut hits = Vec::new();

        for node in tree.nodes {
            if node.is_dir {
                continue;
            }
            let Ok(doc) = self.load_document(&node.rel_path) else {
                continue;
            };
            let mut matched_in_doc = false;

            for entry in &doc.entries {
                if entry.text.to_ascii_lowercase().contains(&needle)
                    || entry.time.contains(&needle)
                {
                    hits.push(SearchHit {
                        rel_path: doc.rel_path.clone(),
                        doc_title: doc.title.clone(),
                        entry_id: Some(entry.id),
                        time: Some(entry.time.clone()),
                        signifier: Some(entry.signifier),
                        modifier: entry.modifier,
                        snippet: entry.text.clone(),
                    });
                    matched_in_doc = true;
                }
            }

            if !matched_in_doc {
                for line in &doc.prose_lines {
                    if line.to_ascii_lowercase().contains(&needle) {
                        hits.push(SearchHit {
                            rel_path: doc.rel_path.clone(),
                            doc_title: doc.title.clone(),
                            entry_id: None,
                            time: None,
                            signifier: None,
                            modifier: EntryModifier::None,
                            snippet: line.trim().to_string(),
                        });
                        break;
                    }
                }
            }

            if hits.len() >= 100 {
                break;
            }
        }

        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn searches_across_daily_and_spaces() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(dir.path().to_path_buf(), &["Studio & Work".into()]).unwrap();

        let d1 = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        let mut daily = vault.open_or_create_daily(d1, 42).unwrap();
        daily.add_entry("09:30".into(), Signifier::Event, "Coffee with Maya #social".into());
        vault.save_document(&daily).unwrap();

        let mut space = vault.load_document("Spaces/studio-work.md").unwrap();
        space.add_entry_with_modifier(
            "14:00".into(),
            EntryModifier::Priority,
            Signifier::Action,
            "Review Maya zine proofs #studio".into(),
        );
        vault.save_document(&space).unwrap();

        let hits = vault.search_vault("maya").unwrap();
        assert_eq!(hits.len(), 2);
        let tag_hits = vault.search_vault("#studio").unwrap();
        assert_eq!(tag_hits.len(), 1);
        assert_eq!(tag_hits[0].modifier, EntryModifier::Priority);
    }

    #[test]
    fn monthly_log_triage_future_buckets_paper_queue_and_manage_spaces() {
        let dir = tempdir().unwrap();
        let vault = Vault::open_or_init(dir.path().to_path_buf(), &["Studio & Work".into()]).unwrap();

        // Manage Spaces: rename & delete
        let renamed = vault.rename_space("Spaces/studio-work.md", "Design Studio").unwrap();
        assert_eq!(renamed.rel_path, "Spaces/design-studio.md");
        assert!(!dir.path().join("Spaces/studio-work.md").exists());
        vault.create_space("Temp Space").unwrap();
        vault.delete_space("Spaces/temp-space.md").unwrap();
        assert!(!dir.path().join("Spaces/temp-space.md").exists());

        // Monthly Log (Calendar Page + Task List)
        let d_past = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let d_today = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        let monthly = vault.open_or_create_monthly_log(d_today).unwrap();
        assert_eq!(monthly.kind, DocKind::MonthlyLog);
        assert!(monthly.prose_lines.iter().any(|l| l.contains("## Calendar Page")));

        // Nested sub-task indentation + Batch Migration Triage
        let mut past_doc = vault.open_or_create_daily(d_past, 10).unwrap();
        let id1 = past_doc.add_entry("09:00".into(), Signifier::Action, "Parent task".into());
        let id2 = past_doc.add_entry("09:05".into(), Signifier::Action, "Child sub-task".into());
        past_doc.entries[1].indent = 1;
        vault.save_document(&past_doc).unwrap();

        let reloaded = vault.load_document(&past_doc.rel_path).unwrap();
        assert_eq!(reloaded.entries[1].indent, 1);

        let open = vault.collect_open_actions(d_today).unwrap();
        assert_eq!(open.len(), 2);

        // Triage id1 -> today, id2 -> future month bucket
        vault.triage_open_action(&past_doc.rel_path, id1, "today", d_today, 11).unwrap();
        let mut past_again = vault.load_document(&past_doc.rel_path).unwrap();
        vault.schedule_to_future_month(&mut past_again, id2, "2026-12").unwrap();
        let fl = vault.load_document("Future Log.md").unwrap();
        assert!(fl.entries.iter().any(|e| e.text.contains("[2026-12]")));

        // Global Carry-to-Paper Queue
        let pending_before = vault.collect_paper_pending().unwrap();
        assert!(!pending_before.is_empty());
        let synced_count = vault.mark_all_vault_paper_synced().unwrap();
        assert_eq!(synced_count, pending_before.len());
        assert!(vault.collect_paper_pending().unwrap().is_empty());
    }
}

fn list_md_files(dir: &Path) -> Result<Vec<String>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md")
            && let Some(name) = path.file_name().and_then(|s| s.to_str())
        {
            out.push(name.to_string());
        }
    }
    Ok(out)
}
