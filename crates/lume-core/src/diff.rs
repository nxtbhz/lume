#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LineDiffStatus {
    Unchanged,
    Added,
    Modified,
    DeletedAbove,
}

#[derive(Debug, PartialEq, Eq)]
enum DiffOp {
    Keep(usize, usize), // (base_idx, curr_idx)
    Insert(usize),       // curr_idx
    Delete(usize),       // base_idx
}

/// Computes line-by-line diff status comparing `base_lines` against `current_lines`.
/// Returns a vector with one `LineDiffStatus` for each line in `current_lines`.
pub fn compute_line_diffs(base_lines: &[&str], current_lines: &[&str]) -> Vec<LineDiffStatus> {
    if current_lines.is_empty() {
        return Vec::new();
    }

    if base_lines.is_empty() {
        return vec![LineDiffStatus::Added; current_lines.len()];
    }

    // 1. Trim common prefix
    let mut prefix_len = 0;
    while prefix_len < base_lines.len()
        && prefix_len < current_lines.len()
        && base_lines[prefix_len] == current_lines[prefix_len]
    {
        prefix_len += 1;
    }

    if prefix_len == base_lines.len() && prefix_len == current_lines.len() {
        return vec![LineDiffStatus::Unchanged; current_lines.len()];
    }

    // 2. Trim common suffix
    let mut suffix_len = 0;
    while suffix_len < (base_lines.len() - prefix_len)
        && suffix_len < (current_lines.len() - prefix_len)
        && base_lines[base_lines.len() - 1 - suffix_len]
            == current_lines[current_lines.len() - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let mut result = vec![LineDiffStatus::Unchanged; current_lines.len()];

    let base_mid_start = prefix_len;
    let base_mid_end = base_lines.len() - suffix_len;
    let curr_mid_start = prefix_len;
    let curr_mid_end = current_lines.len() - suffix_len;

    let base_mid = &base_lines[base_mid_start..base_mid_end];
    let curr_mid = &current_lines[curr_mid_start..curr_mid_end];

    // Case: pure additions in the middle
    if base_mid.is_empty() {
        for idx in curr_mid_start..curr_mid_end {
            result[idx] = LineDiffStatus::Added;
        }
        return result;
    }

    // Case: pure deletions in the middle
    if curr_mid.is_empty() {
        // Mark the line right after the deletion
        if curr_mid_end < current_lines.len() {
            result[curr_mid_end] = LineDiffStatus::DeletedAbove;
        } else if !current_lines.is_empty() {
            // Deletion at EOF: mark the last line of current_lines
            result[current_lines.len() - 1] = LineDiffStatus::DeletedAbove;
        }
        return result;
    }

    // Dynamic Programming LCS for the middle slice
    let m = base_mid.len();
    let n = curr_mid.len();
    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 0..m {
        for j in 0..n {
            if base_mid[i] == curr_mid[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    // Backtrack to find edit script
    let mut ops = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && base_mid[i - 1] == curr_mid[j - 1] && dp[i][j] == dp[i - 1][j - 1] + 1 {
            ops.push(DiffOp::Keep(curr_mid_start + i - 1, curr_mid_start + j - 1));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j] == dp[i][j - 1]) {
            ops.push(DiffOp::Insert(curr_mid_start + j - 1));
            j -= 1;
        } else {
            ops.push(DiffOp::Delete(base_mid_start + i - 1));
            i -= 1;
        }
    }

    ops.reverse();

    // Group ops into hunks between Keeps
    let mut k = 0;
    while k < ops.len() {
        match ops[k] {
            DiffOp::Keep(_, curr_idx) => {
                result[curr_idx] = LineDiffStatus::Unchanged;
                k += 1;
            }
            _ => {
                // Collect contiguous block of Insert / Delete
                let mut inserts = Vec::new();
                let mut deletes_count = 0;

                while k < ops.len() {
                    match ops[k] {
                        DiffOp::Insert(c_idx) => {
                            inserts.push(c_idx);
                            k += 1;
                        }
                        DiffOp::Delete(_) => {
                            deletes_count += 1;
                            k += 1;
                        }
                        DiffOp::Keep(_, _) => break,
                    }
                }

                let num_inserts = inserts.len();
                let modified_count = deletes_count.min(num_inserts);

                for &idx in &inserts[..modified_count] {
                    result[idx] = LineDiffStatus::Modified;
                }
                for &idx in &inserts[modified_count..] {
                    result[idx] = LineDiffStatus::Added;
                }

                if deletes_count > 0 && num_inserts == 0 {
                    // Pure deletion: mark following line with DeletedAbove
                    if k < ops.len() {
                        if let DiffOp::Keep(_, next_curr) = ops[k] {
                            result[next_curr] = LineDiffStatus::DeletedAbove;
                        }
                    } else if curr_mid_end < current_lines.len() {
                        result[curr_mid_end] = LineDiffStatus::DeletedAbove;
                    } else if !current_lines.is_empty() {
                        result[current_lines.len() - 1] = LineDiffStatus::DeletedAbove;
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_identical_lines_all_unchanged() {
        let base = vec!["fn main() {", "    println!(\"hello\");", "}"];
        let curr = vec!["fn main() {", "    println!(\"hello\");", "}"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(diffs, vec![LineDiffStatus::Unchanged; 3]);
    }

    #[test]
    fn test_diff_empty_base_all_added() {
        let base: Vec<&str> = vec![];
        let curr = vec!["line 1", "line 2"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(diffs, vec![LineDiffStatus::Added, LineDiffStatus::Added]);
    }

    #[test]
    fn test_diff_empty_curr_returns_empty_vec() {
        let base = vec!["line 1", "line 2"];
        let curr: Vec<&str> = vec![];
        let diffs = compute_line_diffs(&base, &curr);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_diff_single_line_added_in_middle() {
        let base = vec!["alpha", "gamma"];
        let curr = vec!["alpha", "beta", "gamma"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![
                LineDiffStatus::Unchanged,
                LineDiffStatus::Added,
                LineDiffStatus::Unchanged
            ]
        );
    }

    #[test]
    fn test_diff_single_line_modified() {
        let base = vec!["let x = 1;", "let y = 2;"];
        let curr = vec!["let x = 10;", "let y = 2;"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![LineDiffStatus::Modified, LineDiffStatus::Unchanged]
        );
    }

    #[test]
    fn test_diff_line_deleted_shows_deleted_above() {
        let base = vec!["alpha", "beta", "gamma"];
        let curr = vec!["alpha", "gamma"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![
                LineDiffStatus::Unchanged,
                LineDiffStatus::DeletedAbove
            ]
        );
    }

    #[test]
    fn test_diff_line_deleted_at_eof() {
        let base = vec!["alpha", "beta", "gamma"];
        let curr = vec!["alpha", "beta"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![
                LineDiffStatus::Unchanged,
                LineDiffStatus::DeletedAbove
            ]
        );
    }

    #[test]
    fn test_diff_multibyte_utf8() {
        let base = vec!["coração", "música"];
        let curr = vec!["coração", "música brasileira", "dança"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![
                LineDiffStatus::Unchanged,
                LineDiffStatus::Modified,
                LineDiffStatus::Added
            ]
        );
    }
}
