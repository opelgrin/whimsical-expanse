//! Flags documents that have grown large enough to deserve a review.
//!
//! Size is a signal, not a rule: a long document is fine when it has one scope
//! and is read as a whole, and splitting it for the sake of a number only
//! scatters it (see `dev/README.md`, "Keeping documents navigable"). This tool
//! therefore prints warnings and exits successfully. It fails only when it
//! cannot read the documents.
//!
//! Size is measured in bytes, which approximate the context a reader spends.
//! Unlike lines, bytes cannot be reduced by reflowing text.
//!
//! A reviewed document records its size at review time with a marker such as
//! `<!-- size-reviewed: 14200 -->`. It is not flagged again until it has grown
//! by more than a quarter beyond that size.
//!
//! Usage: `expanse-docs-check [ROOT]`, where `ROOT` defaults to the current
//! directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Developer documents are loaded by coding agents, so they have the tighter
/// limit. About 250 lines at the density of the current documents.
const DEV_LIMIT: usize = 10_000;
/// Design documents are read as a whole by people.
const DOCS_LIMIT: usize = 16_000;

/// A reviewed document is flagged again after growing past this fraction of its
/// reviewed size, written as a numerator and a denominator.
const GROWTH_ALLOWED: (usize, usize) = (5, 4);

const MARKER_START: &str = "<!-- size-reviewed:";
const MARKER_END: &str = "-->";

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// Over the limit and never reviewed.
    Unreviewed { size: usize, limit: usize },
    /// Reviewed, but grown well beyond the reviewed size.
    GrownSinceReview { size: usize, reviewed: usize },
}

fn main() -> ExitCode {
    let root = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    match check(&root) {
        Ok(findings) => {
            report(&render(&findings));
            ExitCode::SUCCESS
        }
        Err(error) => {
            report_error(&error);
            ExitCode::from(2)
        }
    }
}

/// Assesses every Markdown file under `dev/` and `docs/` of `root`, in a stable
/// order.
fn check(root: &Path) -> io::Result<Vec<(String, Verdict)>> {
    let mut findings = Vec::new();
    for (directory, limit) in [("dev", DEV_LIMIT), ("docs", DOCS_LIMIT)] {
        let mut files = Vec::new();
        collect_markdown(&root.join(directory), &mut files)?;
        files.sort();
        for file in files {
            let text = fs::read_to_string(&file)?;
            if let Some(verdict) = assess(text.len(), limit, reviewed_size(&text)) {
                let relative = file.strip_prefix(root).unwrap_or(&file);
                findings.push((relative.to_string_lossy().replace('\\', "/"), verdict));
            }
        }
    }
    Ok(findings)
}

fn collect_markdown(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_markdown(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

fn assess(size: usize, limit: usize, reviewed: Option<usize>) -> Option<Verdict> {
    if size <= limit {
        return None;
    }
    match reviewed {
        None => Some(Verdict::Unreviewed { size, limit }),
        Some(reviewed) => {
            let (numerator, denominator) = GROWTH_ALLOWED;
            let grown = size.saturating_mul(denominator) > reviewed.saturating_mul(numerator);
            grown.then_some(Verdict::GrownSinceReview { size, reviewed })
        }
    }
}

/// The size recorded by the review marker, if the text has a well-formed one.
fn reviewed_size(text: &str) -> Option<usize> {
    let after_start = text.split_once(MARKER_START)?.1;
    let number = after_start.split_once(MARKER_END)?.0;
    number.trim().parse().ok()
}

fn render(findings: &[(String, Verdict)]) -> String {
    let mut output = String::new();
    for (path, verdict) in findings {
        let line = match verdict {
            Verdict::Unreviewed { size, limit } => format!(
                "{path}: {size} bytes, over the {limit} byte signal. Review it (see \
                 dev/README.md). If it is fine as it is, add \
                 `{MARKER_START} {size} {MARKER_END}`.\n"
            ),
            Verdict::GrownSinceReview { size, reviewed } => format!(
                "{path}: {size} bytes, grown since its review at {reviewed} bytes. \
                 Review it again and update the marker.\n"
            ),
        };
        output.push_str(&line);
    }
    output
}

#[expect(
    clippy::print_stdout,
    reason = "the warnings are this program's output"
)]
fn report(text: &str) {
    print!("{text}");
}

