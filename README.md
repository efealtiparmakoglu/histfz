# histfz

**Fuzzy search over your shell history — Smith-Waterman local alignment, so `gmtr` finds `git merge --squash trunk`. Parses zsh/bash/fish history, ranks hits, copies the best one. Rust, zero dependencies.**

```
$ histfz search gmtr
  1.     4  git merge --squash trunk
  2.     3  git status
  3.     3  git push origin main

$ histfz top --limit 5
  1.    42x  cargo build --release
  2.    31x  git status

$ histfz search kubectl --copy
  1.    17  sudo kubectl get pods -n production
copied: kubectl get pods -n production
```

`Ctrl-R` searches your history the way grep reads: literally. You remember
the command, not the string — it was `kubectl` and `get pods` and
something with `-n`. histfz scores every entry with a real alignment
algorithm, so approximate queries still land on the right line.

## Why histfz

- **Smith-Waterman, not grep.** Local sequence alignment with a traceback,
  written out: match +2, mismatch −1, gap −2. A query can skip characters
  inside the command and still win, which is the whole point — and the
  traceback tells you *which characters* matched, so the highlight is
  exact rather than a guess.
- **Word-boundary bonus.** A query that starts at the beginning of the
  line, or right after a space, `/`, `-` or `=`, outranks the same
  characters buried mid-word. Searching `git` prefers `git status` over
  `legitimate`.
- **Every history format.** zsh extended history (`: 1758000000:0;cmd`),
  bash `#epoch` and `#epoch:cmd`, plain lines, fish-style. Metadata is
  parsed when present and ignored cleanly when it is not.
- **Deduplication that keeps the useful copy.** Search shows the most
  recent occurrence of each command; `top` and `stats` count the raw
  history, because "how often do I run this" is a different question.
- **Copy-friendly output.** `--copy` puts the best match on the clipboard
  (pbcopy / wl-copy / xclip, whichever exists), stripped of `sudo`, of
  leading `VAR=value` assignments, and of everything before the last `&&`
  — because you usually want the last segment, not the whole chain.
- **UTF-8 safe highlights.** Ranges are byte offsets, and the tests check
  that a match inside `açıklama` highlights exactly the right bytes.
- **Offline by construction.** Reads a file, prints to a terminal. No
  network code exists in the crate.

## Usage

```sh
histfz search <query> [--file PATH] [--limit N] [--min-score N] [--json] [--live]
histfz top [--limit N] [--file PATH] [--json]
histfz stats [--file PATH]
histfz search "gmtr" --copy        # best match to the clipboard
histfz search git --live           # weakest first, so the best ends up last
```

Exit codes: 0 ok, 1 runtime error (unreadable history, no clipboard
available with `--copy`), 2 usage error.

## Development

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
```

26 tests cover each history format, deduplication, `copyable`'s prefix
and chain stripping, the alignment properties (exact ≥ partial >
unrelated, case-insensitivity, gap tolerance, empty query), byte-offset
ranges on multibyte text, ranking, tie-breaking, highlighting including
overlapping ranges, and the frequency counting.

## License

MIT — see [LICENSE](LICENSE).
