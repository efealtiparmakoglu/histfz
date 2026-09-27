//! Smith-Waterman local alignment, used to rank history entries against a
//! query. Local (not global) alignment is the right model here: the query
//! matches *somewhere* inside a long command line.

pub const MATCH_SCORE: i32 = 2;
pub const MISMATCH_SCORE: i32 = -1;
pub const GAP_SCORE: i32 = -2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// (start, end) byte ranges of the haystack that the query matched
    pub ranges: Vec<(usize, usize)>,
    pub score: i32,
}

/// Score `query` against `text`, case-insensitively, returning the best
/// local alignment and where it landed.
pub fn smith_waterman(query: &str, text: &str) -> Match {
    let pattern: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let target: Vec<char> = text.chars().flat_map(|c| c.to_lowercase()).collect();
    if pattern.is_empty() || target.is_empty() {
        return Match {
            ranges: Vec::new(),
            score: 0,
        };
    }

    let rows = pattern.len() + 1;
    let columns = target.len() + 1;
    // score[i][j] = best alignment of pattern[..i] ending at target[..j]
    let mut score = vec![0i32; rows * columns];
    // traceback direction: 0 = stop, 1 = diagonal, 2 = up, 3 = left
    let mut trace = vec![0u8; rows * columns];

    let mut best = 0i32;
    let mut best_at = (0usize, 0usize);

    for i in 1..rows {
        for j in 1..columns {
            let substitution = if pattern[i - 1] == target[j - 1] {
                MATCH_SCORE
            } else {
                MISMATCH_SCORE
            };
            let diagonal = score[(i - 1) * columns + (j - 1)] + substitution;
            let up = score[(i - 1) * columns + j] + GAP_SCORE;
            let left = score[i * columns + (j - 1)] + GAP_SCORE;
            let (mut value, mut direction) = (0, 0u8);
            if diagonal > value {
                value = diagonal;
                direction = 1;
            }
            if up > value {
                value = up;
                direction = 2;
            }
            if left > value {
                value = left;
                direction = 3;
            }
            score[i * columns + j] = value;
            trace[i * columns + j] = direction;
            if value > best {
                best = value;
                best_at = (i, j);
            }
        }
    }

    // walk back from the best cell, collecting matched target positions
    let mut matched: Vec<usize> = Vec::new();
    let (mut i, mut j) = best_at;
    while i > 0 && j > 0 && score[i * columns + j] > 0 {
        match trace[i * columns + j] {
            1 => {
                matched.push(j - 1);
                i -= 1;
                j -= 1;
            }
            2 => i -= 1,
            3 => j -= 1,
            _ => break,
        }
    }
    matched.reverse();

    // collapse consecutive positions into byte ranges
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut byte_offset = 0usize;
    let mut char_offsets: Vec<usize> = Vec::with_capacity(target.len() + 1);
    for ch in text.chars() {
        char_offsets.push(byte_offset);
        byte_offset += ch.len_utf8();
    }
    char_offsets.push(byte_offset);

    let mut index = 0usize;
    while index < matched.len() {
        let start = index;
        while index + 1 < matched.len() && matched[index + 1] == matched[index] + 1 {
            index += 1;
        }
        let first = matched[start];
        let last = matched[index];
        let range = (char_offsets[first], char_offsets[last + 1]);
        if ranges.last().map(|previous| previous.1 == range.0).unwrap_or(false) {
            if let Some(previous) = ranges.last_mut() {
                previous.1 = range.1;
            }
        } else {
            ranges.push(range);
        }
        index += 1;
    }

    Match { ranges, score: best }
}

/// A case-insensitive substring hit, used as a boost above the alignment
/// score: an exact substring is almost always what the user meant.
pub fn substring_bonus(query: &str, text: &str) -> i32 {
    if query.is_empty() {
        return 0;
    }
    let needle = query.to_lowercase();
    let haystack = text.to_lowercase();
    match haystack.find(&needle) {
        Some(position) => {
            let earlier = better_position(&haystack, &needle);
            let start_bonus = if position == 0 { 6 } else { 0 };
            start_bonus + earlier
        }
        None => 0,
    }
}

fn better_position(haystack: &str, needle: &str) -> i32 {
    // reward matches that start at a word boundary
    match haystack.find(needle) {
        Some(0) => 0,
        Some(position) => {
            let before = haystack[..position].chars().last().unwrap_or(' ');
            if before.is_whitespace() || before == '/' || before == '-' || before == '=' {
                3
            } else {
                0
            }
        }
        None => 0,
    }
}
