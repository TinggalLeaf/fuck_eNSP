//! Tails hook JSONL logs, decodes payloads (utf8/gbk/hex), classifies error
//! severity, and streams batches to the frontend via Tauri events.

use crate::native;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::Emitter;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

pub static STREAMING: AtomicBool = AtomicBool::new(false);
pub static LAST_CLEAR_TS: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    pub ts: i64,
    pub pid: u32,
    pub proc: String,
    pub cat: String,
    pub api: String,
    #[serde(default)]
    pub peer: String,
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub len: usize,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub child_pid: Option<u32>,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub cmd: String,
    #[serde(default)]
    pub enc: String,
    #[serde(default)]
    pub data: String,
    // enriched fields (not present in the raw hook JSON; filled by enrich)
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub err_code: Option<i64>,
}

fn try_decode(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (cow, _, _) = encoding_rs::GBK.decode(raw);
            cow.into_owned()
        }
    }
}

fn looks_binary(raw: &[u8]) -> bool {
    if raw.is_empty() {
        return false;
    }
    let bad = raw
        .iter()
        .filter(|b| !(matches!(**b, 0x09 | 0x0a | 0x0d | 0x20..=0x7e) || **b >= 0x80))
        .count();
    bad * 10 > raw.len()
}

fn extract_err_code(text: &str) -> Option<i64> {
    let lower = text.to_lowercase();
    for pat in ["errorcode", "错误代码", "errcode"] {
        if let Some(pos) = lower.find(pat) {
            let tail = &text[pos + pat.len()..];
            let digits: String = tail
                .chars()
                .skip_while(|c| !c.is_ascii_digit() && *c != '-')
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            if let Ok(n) = digits.parse::<i64>() {
                return Some(n);
            }
        }
    }
    // errorcode="40" / error code = 40
    if let Some(pos) = lower.find("errorcode") {
        let tail = &lower[pos + 9..];
        let eq = tail.find('=')?;
        let tail = &tail[eq + 1..];
        let tail = tail.trim_start_matches(['"', '\'', ':', ' ']);
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<i64>() {
            return Some(n);
        }
    }
    None
}

fn classify(text: &str, cat: &str, ok: Option<bool>) -> &'static str {
    if let Some(false) = ok {
        return "error";
    }
    let lower = text.to_lowercase();
    if lower.contains("错误代码")
        || lower.contains("errorcode")
        || lower.contains("failed")
        || lower.contains("失败")
        || lower.contains("fatal")
        || lower.contains("error")
    {
        return "error";
    }
    if lower.contains("warn") || lower.contains("警告") {
        return "warn";
    }
    if cat == "proc" {
        return "info";
    }
    "info"
}

pub fn enrich(mut e: HookEntry) -> HookEntry {
    let raw: Vec<u8> = if e.enc == "hex" {
        (0..e.data.len())
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&e.data[i..(i + 2).min(e.data.len())], 16).ok())
            .collect()
    } else {
        e.data.clone().into_bytes()
    };

    if e.cat == "net" {
        if looks_binary(&raw) {
            e.text = format!("<{} binary bytes: {}>", raw.len(), hex_prefix(&raw, 64));
        } else {
            e.text = try_decode(&raw);
        }
    } else if e.cat == "proc" {
        e.text = if e.cmd.is_empty() {
            e.app.clone()
        } else {
            e.cmd.clone()
        };
    } else {
        e.text = e.data.clone();
    }

    e.level = classify(&e.text, &e.cat, e.ok).to_string();
    e.err_code = extract_err_code(&e.text);
    if e.enc == "hex" {
        e.data = hex_prefix(&raw, 256);
        e.enc = "hex-preview".into();
    }
    e
}

fn hex_prefix(raw: &[u8], max: usize) -> String {
    raw.iter()
        .take(max)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse one JSONL line; silently drop malformed lines.
pub fn parse_line(line: &str) -> Option<HookEntry> {
    let e: HookEntry = serde_json::from_str(line).ok()?;
    let clear_ts = LAST_CLEAR_TS.load(Ordering::SeqCst);
    if clear_ts > 0 && e.ts < clear_ts {
        return None;
    }
    Some(enrich(e))
}

struct FileCursor {
    reader: BufReader<std::fs::File>,
    pos: u64,
}

pub fn start_stream(app: tauri::AppHandle) {
    if STREAMING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let dir = native::hook_dir();
        let mut cursors: HashMap<std::path::PathBuf, FileCursor> = HashMap::new();
        while STREAMING.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let mut batch = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for ent in rd.flatten() {
                    let path = ent.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }
                    let meta = match std::fs::metadata(&path) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    let cursor = cursors.entry(path.clone()).or_insert_with(|| {
                        let file = std::fs::File::open(&path).unwrap();
                        let mut reader = BufReader::new(file);
                        // start at end for files created before we attached
                        let pos = reader.seek(SeekFrom::End(0)).unwrap_or(0);
                        FileCursor { reader, pos }
                    });
                    if meta.len() < cursor.pos {
                        // truncated/rotated: restart
                        let _ = cursor.reader.seek(SeekFrom::Start(0));
                        cursor.pos = 0;
                    }
                    let mut lines = Vec::new();
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match cursor.reader.read_line(&mut line) {
                            Ok(0) => break,
                            Ok(_) => {
                                cursor.pos += line.len() as u64;
                                let t = line.trim();
                                if !t.is_empty() {
                                    lines.push(t.to_string());
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    if lines.len() > 5000 {
                        lines = lines.split_off(lines.len() - 5000);
                    }
                    for l in lines {
                        if let Some(e) = parse_line(&l) {
                            batch.push(e);
                        }
                    }
                }
            }
            if !batch.is_empty() {
                batch.sort_by_key(|e| e.ts);
                let _ = app.emit("hook-log", &batch);
            }
        }
    });
}

pub fn stop_stream() {
    STREAMING.store(false, Ordering::SeqCst);
}

/// Read all current hook log content (for initial load / refresh).
pub fn read_all(limit_per_file: usize) -> Vec<HookEntry> {
    let dir = native::hook_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(file) = std::fs::File::open(&path) {
                let reader = BufReader::new(file);
                let lines: Vec<String> = reader
                    .lines()
                    .map_while(Result::ok)
                    .filter(|l| !l.trim().is_empty())
                    .collect();
                let start = lines.len().saturating_sub(limit_per_file);
                for l in &lines[start..] {
                    if let Some(e) = parse_line(l) {
                        out.push(e);
                    }
                }
            }
        }
    }
    out.sort_by_key(|e| e.ts);
    out
}

pub fn clear_logs() {
    LAST_CLEAR_TS.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
        Ordering::SeqCst,
    );
}
