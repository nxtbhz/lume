//! Zero-dependency fuzzy subsequence matching engine for fast file filtering.

/// Result of a successful fuzzy match containing score and character indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Match score (higher is better).
    pub score: i64,
    /// Indices of matched characters in the candidate string (0-based character positions).
    pub match_indices: Vec<usize>,
}

/// Matches a query pattern against a candidate string.
///
/// Supports:
/// - Space-separated multi-token queries (`"tui main"`). Each token must match.
/// - Case-insensitive matching with exact-case bonus.
/// - Boundary bonus (start of string or immediately following `/`, `\`, `_`, `-`, `.`, or space).
/// - Filename bonus (characters within the final path segment).
/// - Consecutive match bonus and distance penalty between matches.
pub fn fuzzy_match(pattern: &str, candidate: &str) -> Option<FuzzyMatch> {
    if pattern.trim().is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            match_indices: Vec::new(),
        });
    }

    let tokens: Vec<&str> = pattern.split_whitespace().collect();
    if tokens.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            match_indices: Vec::new(),
        });
    }

    let cand_chars: Vec<char> = candidate.chars().collect();
    let cand_lower_chars: Vec<char> = cand_chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();

    let filename_start = cand_chars
        .iter()
        .rposition(|&c| c == '/' || c == '\\')
        .map(|pos| pos + 1)
        .unwrap_or(0);

    let mut total_score = 0i64;
    let mut combined_indices = Vec::new();

    for token in tokens {
        let token_chars: Vec<char> = token.chars().collect();
        let token_lower_chars: Vec<char> = token_chars
            .iter()
            .map(|c| c.to_lowercase().next().unwrap_or(*c))
            .collect();

        if let Some((score, indices)) = match_single_token(
            &token_chars,
            &token_lower_chars,
            &cand_chars,
            &cand_lower_chars,
            filename_start,
        ) {
            total_score += score;
            combined_indices.extend(indices);
        } else {
            return None;
        }
    }

    combined_indices.sort_unstable();
    combined_indices.dedup();

    Some(FuzzyMatch {
        score: total_score,
        match_indices: combined_indices,
    })
}

/// Computes the fuzzy match score of a pattern against a candidate.
pub fn fuzzy_score(pattern: &str, candidate: &str) -> Option<i64> {
    fuzzy_match(pattern, candidate).map(|m| m.score)
}

fn is_separator(c: char) -> bool {
    matches!(c, '/' | '\\' | '_' | '-' | '.' | ' ')
}

