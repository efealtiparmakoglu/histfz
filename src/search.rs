//! The search driver: rank history entries against a query.

use crate::hist::{self, Entry};
use crate::score::{smith_waterman, substring_bonus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub entry: Entry,
    pub score: i32,
    pub ranges: Vec<(usize, usize)>,
}

impl Hit {
    /// The command with the matched parts wrapped in markers, ready to print.
    pub fn highlight(&self, open: &str, close: &str) -> String {
        let mut out = String::new();
        let mut cursor = 0usize;
        for (start, end) in &self.ranges {
            if *start < cursor {
                continue;
            }
            out.push_str(&self.entry.command[cursor..*start]);
            out.push_str(open);
            out.push_str(&self.entry.command[*start..*end]);
            out.push_str(close);
            cursor = *end;
        }
        out.push_str(&self.entry.command[cursor..]);
        out
    }

}

/// Rank entries against the query, keeping the best `limit`.
pub fn search(entries: &[Entry], query: &str, limit: usize, min_score: i32) -> Vec<Hit> {
    let mut hits: Vec<Hit> = entries
        .iter()
        .filter_map(|entry| {
            let alignment = smith_waterman(query, &entry.command);
            let score = alignment.score + substring_bonus(query, &entry.command);
            if score < min_score || score <= 0 {
                return None;
            }
            Some(Hit {
                entry: entry.clone(),
                score,
                ranges: alignment.ranges,
            })
        })
        .collect();

    // best score first; shorter commands win ties (more likely the thing
    // you meant than a long command that merely contains the words)
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.entry.command.len().cmp(&b.entry.command.len()))
            .then_with(|| b.entry.line.cmp(&a.entry.line))
    });
    hits.truncate(limit);
    hits
}

/// Most frequently used commands, newest first for ties.
pub fn top(entries: &[Entry], limit: usize) -> Vec<(String, usize)> {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for entry in entries {
        *counts.entry(entry.command.as_str()).or_insert(0) += 1;
    }
    let mut pairs: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(command, count)| (command.to_string(), count))
        .collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.truncate(limit);
    pairs
}

/// Copy the best match to the system clipboard. Returns false when no
/// clipboard command is available (headless machines, containers).
pub fn copy_to_clipboard(text: &str) -> bool {
    for command in ["pbcopy", "wl-copy", "xclip -selection clipboard"] {
        let mut parts = command.split_whitespace();
        let program = parts.next().unwrap_or("");
        let arguments: Vec<&str> = parts.collect();
        if !command_exists(program) {
            continue;
        }
        use std::io::Write;
        let child = std::process::Command::new(program)
            .args(&arguments)
            .stdin(std::process::Stdio::piped())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = writeln!(stdin, "{}", hist::copyable(text));
            } // the handle drops here, closing the pipe
            return child
                .wait()
                .map(|status| status.success())
                .unwrap_or(false);
        }
    }
    false
}

fn command_exists(program: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for directory in path.split(':') {
            let candidate = std::path::Path::new(directory).join(program);
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}
