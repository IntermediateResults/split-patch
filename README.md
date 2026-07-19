# Split patch files on file, hunk or change boundaries

This program takes patch files in unified diff format, optionally with
a header (e.g. as produced by `git format-patch`), and splits them up
into separate patch files that contain the same header (except
optionally with a suffix added in its subject line), but only a diff
for one target file each, or one hunk for one file each, or even one
change each.

Our terminology here:

A patch: a file with a(n optional) header and one or more file diffs.

A diff: the differences for one file.

A hunk: the differences for an uninterrupted region in a file (i.e. a diff split on the `@@ ` lines).

A change: one region of `-` and/or `+` lines including associated context lines (` `).

## Usage

Install via `cargo install --locked split-patch`, or if you have
checked out the repository, `cargo install --locked --path .`.

Run `split-patch --help` for usage help.

## History

This program is a rewrite of a [version written in Perl](https://github.com/pflanze/chj-scripts/blob/a44a1c466f9704a35aa839c653f447d51e2d15df/split-patch).

