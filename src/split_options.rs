use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone, clap::Parser)]
#[command(allow_hyphen_values = true)]
pub struct SplitOptions {
    /// Split on hunk boundaries, not just file boundaries.
    #[clap(long)]
    pub hunks: bool,

    /// Split on individual change groups, too (implies `--hunks`)
    #[clap(short, long)]
    pub changes: bool,

    /// Omit the addition of a prefix to the subject line of patch
    /// files that have a git style patch header
    #[clap(long)]
    pub no_subject_change: bool,

    /// When using `--changes`, use a single number counter for
    /// generating the ids for the generated output file names and
    /// subject prefixes instead of `{hunk_id}-{change_id}`.
    #[clap(long)]
    pub monotonous_numbers: bool,

    /// Path to the directory where to write the split files
    /// to. Default: the same directory as the input file.
    #[clap(long)]
    pub output_dir: Option<PathBuf>,

    /// Do not insert the prefix after "[PATCH]", but before
    /// everything.
    #[clap(long)]
    pub no_insert_after_patch: bool,
}

impl SplitOptions {
    /// Whether to split on hunks. Is true even if the user (only)
    /// specified `--changes` (which implies to split on hunks, too)
    pub fn hunks(&self) -> bool {
        self.changes || self.hunks
    }
}

impl Default for SplitOptions {
    fn default() -> Self {
        SplitOptions::parse_from(["ignored-program-name"])
    }
}

#[test]
fn t_config_default_split_options() {
    let d = SplitOptions::default();
    assert_eq!(d.no_insert_after_patch, false);
}
