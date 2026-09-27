//! Shell history parsing: zsh extended history, zsh/bash plain lines, and
//! the `#1234` / `!1234` timestamp forms. Every format degrades to "the
//! command text" when the metadata is missing or unparsable.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub command: String,
    pub timestamp: Option<i64>,
    pub duration: Option<u32>,
    pub line: usize,
}

/// Parse one history line. Returns None for blank lines and comments.
pub fn parse_line(line: &str) -> Option<Entry> {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    if trimmed.trim().is_empty() {
        return None;
    }

    // zsh extended history: ": 1758000000:0;command"
    if let Some(rest) = trimmed.strip_prefix(": ") {
        if let Some((meta, command)) = rest.split_once(';') {
            if let Some((seconds, elapsed)) = meta.split_once(':') {
                if let Ok(timestamp) = seconds.trim().parse::<i64>() {
                    return Some(Entry {
                        command: command.trim().to_string(),
                        timestamp: Some(timestamp),
                        duration: elapsed.trim().parse::<u32>().ok(),
                        line: 0,
                    });
                }
            }
        }
    }

    // bash with timestamps: "#1758000000" or HISTTIMEFORMAT's "#1758000000:cmd"
    if let Some(stripped) = trimmed.strip_prefix('#') {
        if let Some((stamp, command)) = stripped.split_once(':') {
            if let Ok(timestamp) = stamp.trim().parse::<i64>() {
                let command = command.trim();
                if !command.is_empty() {
                    return Some(Entry {
                        command: command.to_string(),
                        timestamp: Some(timestamp),
                        duration: None,
                        line: 0,
                    });
                }
            }
        }
        if let Ok(timestamp) = stripped.trim().parse::<i64>() {
            return Some(Entry {
                command: String::new(),
                timestamp: Some(timestamp),
                duration: None,
                line: 0,
            });
        }
    }

    Some(Entry {
        command: trimmed.to_string(),
        timestamp: None,
        duration: None,
        line: 0,
    })
}

/// Read a history file, keeping the newest occurrence of each command.
pub fn read_history(path: &Path) -> std::io::Result<Vec<Entry>> {
    let text = fs::read_to_string(path)?;
    Ok(parse_all(&text))
}

/// Read every entry, duplicates included — `top` needs the real counts.
pub fn read_history_raw(path: &Path) -> std::io::Result<Vec<Entry>> {
    let text = fs::read_to_string(path)?;
    Ok(parse_all_raw(&text))
}

/// Parse without deduplicating.
pub fn parse_all_raw(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if let Some(mut entry) = parse_line(line) {
            if entry.command.is_empty() {
                continue;
            }
            entry.line = index + 1;
            entries.push(entry);
        }
    }
    entries
}

pub fn parse_all(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if let Some(mut entry) = parse_line(line) {
            if entry.command.is_empty() {
                continue;
            }
            entry.line = index + 1;
            entries.push(entry);
        }
    }
    dedupe(&mut entries);
    entries
}

/// Keep the last occurrence of each command (that is the one whose
/// environment state matches "what I just ran").
pub fn dedupe(entries: &mut Vec<Entry>) {
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<Entry> = Vec::new();
    for entry in entries.iter().rev() {
        let key = entry.command.trim();
        if seen.insert(key.to_string()) {
            kept.push(entry.clone());
        }
    }
    kept.reverse();
    *entries = kept;
}

/// The command part most worth copying: strips a leading `sudo`, a
/// leading `VAR=value` prefix, and a trailing `&&`-chained segment.
pub fn copyable(command: &str) -> &str {
    let mut text = command.trim();
    for prefix in ["sudo ", "doas ", "env "] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim_start();
        }
    }
    // drop leading environment assignments
    loop {
        let mut split = None;
        for (index, part) in text.split_whitespace().enumerate() {
            if part.contains('=') && part.starts_with(|c: char| c.is_ascii_alphabetic()) {
                if index == 0 {
                    split = text.find(char::is_whitespace);
                }
                break;
            }
            if index > 0 {
                break;
            }
        }
        match split {
            Some(position) => text = text[position..].trim_start(),
            None => break,
        }
    }
    // the last segment of an && / ; / | chain is usually what you want
    for separator in ["&&", "||", ";", "|"] {
        if let Some(position) = text.rfind(separator) {
            text = text[position + separator.len()..].trim_start();
        }
    }
    if text.is_empty() {
        command.trim()
    } else {
        text
    }
}
