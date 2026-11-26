use crate::core::config::{FolioConfig, RtlMode, expand_tilde};
use crate::core::io::{ExportFormat, ImportFormat, export_document, import_from_str};
use crate::core::markdown::serialize_markdown;
use crate::core::model::{
    Document, EntryModifier, Signifier, SpaceInfo, VaultTree, parse_rapid_input_full,
};
use crate::core::review::{generate_monthly_review, generate_weekly_review};
use crate::core::updater::{check_for_updates, install_update};
use crate::core::vault::Vault;
use anyhow::{Context, Result};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const INDEX_HTML: &str = include_str!("index.html");

#[derive(Serialize)]
struct StateResponse {
    display_path: String,
    rel_path: String,
    doc: Document,
    inline_tags: Vec<String>,
    paper_pending: usize,
    raw_markdown: String,
    spaces: Vec<SpaceInfo>,
    vault_tree: VaultTree,
}

#[derive(Deserialize)]
struct AddEntryReq {
    path: String,
    #[serde(default)]
    signifier: Option<String>,
    #[serde(default)]
    modifier: Option<String>,
    text: String,
}

#[derive(Deserialize)]
struct UpdateEntryReq {
    path: String,
    id: usize,
    action: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    scheduled: Option<bool>,
}

#[derive(Deserialize)]
struct DeleteEntryReq {
    path: String,
    id: usize,
}

#[derive(Deserialize)]
struct PathOnlyReq {
    path: String,
}

#[derive(Deserialize)]
struct DayNavReq {
    path: String,
    delta: i64,
}

#[derive(Deserialize)]
struct SpaceReq {
    name: String,
}

#[derive(Deserialize)]
struct PageReq {
    path: String,
    page: u32,
}

#[derive(Deserialize)]
struct ReviewReq {
    kind: String,
}

#[derive(Deserialize)]
struct ImportReq {
    path: String,
    #[serde(default)]
    format: Option<String>,
    content: String,
}

#[derive(Serialize)]
struct ConfigResponse {
    vault_path: String,
    config_path: String,
    notebook_page: u32,
    default_spaces: Vec<String>,
    rtl_mode: RtlMode,
    gui_theme: String,
    persian_font: String,
    auto_check_updates: bool,
}

#[derive(Deserialize)]
struct UpdateConfigReq {
    #[serde(default)]
    vault_path: Option<String>,
    #[serde(default)]
    notebook_page: Option<u32>,
    #[serde(default)]
    default_spaces: Option<Vec<String>>,
    #[serde(default)]
    rtl_mode: Option<RtlMode>,
    #[serde(default)]
    gui_theme: Option<String>,
    #[serde(default)]
    persian_font: Option<String>,
    #[serde(default)]
    auto_check_updates: Option<bool>,
}

pub fn run_gui(mut vault: Vault, mut config: FolioConfig, port: u16, open_browser: bool) -> Result<()> {
    let today = Vault::today_date();
    let _ = vault.open_or_create_daily(today, config.notebook_page)?;

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("failed to bind GUI server on {addr}: {e}"))?;

    let url = format!("http://{addr}");
    println!("Folio GUI running at {url}");
    println!("Vault: {}", vault.display_root());
    println!("Press Ctrl+C to stop.");

    if open_browser {
        let _ = open::that(&url);
    }

    for mut req in server.incoming_requests() {
        let resp = handle_request(&mut vault, &mut config, &mut req);
        let _ = req.respond(resp);
    }

    Ok(())
}

fn build_config_response(vault: &Vault, config: &FolioConfig) -> ConfigResponse {
    ConfigResponse {
        vault_path: vault.root.to_string_lossy().to_string(),
        config_path: FolioConfig::config_path().to_string_lossy().to_string(),
        notebook_page: config.notebook_page,
        default_spaces: config.default_spaces.clone(),
        rtl_mode: config.rtl_mode,
        gui_theme: config.gui_theme.clone(),
        persian_font: config.persian_font.clone(),
        auto_check_updates: config.auto_check_updates,
    }
}

