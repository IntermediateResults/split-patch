#!/usr/bin/env sh
set -eux

# Path to perl version of split-patch
split_patch_pl=/opt/chj/bin/split-patch
# Path to rust version of split-patch
split_patch_rs=target/debug/split-patch

dir=$(mktemp -d)
# Revisions to make patch files for
revision_range=...

patch_dir=$dir/PATCHES
rm -rf "$patch_dir"
mkdir -p "$patch_dir"

git format-patch -o "$patch_dir" "$revision_range"

for opt in "hunks" "changes" "quiet"; do
    echo "Working on option: $opt"
    pl_dir="$dir/pl/$opt"
    rm -rf "$pl_dir"
    mkdir -p "$pl_dir"
    rs_dir="$dir/rs/$opt"
    rm -rf "$rs_dir"
    mkdir -p "$rs_dir"

    cp "$patch_dir"/* "$pl_dir"
    "$split_patch_pl" "--$opt" "$pl_dir"/*

    cp "$patch_dir"/* "$rs_dir"
    "$split_patch_rs" "--$opt" "$rs_dir"/*

    diff -ru "$pl_dir" "$rs_dir"
done
