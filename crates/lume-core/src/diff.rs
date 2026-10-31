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
    Insert(usize),      // curr_idx
    Delete(usize),      // base_idx
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
        result[curr_mid_start..curr_mid_end].fill(LineDiffStatus::Added);
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

    let m = base_mid.len();
    let n = curr_mid.len();

    // Guard against excessive quadratic DP allocation on UI thread (e.g. massive reformat/paste)
    if (m as u64) * (n as u64) > 250_000 {
        let min_len = m.min(n);
        for idx in 0..min_len {
            result[curr_mid_start + idx] = LineDiffStatus::Modified;
        }
        for idx in min_len..n {
            result[curr_mid_start + idx] = LineDiffStatus::Added;
        }
        if m > n {
            if curr_mid_end < current_lines.len() {
                result[curr_mid_end] = LineDiffStatus::DeletedAbove;
            } else if !current_lines.is_empty() {
                result[current_lines.len() - 1] = LineDiffStatus::DeletedAbove;
            }
        }
        return result;
    }

    // 1D flattened DP table for cache efficiency & single allocation
    let stride = n + 1;
    let mut dp = vec![0u32; (m + 1) * (n + 1)];

    for i in 0..m {
        for j in 0..n {
            if base_mid[i] == curr_mid[j] {
                dp[(i + 1) * stride + (j + 1)] = dp[i * stride + j] + 1;
            } else {
                let up = dp[i * stride + (j + 1)];
                let left = dp[(i + 1) * stride + j];
                dp[(i + 1) * stride + (j + 1)] = up.max(left);
            }
        }
    }

    // Backtrack to find edit script
    let mut ops = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && base_mid[i - 1] == curr_mid[j - 1]
            && dp[i * stride + j] == dp[(i - 1) * stride + (j - 1)] + 1
        {
            ops.push(DiffOp::Keep(base_mid_start + i - 1, curr_mid_start + j - 1));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i * stride + j] == dp[i * stride + (j - 1)]) {
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
                if result[curr_idx] != LineDiffStatus::DeletedAbove {
                    result[curr_idx] = LineDiffStatus::Unchanged;
                }
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

/// Computes line-by-line diff between two ropes, avoiding string allocations for common prefix and suffix.
pub fn compute_rope_diffs(base: &ropey::Rope, curr: &ropey::Rope) -> Vec<LineDiffStatus> {
    let curr_len = curr.len_lines();
    if curr_len == 0 {
        return Vec::new();
    }

    let base_len = base.len_lines();
    if base_len == 0 {
        return vec![LineDiffStatus::Added; curr_len];
    }

    // 1. Common prefix trimming directly on RopeSlice (zero string allocations)
    let mut p = 0;
    while p < base_len && p < curr_len {
        let base_line = base.line(p);
        let curr_line = curr.line(p);
        if base_line == curr_line {
            p += 1;
        } else {
            break;
        }
    }

    if p == base_len && p == curr_len {
        return vec![LineDiffStatus::Unchanged; curr_len];
    }

    // 2. Common suffix trimming directly on RopeSlice (zero string allocations)
    let mut s = 0;
    while s < (base_len - p) && s < (curr_len - p) {
        let base_line = base.line(base_len - 1 - s);
        let curr_line = curr.line(curr_len - 1 - s);
        if base_line == curr_line {
            s += 1;
        } else {
            break;
        }
    }

    // 3. Extract only the middle slice strings
    let base_mid_start = p;
    let base_mid_end = base_len - s;
    let curr_mid_start = p;
    let curr_mid_end = curr_len - s;

    let base_mid_strings: Vec<String> = (base_mid_start..base_mid_end)
        .map(|idx| {
            let mut line = base.line(idx).to_string();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            line
        })
        .collect();

    let curr_mid_strings: Vec<String> = (curr_mid_start..curr_mid_end)
        .map(|idx| {
            let mut line = curr.line(idx).to_string();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            line
        })
        .collect();

    let base_refs: Vec<&str> = base_mid_strings
        .iter()
        .map(|str_val| str_val.as_str())
        .collect();
    let curr_refs: Vec<&str> = curr_mid_strings
        .iter()
        .map(|str_val| str_val.as_str())
        .collect();

    let mut result = vec![LineDiffStatus::Unchanged; curr_len];

    // Compute diff on middle slice
    let mid_diffs = compute_line_diffs(&base_refs, &curr_refs);
    for (i, &status) in mid_diffs.iter().enumerate() {
        result[curr_mid_start + i] = status;
    }

    // If middle in curr was empty, handle deletion boundary
    if curr_mid_strings.is_empty() && !base_mid_strings.is_empty() {
        if curr_mid_end < curr_len {
            result[curr_mid_end] = LineDiffStatus::DeletedAbove;
        } else if curr_len > 0 {
            result[curr_len - 1] = LineDiffStatus::DeletedAbove;
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
            vec![LineDiffStatus::Unchanged, LineDiffStatus::DeletedAbove]
        );
    }

    #[test]
    fn test_diff_line_deleted_at_eof() {
        let base = vec!["alpha", "beta", "gamma"];
        let curr = vec!["alpha", "beta"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![LineDiffStatus::Unchanged, LineDiffStatus::DeletedAbove]
        );
    }

    #[test]
    fn test_diff_deletion_followed_by_kept_line_in_lcs() {
        let base = vec!["start", "delete_me", "keep_me", "old_end", "tail"];
        let curr = vec!["start", "keep_me", "new_end", "tail"];
        let diffs = compute_line_diffs(&base, &curr);
        assert_eq!(
            diffs,
            vec![
                LineDiffStatus::Unchanged,
                LineDiffStatus::DeletedAbove,
                LineDiffStatus::Modified,
                LineDiffStatus::Unchanged
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
