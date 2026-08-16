# Contributing to split-patch

This document is for those who would like to contribute to split-patch.

## PR notes

Instead of mentioning the issue or pull request numbers in the commit
messages or using GitHub's merge commits, this project uses a GitHub
action to add Git notes to the commit that was merged. You can get
those notes by running the following command after cloning the
repository:

    git fetch origin refs/notes/commits:refs/notes/commits

The notes are shown in "git log" output as "Notes:" at the end of
commit messages. `gitk` shows them the same way and also shows yellow
"sticky note" markers in the commit list. Other history viewers
probably do the same.

To automatically retrieve the notes whenever you fetch from the
repository, configure the remote correspondingly via:

    git config --add remote.origin.fetch '+refs/notes/commits:refs/notes/commits'

## Clippy

A number of warnings deemed not useful enough have been configured as
"allow" in Cargo.toml.

To run clippy the same way that the CI runs it (except CI runs it in
error mode, via `make clippy_deny`):

    make clippy

In case you want to see all of the default clippy warnings,
i.e. ignore the ignores:

    CLIPPY_ARGS="-- -W clippy::all" make clippy

To have clippy fix the warnings according to the project desires:

    make clippy_fix

## Testing / CI

GitHub CI runs `make ci`. You can run this locally before
(re)submitting the PR to speed up checking. This does run `cargo fmt`
and will output the diff from the last commit.

During development, `make test` is likely what you usually want to
run. For other, more finegrained test choices, have a look at the
`Makefile`.