#[expect(clippy::print_stderr, reason = "read errors go to the terminal")]
fn report_error(error: &io::Error) {
    eprintln!("error: {error}");
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A scratch directory that is removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "expanse-docs-check-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("the scratch directory can be created");
            Self(path)
        }

        fn write(&self, relative: &str, size: usize) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().expect("test paths have a parent"))
                .expect("the parent can be created");
            fs::write(path, "x".repeat(size)).expect("the file can be written");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const LIMIT: usize = 1_000;

    #[test]
    fn a_document_exactly_at_the_limit_is_not_flagged() {
        // Catches an off-by-one that would warn about documents within budget.
        assert_eq!(assess(LIMIT, LIMIT, None), None);
        assert!(assess(LIMIT + 1, LIMIT, None).is_some());
    }

    #[test]
    fn an_unreviewed_document_over_the_limit_is_flagged() {
        assert_eq!(
            assess(LIMIT + 1, LIMIT, None),
            Some(Verdict::Unreviewed {
                size: LIMIT + 1,
                limit: LIMIT
            })
        );
    }

    #[test]
    fn a_reviewed_document_stays_quiet_until_it_grows_past_the_allowance() {
        // Catches a review marker that silences the document forever, and one
        // that is ignored. Exactly a quarter more is still allowed.
        let reviewed = 2_000;
        let (numerator, denominator) = GROWTH_ALLOWED;
        let allowed = reviewed * numerator / denominator;
        assert_eq!(assess(reviewed, LIMIT, Some(reviewed)), None);
        assert_eq!(assess(allowed, LIMIT, Some(reviewed)), None);
        assert_eq!(
            assess(allowed + 1, LIMIT, Some(reviewed)),
            Some(Verdict::GrownSinceReview {
                size: allowed + 1,
                reviewed
            })
        );
    }

    #[test]
    fn a_reviewed_document_that_shrank_below_the_limit_is_not_flagged() {
        // Catches the marker being consulted before the limit.
        assert_eq!(assess(LIMIT - 1, LIMIT, Some(5_000)), None);
    }

    #[test]
    fn the_marker_is_read_from_anywhere_in_the_text() {
        let text = "# Title\n\nbody\n\n<!-- size-reviewed: 14200 -->\n";
        assert_eq!(reviewed_size(text), Some(14_200));
    }

    #[test]
    fn a_malformed_marker_does_not_count_as_a_review() {
        // Catches a typo silently silencing the warning.
        for text in [
            "no marker",
            "<!-- size-reviewed: lots -->",
            "<!-- size-reviewed: 100",
            "<!-- size-reviewed: -->",
        ] {
            assert_eq!(reviewed_size(text), None, "{text}");
        }
    }

    #[test]
    fn dev_and_docs_use_their_own_limits_and_nested_files_are_found() {
        // Catches one limit being applied to both trees, and files in
        // subdirectories (such as a split architecture) being skipped.
        let scratch = Scratch::new();
        let between = DEV_LIMIT + 1;
        assert!(
            between <= DOCS_LIMIT,
            "the test needs the dev limit to be lower"
        );
        scratch.write("dev/over.md", between);
        scratch.write("dev/architecture/nested.md", between);
        scratch.write("docs/fine.md", between);
        scratch.write("docs/over.md", DOCS_LIMIT + 1);
        scratch.write("dev/notes.txt", DOCS_LIMIT + 1);

        let paths: Vec<String> = check(&scratch.0)
            .expect("the scratch tree is readable")
            .into_iter()
            .map(|(path, _)| path)
            .collect();

        assert_eq!(
            paths,
            ["dev/architecture/nested.md", "dev/over.md", "docs/over.md"]
        );
    }

    #[test]
    fn a_missing_directory_is_an_error_not_a_clean_bill() {
        // Catches running from the wrong directory and reporting "all fine".
        let scratch = Scratch::new();
        assert!(check(&scratch.0).is_err());
    }

    #[test]
    fn the_report_is_empty_when_all_is_well_and_names_the_document_otherwise() {
        assert_eq!(render(&[]), "");
        let text = render(&[(
            "dev/A.md".to_owned(),
            Verdict::Unreviewed { size: 1, limit: 0 },
        )]);
        assert!(text.starts_with("dev/A.md:"));
    }
}
