//! histfz — fuzzy search over your shell history with Smith-Waterman
//! scoring, live filtering and copy-the-best-match.

use histfz::{hist, search};
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = "1.0.0";
const DEFAULT_HISTORY: &str = ".zsh_history";

const USAGE_TEMPLATE: &str = "\
histfz {version} — fuzzy shell history search (local, offline)

usage:
  histfz search <query> [--file PATH] [--limit N] [--min-score N] [--json] [--live]
  histfz top [--limit N] [--file PATH]
  histfz stats [--file PATH]
  histfz -h | --help

The query is matched with Smith-Waterman local alignment, so it can skip
characters: 'gmtr' finds `git merge --squash trunk`. Matches are
highlighted; the best hit is what --copy puts on the clipboard.

options:
  --file PATH   history file (default ~/{history})
  --limit N     how many hits to show (default 20)
  --min-score N drop hits below this score (default 1)
  --copy        copy the best match to the clipboard
  --live        print each hit as it is found, best last
  --json        machine-readable output

exit codes: 0 ok, 1 runtime error, 2 usage error
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        println!(
            "{}",
            USAGE_TEMPLATE.replace("{version}", VERSION).replace("{history}", DEFAULT_HISTORY)
        );
        return ExitCode::from(if args.is_empty() { 2 } else { 0 });
    }

    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("histfz: {message}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let command = args[0].as_str();
    let rest = &args[1..];

    let mut file: Option<PathBuf> = None;
    let mut limit: usize = 20;
    let mut min_score: i32 = 1;
    let mut as_json = false;
    let mut copy = false;
    let mut live = false;
    let mut positional: Vec<String> = Vec::new();

    let mut index = 0usize;
    while index < rest.len() {
        match rest[index].as_str() {
            "--file" => {
                index += 1;
                let value = rest.get(index).ok_or("--file needs a path")?;
                file = Some(PathBuf::from(expand_tilde(value)));
            }
            "--limit" => {
                index += 1;
                limit = rest
                    .get(index)
                    .ok_or("--limit needs a number")?
                    .parse()
                    .map_err(|_| "--limit must be a number".to_string())?;
            }
            "--min-score" => {
                index += 1;
                min_score = rest
                    .get(index)
                    .ok_or("--min-score needs a number")?
                    .parse()
                    .map_err(|_| "--min-score must be a number".to_string())?;
            }
            "--json" => as_json = true,
            "--copy" => copy = true,
            "--live" => live = true,
            "--version" => {
                println!("histfz {VERSION}");
                return Ok(ExitCode::SUCCESS);
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option '{other}'"));
            }
            other => positional.push(other.to_string()),
        }
        index += 1;
    }

    let path = file.unwrap_or_else(default_history_path);
    let entries = hist::read_history(&path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let raw_entries = hist::read_history_raw(&path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;

    match command {
        "search" => {
            if positional.is_empty() {
                return Err("search needs a query".into());
            }
            let query = positional.join(" ");
            let hits = search::search(&entries, &query, limit, min_score);
            if as_json {
                let payload: Vec<serde_lite::Hit> =
                    hits.iter().map(serde_lite::hit_to_json).collect();
                println!("{}", serde_lite::to_pretty(&payload));
            } else if hits.is_empty() {
                println!("no matches for '{query}'");
            } else if live {
                // live mode: worst first so the best match ends up on the
                // user's clipboard/mouse when they stop typing
                for hit in hits.iter().rev() {
                    println!("{:>5}  {}", hit.score, hit.highlight("[", "]"));
                }
            } else {
                for (rank, hit) in hits.iter().enumerate() {
                    println!(
                        "{:>3}. {:>5}  {}",
                        rank + 1,
                        hit.score,
                        hit.highlight("[4m", "[0m")
                    );
                }
            }
            if copy {
                match hits.first() {
                    Some(hit) => {
                        if search::copy_to_clipboard(&hit.entry.command) {
                            eprintln!("copied: {}", hist::copyable(&hit.entry.command));
                        } else {
                            eprintln!("no clipboard command available");
                            return Ok(ExitCode::from(1));
                        }
                    }
                    None => eprintln!("nothing to copy"),
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "top" => {
            // counts come from the undeduplicated history
            let pairs = search::top(&raw_entries, limit);
            if as_json {
                let payload: Vec<serde_lite::Count> = pairs
                    .iter()
                    .map(|(command, count)| serde_lite::Count {
                        command: command.clone(),
                        count: *count,
                    })
                    .collect();
                println!("{}", serde_lite::to_pretty_counts(&payload));
                return Ok(ExitCode::SUCCESS);
            }
            if pairs.is_empty() {
                println!("(empty history)");
                return Ok(ExitCode::SUCCESS);
            }
            for (rank, (command, count)) in pairs.iter().enumerate() {
                println!("{:>3}. {:>5}x  {}", rank + 1, count, command);
            }
            Ok(ExitCode::SUCCESS)
        }
        "stats" => {
            let dated = entries.iter().filter(|e| e.timestamp.is_some()).count();
            println!("file:      {}", path.display());
            println!("commands:  {}", raw_entries.len());
            println!("with time: {dated}");
            println!("unique:    {}", search::top(&raw_entries, usize::MAX).len());
            if let Some(newest) = entries.last() {
                println!("newest:    {}", newest.command);
            }
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

fn default_history_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let candidates = [
        format!("{home}/.zsh_history"),
        format!("{home}/.bash_history"),
        format!("{home}/.local/share/fish/fish_history"),
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from(&candidates[0])
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        return format!("{home}/{rest}");
    }
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    path.to_string()
}

/// A tiny JSON writer — the tool has three output shapes and a dependency
/// would be a silly price for them.
mod serde_lite {
    pub struct Hit {
        pub command: String,
        pub score: i32,
        pub ranges: Vec<(usize, usize)>,
    }

    pub struct Count {
        pub command: String,
        pub count: usize,
    }

    pub fn hit_to_json(hit: &crate::search::Hit) -> Hit {
        Hit {
            command: hit.entry.command.clone(),
            score: hit.score,
            ranges: hit.ranges.clone(),
        }
    }

    fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 2);
        for ch in text.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }

    pub fn to_pretty(hits: &[Hit]) -> String {
        let rows: Vec<String> = hits
            .iter()
            .map(|hit| {
                let ranges: Vec<String> = hit
                    .ranges
                    .iter()
                    .map(|(start, end)| format!("[{start}, {end}]"))
                    .collect();
                format!(
                    "  {{\n    \"command\": \"{}\",\n    \"score\": {},\n    \"ranges\": [{}]\n  }}",
                    escape(&hit.command),
                    hit.score,
                    ranges.join(", ")
                )
            })
            .collect();
        format!("[\n{}\n]", rows.join(",\n"))
    }

    pub fn to_pretty_counts(counts: &[Count]) -> String {
        let rows: Vec<String> = counts
            .iter()
            .map(|item| {
                format!(
                    "  {{\"command\": \"{}\", \"count\": {}}}",
                    escape(&item.command),
                    item.count
                )
            })
            .collect();
        format!("[\n{}\n]", rows.join(",\n"))
    }
}
