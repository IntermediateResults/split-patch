use clap::Parser;

/// Split the given patchfile(s) into new files
///
/// So that each new file only contains the part of the patch for
/// one particular target file.
#[derive(Debug, Parser)]
#[command(version, about, long_about)]
struct Args {
    /// Split on hunk boundaries, too.
    #[arg(long)]
    hunks: bool,

    /// Split on individual change groups, too (implies --hunks)
    #[arg(short, long)]
    changes: bool,

    /// Do not print the generated files.
    #[arg(short, long)]
    quiet: bool,
}

fn main() {
    let mut args = Args::parse();

    if args.changes {
        args.hunks = true;
    }

    dbg!(args);
}