/// Matches a single whitespace-free token against the candidate using memoized DP.
fn match_single_token(
    token_chars: &[char],
    token_lower: &[char],
    cand_chars: &[char],
    cand_lower: &[char],
    filename_start: usize,
) -> Option<(i64, Vec<usize>)> {
    let m = token_chars.len();
    let n = cand_chars.len();

    if m == 0 {
        return Some((0, Vec::new()));
    }
    if m > n {
        return None;
    }

    // Fast greedy check: verify candidate contains token characters as a subsequence
    let mut c_it = 0;
    for &tc in token_lower {
        let mut found = false;
        while c_it < n {
            if cand_lower[c_it] == tc {
                found = true;
                c_it += 1;
                break;
            }
            c_it += 1;
        }
        if !found {
            return None;
        }
    }

    // DP with memoization: state is (p, prev), where prev is 0..=n (n denotes None)
    // Table dimensions: (m + 1) * (n + 1)
    let stride = n + 1;
    let mut memo = vec![None; (m + 1) * stride];
    let mut best_choice = vec![0usize; (m + 1) * stride];

    fn solve(
        p: usize,
        prev: usize,
        token_chars: &[char],
        token_lower: &[char],
        cand_chars: &[char],
        cand_lower: &[char],
        filename_start: usize,
        memo: &mut [Option<Option<i64>>],
        best_choice: &mut [usize],
        stride: usize,
    ) -> Option<i64> {
        let m = token_chars.len();
        let n = cand_chars.len();

        if p == m {
            return Some(0);
        }

        let idx = p * stride + prev;
        if let Some(res) = memo[idx] {
            return res;
        }

        let min_c = if prev == n { 0 } else { prev + 1 };
        let remaining_tokens = m - p - 1;
        let max_c = if n >= remaining_tokens {
            n - remaining_tokens
        } else {
            0
        };

        let mut max_score: Option<i64> = None;
        let mut choice = 0;

        let target_char = token_lower[p];

        for curr in min_c..max_c {
            if cand_lower[curr] != target_char {
                continue;
            }

            // Calculate bonuses for matching at `curr`
            let mut char_score = 10i64; // Base hit

            // Boundary bonus: start of string or after separator
            if curr == 0 || is_separator(cand_chars[curr - 1]) {
                char_score += 30;
            }

            // Filename bonus: in filename portion
            if curr >= filename_start {
                char_score += 25;
            }

            // Exact case bonus
            if cand_chars[curr] == token_chars[p] {
                char_score += 5;
            }

            // Consecutive bonus & distance penalty
            if prev != n {
                if curr == prev + 1 {
                    char_score += 25;
                } else {
                    let gap = (curr - prev - 1) as i64;
                    char_score -= gap;
                }
            }

            if let Some(rest_score) = solve(
                p + 1,
                curr,
                token_chars,
                token_lower,
                cand_chars,
                cand_lower,
                filename_start,
                memo,
                best_choice,
                stride,
            ) {
                let candidate_total = char_score + rest_score;
                if max_score.is_none() || candidate_total > max_score.unwrap() {
                    max_score = Some(candidate_total);
                    choice = curr;
                }
            }
        }

        memo[idx] = Some(max_score);
        if max_score.is_some() {
            best_choice[idx] = choice;
        }
        max_score
    }

    let best_score = solve(
        0,
        n,
        token_chars,
        token_lower,
        cand_chars,
        cand_lower,
        filename_start,
        &mut memo,
        &mut best_choice,
        stride,
    )?;

    // Reconstruct matched indices
    let mut indices = Vec::with_capacity(m);
    let mut p = 0;
    let mut prev = n;
    while p < m {
        let idx = p * stride + prev;
        let curr = best_choice[idx];
        indices.push(curr);
        prev = curr;
        p += 1;
    }

    Some((best_score, indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_empty_pattern() {
        let m = fuzzy_match("", "crates/lume-tui/src/main.rs")
            .expect("empty pattern matches everything");
        assert_eq!(m.score, 0);
        assert!(m.match_indices.is_empty());
    }

    #[test]
    fn test_fuzzy_match_single_term_subsequence() {
        assert!(fuzzy_match("xyz", "crates/lume-tui/src/main.rs").is_none());

        let m1 = fuzzy_match("main", "crates/lume-tui/src/main.rs").expect("should match main");
        assert!(m1.score > 0);

        let m2 =
            fuzzy_match("tuimain", "crates/lume-tui/src/main.rs").expect("should match tuimain");
        assert!(m2.score > 0);
    }

    #[test]
    fn test_fuzzy_match_multi_term_spaces() {
        let m = fuzzy_match("tui main", "crates/lume-tui/src/main.rs")
            .expect("should match both tokens");
        assert!(m.score > 0);
        // Both 'tui' and 'main' indices must be present
        assert!(m.match_indices.len() >= 7);

        // One token missing causes match failure
        assert!(fuzzy_match("tui missing", "crates/lume-tui/src/main.rs").is_none());
    }

    #[test]
    fn test_fuzzy_match_prefers_filename_and_consecutive() {
        let score_file = fuzzy_score("main", "crates/lume-tui/src/main.rs").unwrap();
        let score_dir = fuzzy_score("main", "crates/main_dir/src/lib.rs").unwrap();
        assert!(
            score_file > score_dir,
            "matches in filename should score higher than deep directory matches"
        );

        let score_consecutive = fuzzy_score("tui", "lume-tui.rs").unwrap();
        let score_scattered = fuzzy_score("tui", "tar_uncompressed_item.rs").unwrap();
        assert!(
            score_consecutive > score_scattered,
            "consecutive characters should score higher than scattered matches"
        );
    }

    #[test]
    fn test_fuzzy_match_multibyte_utf8() {
        let cand = "documentos/ações/música_final.txt";
        let m = fuzzy_match("música", cand).expect("should match unicode word");
        assert!(m.score > 0);

        let m_accents = fuzzy_match("ações música", cand).expect("should match multi-word unicode");
        assert!(m_accents.score > 0);
    }
}
