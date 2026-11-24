//! Unified view of past Claude Code conversations (~/.claude/projects).

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use serde_json::Value;

const MAX_SEARCH_BYTES: usize = 200_000;

pub struct Entry {
    pub path: PathBuf,
    pub project: String,
    /// Session id (the `.jsonl` file stem), as accepted by `claude --resume`.
    pub session_id: String,
    /// Working directory the conversation ran in, if recorded.
    pub cwd: Option<PathBuf>,
    pub title: String,
    pub modified: SystemTime,
    /// Lowercased text used for searching.
    haystack: String,
}

impl Entry {
    /// An entry that is not backed by a file, for tests.
    #[cfg(test)]
    pub fn fake(project: &str, title: &str) -> Self {
        Self {
            path: PathBuf::new(),
            project: project.into(),
            session_id: "id".into(),
            cwd: None,
            title: title.into(),
            modified: SystemTime::now(),
            haystack: format!("{project} {title}").to_lowercase(),
        }
    }
}

pub struct Message {
    pub role: String,
    pub text: String,
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter(|i| i["type"] == "text")
            .filter_map(|i| i["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn parse_messages(raw: &str) -> Vec<Message> {
    raw.lines()
        .filter(|l| l.contains("\"type\":\"user\"") || l.contains("\"type\":\"assistant\""))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            let role = v["type"].as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            if v["isSidechain"] == true {
                return None;
            }
            let text = text_of(&v["message"]["content"]);
            let text = text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            Some(Message {
                role: role.to_string(),
                text,
            })
        })
        .collect()
}

pub fn load_messages(entry: &Entry) -> Vec<Message> {
    fs::read_to_string(&entry.path)
        .map(|raw| parse_messages(&raw))
        .unwrap_or_default()
}

pub fn load_all() -> Vec<Entry> {
    let Some(home) = std::env::var_os("HOME") else {
        return vec![];
    };
    let root = PathBuf::from(home).join(".claude/projects");
    let mut entries = vec![];
    let Ok(dirs) = fs::read_dir(&root) else {
        return entries;
    };
    for dir in dirs.flatten() {
        let Ok(files) = fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let modified = file
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let messages = parse_messages(&raw);
            if messages.is_empty() {
                continue;
            }
            let cwd = raw
                .split_once("\"cwd\":\"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(cwd, _)| cwd.to_string());
            let session_id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let project = cwd
                .clone()
                .unwrap_or_else(|| dir.file_name().to_string_lossy().into_owned());
            let title = messages
                .iter()
                .find(|m| m.role == "user" && !m.text.starts_with('<'))
                .or_else(|| messages.first())
                .map(|m| m.text.lines().next().unwrap_or("").to_string())
                .unwrap_or_default();
            let mut haystack = String::new();
            haystack.push_str(&project);
            for m in &messages {
                if haystack.len() > MAX_SEARCH_BYTES {
                    break;
                }
                haystack.push('\n');
                haystack.push_str(&m.text);
            }
            entries.push(Entry {
                path,
                project,
                session_id,
                cwd: cwd.map(PathBuf::from),
                title,
                modified,
                haystack: haystack.to_lowercase(),
            });
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.modified));
    entries
}

pub fn matches(entry: &Entry, query: &str) -> bool {
    let q = query.to_lowercase();
    q.split_whitespace().all(|w| entry.haystack.contains(w))
}

pub fn age(entry: &Entry) -> String {
    let secs = SystemTime::now()
        .duration_since(entry.modified)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_and_assistant_text() {
        let raw = r#"{"type":"mode"}
{"type":"user","message":{"role":"user","content":"hello"}}
{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"x"},{"type":"text","text":"hi there"}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"x"}]}}"#;
        let m = parse_messages(raw);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].text, "hello");
        assert_eq!(m[1].role, "assistant");
        assert_eq!(m[1].text, "hi there");
    }
}
