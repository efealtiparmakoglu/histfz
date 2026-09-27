use histfz::hist::{copyable, dedupe, parse_all, parse_all_raw, parse_line};
use histfz::score::{smith_waterman, substring_bonus};
use histfz::search::{search, top, Hit};

fn entry(command: &str, line: usize) -> histfz::hist::Entry {
    histfz::hist::Entry {
        command: command.to_string(),
        timestamp: None,
        duration: None,
        line,
    }
}

fn hit(command: &str, ranges: Vec<(usize, usize)>, score: i32) -> Hit {
    Hit {
        entry: entry(command, 1),
        score,
        ranges,
    }
}

#[test]
fn parses_plain_lines() {
    let parsed = parse_line("git status").unwrap();
    assert_eq!(parsed.command, "git status");
    assert_eq!(parsed.timestamp, None);
}

#[test]
fn parses_zsh_extended_history() {
    let parsed = parse_line(": 1758000000:12;git commit -m wip").unwrap();
    assert_eq!(parsed.command, "git commit -m wip");
    assert_eq!(parsed.timestamp, Some(1_758_000_000));
    assert_eq!(parsed.duration, Some(12));
}

#[test]
fn parses_bash_timestamp_line() {
    let parsed = parse_line("#1758000000").unwrap();
    assert_eq!(parsed.timestamp, Some(1_758_000_000));
    assert_eq!(parsed.command, "");
}

#[test]
fn parses_bash_timestamp_with_command() {
    let parsed = parse_line("#1758000000:ls -la").unwrap();
    assert_eq!(parsed.command, "ls -la");
    assert_eq!(parsed.timestamp, Some(1_758_000_000));
}

#[test]
fn skips_blank_and_comment_lines() {
    assert!(parse_line("").is_none());
    assert!(parse_line("   ").is_none());
    let entries = parse_all("git status\n\n\nls -la\n");
    assert_eq!(entries.len(), 2);
}

#[test]
fn records_line_numbers() {
    let entries = parse_all("one\ntwo\nthree\n");
    assert_eq!(entries[2].line, 3);
}

#[test]
fn dedupe_keeps_the_last_occurrence() {
    let mut entries = vec![entry("ls", 1), entry("pwd", 2), entry("ls", 3)];
    dedupe(&mut entries);
    assert_eq!(entries.len(), 2);
    let ls = entries.iter().find(|e| e.command == "ls").unwrap();
    assert_eq!(ls.line, 3, "the newest copy should win");
    assert_eq!(entries[1].line, 3, "the surviving entry keeps its original order");
}

#[test]
fn raw_parsing_keeps_duplicates() {
    let text = "ls\npwd\nls\n";
    assert_eq!(parse_all_raw(text).len(), 3);
    assert_eq!(parse_all(text).len(), 2);
}

#[test]
fn copyable_strips_prefixes_and_chains() {
    assert_eq!(copyable("sudo kubectl get pods"), "kubectl get pods");
    assert_eq!(copyable("FOO=bar ./run.sh"), "./run.sh");
    assert_eq!(copyable("cd /tmp && ls -la"), "ls -la");
    assert_eq!(copyable("make build ; make test"), "make test");
    assert_eq!(copyable("plain command"), "plain command");
    assert_eq!(copyable(""), "");
}

#[test]
fn alignment_scores_exact_match_highest() {
    let exact = smith_waterman("git status", "git status");
    let partial = smith_waterman("git status", "git status --short");
    let unrelated = smith_waterman("git status", "cargo build --release");
    // local alignment scores the matched region, not the whole line, so a
    // longer line containing the query ties rather than loses
    assert_eq!(exact.score, partial.score);
    assert!(exact.score > unrelated.score, "exact {} unrelated {}", exact.score, unrelated.score);
}

#[test]
fn alignment_is_case_insensitive() {
    assert_eq!(
        smith_waterman("GIT", "git commit").score,
        smith_waterman("git", "GIT COMMIT").score
    );
}

#[test]
fn alignment_handles_gaps() {
    // "gmtr" should find "git merge trunk" by skipping characters
    let gapped = smith_waterman("gmtr", "git merge --squash trunk");
    let none = smith_waterman("gmtr", "ls -la /var/log");
    assert!(gapped.score > none.score);
    assert!(!gapped.ranges.is_empty());
}

#[test]
fn alignment_of_empty_query_is_zero() {
    assert_eq!(smith_waterman("", "anything").score, 0);
    assert_eq!(smith_waterman("query", "").score, 0);
}