fn handle_request(
    vault: &mut Vault,
    config: &mut FolioConfig,
    req: &mut Request,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let default_page = config.notebook_page;
    let url = req.url().to_string();
    let (route, query) = url.split_once('?').unwrap_or((&url, ""));

    match (req.method(), route) {
        (Method::Get, "/") | (Method::Get, "/index.html") => {
            html_response(&INDEX_HTML.replace("{{VERSION}}", env!("CARGO_PKG_VERSION")))
        }
        (Method::Get, "/api/config") => {
            let cfg = build_config_response(vault, config);
            json_response(StatusCode(200), &cfg)
        }
        (Method::Get, "/api/update/check") => match check_for_updates() {
            Ok(info) => json_response(StatusCode(200), &info),
            Err(e) => error_response(502, &e.to_string()),
        },
        (Method::Post, "/api/update/install") => match check_for_updates() {
            Ok(info) => match install_update(&info) {
                Ok(outcome) => json_response(StatusCode(200), &outcome),
                Err(e) => error_response(500, &e.to_string()),
            },
            Err(e) => error_response(502, &e.to_string()),
        },
        (Method::Post, "/api/config") => match read_json::<UpdateConfigReq>(req) {
            Ok(body) => {
                if let Some(page) = body.notebook_page
                    && page > 0
                {
                    config.notebook_page = page;
                }
                if let Some(spaces) = body.default_spaces {
                    let clean: Vec<String> = spaces
                        .into_iter()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !clean.is_empty() {
                        config.default_spaces = clean;
                    }
                }
                if let Some(rtl) = body.rtl_mode {
                    config.rtl_mode = rtl;
                }
                if let Some(theme) = body.gui_theme
                    && (theme == "dark" || theme == "light")
                {
                    config.gui_theme = theme;
                }
                if let Some(font) = body.persian_font
                    && !font.trim().is_empty()
                {
                    config.persian_font = font.trim().to_string();
                }
                if let Some(auto_upd) = body.auto_check_updates {
                    config.auto_check_updates = auto_upd;
                }

                if let Some(raw_vault) = body.vault_path {
                    let trimmed = raw_vault.trim();
                    if trimmed.is_empty() {
                        return error_response(400, "vault_path cannot be empty");
                    }
                    let expanded = expand_tilde(PathBuf::from(trimmed));
                    match Vault::open_or_init(expanded.clone(), &config.default_spaces) {
                        Ok(new_vault) => {
                            config.vault_path = expanded;
                            *vault = new_vault;
                        }
                        Err(e) => return error_response(400, &format!("failed to open vault: {e}")),
                    }
                }

                if let Err(e) = config.save() {
                    return error_response(500, &format!("failed to save config: {e}"));
                }

                match build_state(vault, config.notebook_page, None) {
                    Ok(state) => json_response(
                        StatusCode(200),
                        &serde_json::json!({
                            "config": build_config_response(vault, config),
                            "state": state,
                        }),
                    ),
                    Err(e) => error_response(500, &e.to_string()),
                }
            }
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Get, "/api/state") => {
            let rel = get_query_param(query, "path");
            match build_state(vault, default_page, rel.as_deref()) {
                Ok(state) => json_response(StatusCode(200), &state),
                Err(e) => error_response(400, &e.to_string()),
            }
        }
        (Method::Get, "/api/search") => {
            let q = get_query_param(query, "q").unwrap_or_default();
            match vault.search_vault(&q) {
                Ok(hits) => json_response(StatusCode(200), &serde_json::json!({ "hits": hits })),
                Err(e) => error_response(500, &e.to_string()),
            }
        }
        (Method::Post, "/api/day") => match read_json::<DayNavReq>(req) {
            Ok(body) => {
                let target_date = if body.delta == 0 {
                    Vault::today_date()
                } else {
                    let base = load_or_daily(vault, default_page, &body.path)
                        .ok()
                        .and_then(|d| d.date)
                        .unwrap_or_else(Vault::today_date);
                    base + Duration::days(body.delta)
                };
                match vault.open_or_create_daily(target_date, default_page) {
                    Ok(doc) => state_ok(vault, default_page, &doc.rel_path),
                    Err(e) => error_response(500, &e.to_string()),
                }
            }
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Post, "/api/entries") => match read_json::<AddEntryReq>(req) {
            Ok(body) => match load_or_daily(vault, default_page, &body.path) {
                Ok(mut doc) => {
                    let default_sig = body
                        .signifier
                        .as_deref()
                        .and_then(Signifier::from_symbol)
                        .unwrap_or(Signifier::Action);
                    let req_mod = match body.modifier.as_deref() {
                        Some("*") | Some("priority") => EntryModifier::Priority,
                        Some("!") | Some("inspiration") => EntryModifier::Inspiration,
                        _ => EntryModifier::None,
                    };
                    let (parsed_mod, sig, clean_text) =
                        parse_rapid_input_full(&body.text, default_sig);
                    let effective_mod = if parsed_mod != EntryModifier::None {
                        parsed_mod
                    } else {
                        req_mod
                    };
                    if !clean_text.is_empty() {
                        doc.add_entry_with_modifier(
                            Vault::now_hhmm(),
                            effective_mod,
                            sig,
                            clean_text,
                        );
                        if let Err(e) = vault.save_document(&doc) {
                            return error_response(500, &e.to_string());
                        }
                    }
                    state_ok(vault, default_page, &doc.rel_path)
                }
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Patch, "/api/entries") => match read_json::<UpdateEntryReq>(req) {
            Ok(body) => match load_or_daily(vault, default_page, &body.path) {
                Ok(mut doc) => {
                    let res: Result<()> = match body.action.as_str() {
                        "toggle_done" => {
                            if let Some(e) = doc.entries.iter_mut().find(|e| e.id == body.id) {
                                e.signifier = e.signifier.toggle_done();
                                vault.save_document(&doc)
                            } else {
                                Ok(())
                            }
                        }
                        "toggle_cancelled" => {
                            if let Some(e) = doc.entries.iter_mut().find(|e| e.id == body.id) {
                                e.signifier = e.signifier.toggle_cancelled();
                                vault.save_document(&doc)
                            } else {
                                Ok(())
                            }
                        }
                        "toggle_priority" => {
                            if let Some(e) = doc.entries.iter_mut().find(|e| e.id == body.id) {
                                e.modifier = e.modifier.toggle_priority();
                                vault.save_document(&doc)
                            } else {
                                Ok(())
                            }
                        }
                        "toggle_inspiration" => {
                            if let Some(e) = doc.entries.iter_mut().find(|e| e.id == body.id) {
                                e.modifier = e.modifier.toggle_inspiration();
                                vault.save_document(&doc)
                            } else {
                                Ok(())
                            }
                        }
                        "toggle_paper" => {
                            if let Some(e) = doc.entries.iter_mut().find(|e| e.id == body.id) {
                                e.paper_synced = !e.paper_synced;
                                vault.save_document(&doc)
                            } else {
                                Ok(())
                            }
                        }
                        "edit" => {
                            if let Some(e) = doc.entries.iter_mut().find(|e| e.id == body.id)
                                && let Some(new_text) = body.text
                            {
                                e.text = new_text.trim().to_string();
                                vault.save_document(&doc)
                            } else {
                                Ok(())
                            }
                        }
                        "migrate" => {
                            let tomorrow =
                                doc.date.unwrap_or_else(Vault::today_date) + Duration::days(1);
                            let target = Vault::daily_rel_path(tomorrow);
                            vault.migrate_entry(&mut doc, body.id, &target, false)
                        }
                        "schedule" => {
                            vault.migrate_entry(&mut doc, body.id, "Future Log.md", true)
                        }
                        "migrate_to" => {
                            let target = body.target.unwrap_or_else(|| {
                                let tomorrow =
                                    doc.date.unwrap_or_else(Vault::today_date) + Duration::days(1);
                                Vault::daily_rel_path(tomorrow)
                            });
                            let scheduled = body.scheduled.unwrap_or(target == "Future Log.md");
                            vault.migrate_entry(&mut doc, body.id, &target, scheduled)
                        }
                        other => Err(anyhow::anyhow!("unknown action '{other}'")),
                    };
                    match res {
                        Ok(()) => state_ok(vault, default_page, &doc.rel_path),
                        Err(e) => error_response(400, &e.to_string()),
                    }
                }
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Delete, "/api/entries") => match read_json::<DeleteEntryReq>(req) {
            Ok(body) => match load_or_daily(vault, default_page, &body.path) {
                Ok(mut doc) => {
                    doc.entries.retain(|e| e.id != body.id);
                    if let Err(e) = vault.save_document(&doc) {
                        return error_response(500, &e.to_string());
                    }
                    state_ok(vault, default_page, &doc.rel_path)
                }
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Post, "/api/paper-all") => match read_json::<PathOnlyReq>(req) {
            Ok(body) => match load_or_daily(vault, default_page, &body.path) {
                Ok(mut doc) => {
                    for e in &mut doc.entries {
                        e.paper_synced = true;
                    }
                    if let Err(e) = vault.save_document(&doc) {
                        return error_response(500, &e.to_string());
                    }
                    state_ok(vault, default_page, &doc.rel_path)
                }
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Post, "/api/spaces") => match read_json::<SpaceReq>(req) {
            Ok(body) => match vault.create_space(&body.name) {
                Ok(doc) => state_ok(vault, default_page, &doc.rel_path),
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Post, "/api/page") => match read_json::<PageReq>(req) {
            Ok(body) => match load_or_daily(vault, default_page, &body.path) {
                Ok(mut doc) => {
                    doc.page = Some(body.page);
                    if let Err(e) = vault.save_document(&doc) {
                        return error_response(500, &e.to_string());
                    }
                    state_ok(vault, default_page, &doc.rel_path)
                }
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Post, "/api/review") => match read_json::<ReviewReq>(req) {
            Ok(body) => {
                let today = Vault::today_date();
                let res = match body.kind.as_str() {
                    "monthly" => generate_monthly_review(vault, today),
                    _ => generate_weekly_review(vault, today),
                };
                match res {
                    Ok((doc, _)) => state_ok(vault, default_page, &doc.rel_path),
                    Err(e) => error_response(500, &e.to_string()),
                }
            }
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Post, "/api/import") => match read_json::<ImportReq>(req) {
            Ok(body) => match load_or_daily(vault, default_page, &body.path) {
                Ok(mut doc) => {
                    let fmt = body
                        .format
                        .as_deref()
                        .map(ImportFormat::parse)
                        .unwrap_or(ImportFormat::Auto);
                    match import_from_str(&body.content, fmt, &mut doc) {
                        Ok(_) => {
                            if let Err(e) = vault.save_document(&doc) {
                                return error_response(500, &e.to_string());
                            }
                            state_ok(vault, default_page, &doc.rel_path)
                        }
                        Err(e) => error_response(400, &e.to_string()),
                    }
                }
                Err(e) => error_response(400, &e.to_string()),
            },
            Err(e) => error_response(400, &e.to_string()),
        },
        (Method::Get, "/api/export") => {
            let rel = get_query_param(query, "path").unwrap_or_default();
            let fmt_str = get_query_param(query, "format").unwrap_or_else(|| "md".into());
            match load_or_daily(vault, default_page, &rel).and_then(|doc| {
                let fmt = ExportFormat::parse(&fmt_str)?;
                let body = export_document(&doc, fmt)?;
                Ok((fmt, body))
            }) {
                Ok((fmt, body)) => {
                    let ct = match fmt {
                        ExportFormat::Markdown => "text/markdown; charset=utf-8",
                        ExportFormat::Json => "application/json; charset=utf-8",
                        ExportFormat::Csv => "text/csv; charset=utf-8",
                    };
                    let bytes = body.into_bytes();
                    let len = bytes.len();
                    let mut resp = Response::new(
                        StatusCode(200),
                        vec![],
                        std::io::Cursor::new(bytes),
                        Some(len),
                        None,
                    );
                    if let Ok(h) = Header::from_bytes("Content-Type", ct) {
                        resp.add_header(h);
                    }
                    resp
                }
                Err(e) => error_response(400, &e.to_string()),
            }
        }
        _ => error_response(404, "not found"),
    }
}

fn load_or_daily(vault: &Vault, default_page: u32, rel_path: &str) -> Result<Document> {
    if rel_path.trim().is_empty() {
        vault.open_or_create_daily(Vault::today_date(), default_page)
    } else {
        vault.load_document(rel_path)
    }
}

fn build_state(vault: &Vault, default_page: u32, rel_path: Option<&str>) -> Result<StateResponse> {
    let today = Vault::today_date();
    let doc = match rel_path {
        Some(p) if !p.trim().is_empty() => vault.load_document(p)?,
        _ => vault.open_or_create_daily(today, default_page)?,
    };
    let spaces = vault.list_spaces(today)?;
    let vault_tree = vault.vault_tree()?;
    let raw_markdown = serialize_markdown(&doc);
    let paper_pending = doc.paper_pending_count();
    let inline_tags = doc.collect_inline_tags();
    let display_path = vault.display_doc_path(&doc.rel_path);
    let rel = doc.rel_path.clone();

    Ok(StateResponse {
        display_path,
        rel_path: rel,
        doc,
        inline_tags,
        paper_pending,
        raw_markdown,
        spaces,
        vault_tree,
    })
}

fn state_ok(
    vault: &Vault,
    default_page: u32,
    rel_path: &str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    match build_state(vault, default_page, Some(rel_path)) {
        Ok(st) => json_response(StatusCode(200), &st),
        Err(e) => error_response(500, &e.to_string()),
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(req: &mut Request) -> Result<T> {
    let mut buf = String::new();
    req.as_reader()
        .read_to_string(&mut buf)
        .context("failed to read request body")?;
    serde_json::from_str(&buf).context("invalid JSON body")
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let bytes = body.as_bytes().to_vec();
    let len = bytes.len();
    let mut resp = Response::new(
        StatusCode(200),
        vec![],
        std::io::Cursor::new(bytes),
        Some(len),
        None,
    );
    if let Ok(h) = Header::from_bytes("Content-Type", "text/html; charset=utf-8") {
        resp.add_header(h);
    }
    resp
}

fn json_response<T: Serialize>(
    status: StatusCode,
    data: &T,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let bytes = serde_json::to_vec(data).unwrap_or_else(|_| b"{}".to_vec());
    let len = bytes.len();
    let mut resp = Response::new(
        status,
        vec![],
        std::io::Cursor::new(bytes),
        Some(len),
        None,
    );
    if let Ok(h) = Header::from_bytes("Content-Type", "application/json; charset=utf-8") {
        resp.add_header(h);
    }
    resp
}

fn error_response(code: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(StatusCode(code), &serde_json::json!({ "error": msg }))
}

fn get_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == key
        {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(val);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
