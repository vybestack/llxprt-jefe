//! Pull-request changed-file, unified-diff, and review-anchor domain types.

/// Side of a pull-request diff used by review-thread and comment anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrReviewThreadSide {
    Left,
    Right,
}

/// Exact line/range metadata for one review thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrReviewThreadAnchor {
    pub side: PrReviewThreadSide,
    pub start_side: Option<PrReviewThreadSide>,
    pub start_line: Option<u32>,
    pub original_line: Option<u32>,
    pub original_start_line: Option<u32>,
}
/// Exact immutable target for one newly authored single-line review comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrReviewCommentTarget {
    pub path: String,
    pub line: u32,
    pub side: PrReviewThreadSide,
    pub commit_id: String,
}

/// GitHub's change classification for one pull-request file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrFileStatus {
    /// A file introduced by the pull request.
    Added,
    /// An existing file whose contents changed.
    Modified,
    /// A file deleted by the pull request.
    Removed,
    /// A file moved to a new path.
    Renamed,
    /// A file copied to a new path.
    Copied,
    /// A future GitHub status retained without dropping the file.
    Unknown(String),
}

impl PrFileStatus {
    /// Parse GitHub's status spelling without rejecting future values.
    #[must_use]
    pub fn from_api(value: &str) -> Self {
        match value {
            "added" => Self::Added,
            "modified" | "changed" => Self::Modified,
            "removed" => Self::Removed,
            "renamed" => Self::Renamed,
            "copied" => Self::Copied,
            unknown => Self::Unknown(unknown.to_owned()),
        }
    }

    /// Short stable marker used by the changed-files list.
    #[must_use]
    pub const fn marker(&self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Removed => "D",
            Self::Renamed => "R",
            Self::Copied => "C",
            Self::Unknown(_) => "?",
        }
    }
}

/// One file returned by GitHub's pull-request files endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFileChange {
    /// Immutable Git blob object id for lazy full-file lookup.
    pub blob_sha: String,
    /// Current path, or prior path for a removed file.
    pub path: String,
    /// Prior path for a rename when supplied by GitHub.
    pub previous_path: Option<String>,
    /// Change classification.
    pub status: PrFileStatus,
    /// Added line count.
    pub additions: u64,
    /// Removed line count.
    pub deletions: u64,
    /// Total changed line count.
    pub changes: u64,
    /// Unified patch hunks; absent for binary or oversized changes.
    pub patch: Option<String>,
}

/// Full immutable Git blob content for one changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrFileBlob {
    /// UTF-8 file text.
    Text(String),
    /// GitHub or the local Git object identified binary bytes.
    Binary,
    /// The blob exceeds the bounded full-file display contract.
    Truncated { byte_size: u64 },
}

/// Semantic kind of one unified-diff row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// Unchanged hunk context.
    Context,
    /// Line present on the new side.
    Added,
    /// Line present on the old side.
    Removed,
    /// Informational marker such as no-newline-at-EOF.
    Marker,
}

/// One line within a parsed hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Row classification.
    pub kind: DiffLineKind,
    /// Content after the unified-diff prefix.
    pub content: String,
    /// Old-side line number when this row exists on that side.
    pub old_line: Option<u32>,
    /// New-side line number when this row exists on that side.
    pub new_line: Option<u32>,
}

/// One parsed unified-diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// Original hunk header including optional section text.
    pub header: String,
    /// First old-side line from the header.
    pub old_start: u32,
    /// First new-side line from the header.
    pub new_start: u32,
    /// Parsed rows in source order.
    pub lines: Vec<DiffLine>,
}

/// Explicit result of parsing GitHub's optional patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedDiff {
    /// Parsed patch hunks.
    Hunks(Vec<DiffHunk>),
    /// GitHub omitted the patch.
    Unavailable,
    /// Patch text was present but did not follow unified-diff structure.
    Malformed(String),
}

/// Parse GitHub's optional unified patch into line-addressable hunks.
#[must_use]
pub fn parse_unified_diff(patch: Option<&str>) -> ParsedDiff {
    let Some(patch) = patch else {
        return ParsedDiff::Unavailable;
    };
    if patch.is_empty() {
        return ParsedDiff::Hunks(Vec::new());
    }
    let mut hunks = Vec::new();
    let mut current: Option<HunkBuilder> = None;
    for row in patch.lines() {
        if row.starts_with("@@") {
            if let Some(builder) = current.take() {
                hunks.push(builder.finish());
            }
            let Some((old_start, new_start)) = parse_hunk_header(row) else {
                return ParsedDiff::Malformed(format!("invalid hunk header: {row}"));
            };
            current = Some(HunkBuilder::new(row, old_start, new_start));
        } else if let Some(builder) = current.as_mut() {
            if builder.push(row).is_err() {
                return ParsedDiff::Malformed(format!("invalid patch row: {row}"));
            }
        } else {
            return ParsedDiff::Malformed("patch row appears before a hunk header".to_owned());
        }
    }
    if let Some(builder) = current {
        hunks.push(builder.finish());
    }
    ParsedDiff::Hunks(hunks)
}

fn parse_hunk_header(header: &str) -> Option<(u32, u32)> {
    let ranges = header.strip_prefix("@@ ")?.split(" @@").next()?;
    let mut parts = ranges.split_whitespace();
    let old = parse_range_start(parts.next()?, '-')?;
    let new = parse_range_start(parts.next()?, '+')?;
    Some((old, new))
}

fn parse_range_start(range: &str, prefix: char) -> Option<u32> {
    range
        .strip_prefix(prefix)?
        .split(',')
        .next()?
        .parse::<u32>()
        .ok()
}

struct HunkBuilder {
    header: String,
    old_start: u32,
    new_start: u32,
    old_line: u32,
    new_line: u32,
    lines: Vec<DiffLine>,
}

impl HunkBuilder {
    fn new(header: &str, old_start: u32, new_start: u32) -> Self {
        Self {
            header: header.to_owned(),
            old_start,
            new_start,
            old_line: old_start,
            new_line: new_start,
            lines: Vec::new(),
        }
    }

    fn push(&mut self, row: &str) -> Result<(), ()> {
        let (kind, content, old_line, new_line) = match row.as_bytes().first() {
            Some(b'+') => (DiffLineKind::Added, &row[1..], None, Some(self.take_new())),
            Some(b'-') => (
                DiffLineKind::Removed,
                &row[1..],
                Some(self.take_old()),
                None,
            ),
            Some(b' ') => {
                let old = self.take_old();
                let new = self.take_new();
                (DiffLineKind::Context, &row[1..], Some(old), Some(new))
            }
            Some(b'\\') if row == "\\ No newline at end of file" => {
                (DiffLineKind::Marker, &row[1..], None, None)
            }
            _ => return Err(()),
        };
        self.lines.push(DiffLine {
            kind,
            content: content.to_owned(),
            old_line,
            new_line,
        });
        Ok(())
    }

    fn take_old(&mut self) -> u32 {
        let line = self.old_line;
        self.old_line = self.old_line.saturating_add(1);
        line
    }

    fn take_new(&mut self) -> u32 {
        let line = self.new_line;
        self.new_line = self.new_line.saturating_add(1);
        line
    }

    fn finish(self) -> DiffHunk {
        DiffHunk {
            header: self.header,
            old_start: self.old_start,
            new_start: self.new_start,
            lines: self.lines,
        }
    }
}

#[cfg(test)]
#[path = "pr_diff_tests.rs"]
mod tests;