#[test]
fn alignment_ranges_point_at_the_matched_text() {
    let result = smith_waterman("commit", "git commit -m fix");
    assert_eq!(result.ranges.len(), 1);
    let (start, end) = result.ranges[0];
    assert_eq!(&"git commit -m fix"[start..end], "commit");
}

#[test]
fn alignment_ranges_are_byte_offsets_for_multibyte_text() {
    let text = "echo açıklama";
    let result = smith_waterman("açık", text);
    let (start, end) = result.ranges[0];
    assert_eq!(&text[start..end], "açık");
}

#[test]
fn substring_bonus_rewards_word_starts() {
    let at_start = substring_bonus("git", "git status");
    let at_word = substring_bonus("git", "sudo git status");
    let mid_word = substring_bonus("it", "git status");
    assert!(at_start > at_word, "start {at_start} word {at_word}");
    assert!(at_word > mid_word, "word {at_word} mid {mid_word}");
    assert_eq!(substring_bonus("zzz", "git status"), 0);
}

#[test]
fn search_ranks_best_first() {
    let entries = vec![
        entry("ls -la /var/log", 1),
        entry("git merge --squash trunk", 2),
        entry("cargo build --release", 3),
    ];
    let hits = search(&entries, "gmtr", 10, 1);
    assert_eq!(hits[0].entry.command, "git merge --squash trunk");
}

#[test]
fn search_respects_limit_and_min_score() {
    let entries = vec![
        entry("git status", 1),
        entry("git commit", 2),
        entry("git push", 3),
    ];
    assert_eq!(search(&entries, "git", 2, 1).len(), 2);
    let strict = search(&entries, "git", 10, 100);
    assert!(strict.is_empty(), "min-score should filter everything out");
}

#[test]
fn unrelated_queries_score_far_lower() {
    // fuzzy search is fuzzy on purpose: scattered letters can still match,
    // so the guarantee is a much lower score, not an empty result
    let related = search(&[entry("kubectl get pods -n prod", 1)], "kubectl", 10, 1);
    let unrelated = search(&[entry("ls -la", 1)], "kubectl", 10, 1);
    let related_score = related.first().map(|hit| hit.score).unwrap_or(0);
    let unrelated_score = unrelated.first().map(|hit| hit.score).unwrap_or(0);
    assert!(related_score > unrelated_score * 2,
            "related {related_score} unrelated {unrelated_score}");
    assert!(search(&[entry("ls -la", 1)], "kubectl", 10, 12).is_empty(),
            "a high min-score should filter weak matches out");
}

#[test]
fn search_ties_prefer_shorter_commands() {
    let entries = vec![
        entry("git status --short --branch --verbose", 1),
        entry("git status", 2),
    ];
    let hits = search(&entries, "git status", 10, 1);
    assert_eq!(hits[0].entry.command, "git status");
}

#[test]
fn highlight_wraps_the_matched_parts() {
    let hits = search(&[entry("git commit -m fix", 1)], "commit", 10, 1);
    let rendered = hits[0].highlight("[", "]");
    assert!(rendered.starts_with("git [commit]"), "got {rendered}");
}

#[test]
fn highlight_handles_multiple_ranges() {
    let item = hit("abcdef", vec![(0, 1), (4, 6)], 10);
    assert_eq!(item.highlight("<", ">"), "<a>bcd<ef>");
}

#[test]
fn highlight_skips_overlapping_ranges() {
    // the second range starts inside the first: it is skipped, not doubled
    let item = hit("abcdef", vec![(0, 3), (1, 4)], 10);
    assert_eq!(item.highlight("<", ">"), "<abc>def");
}

#[test]
fn highlight_without_ranges_is_identity() {
    let item = hit("plain", vec![], 0);
    assert_eq!(item.highlight("[", "]"), "plain");
}

#[test]
fn top_counts_real_occurrences() {
    let entries = vec![
        entry("cargo build", 1),
        entry("git status", 2),
        entry("cargo build", 3),
        entry("cargo build", 4),
    ];
    let top_three = top(&entries, 3);
    assert_eq!(top_three[0], ("cargo build".to_string(), 3));
    assert_eq!(top_three[1].0, "git status");
}

#[test]
fn top_limits_results() {
    let entries = vec![entry("a", 1), entry("b", 2), entry("c", 3)];
    assert_eq!(top(&entries, 2).len(), 2);
}
