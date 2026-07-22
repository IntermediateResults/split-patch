#!/usr/bin/env sh
set -eux

# path to perl version of split-patch
split_patch_pl="/opt/chj/bin/split-patch";
# path to rust version of split-patch
split_patch_rs=...;

dir=$(mktemp -d)
# commit sha for `git format-patch`
revision_range=...

patch_dir=$dir/PATCHES
mkdir -p "$patch_dir"
rm -rf "${patch_dir:?}"/*

git format-patch -o "$patch_dir" "$revision_range"

for opt in "hunks" "changes" "quiet" ; do
    echo "Working on option: $opt"
    pl_dir="$dir/pl/$opt"
    mkdir -p "$pl_dir"
    rm -rf "${pl_dir:?}"/*
    rs_dir="$dir/rs/$opt"
    mkdir -p "$rs_dir"
    rm -rf "${rs_dir:?}"/*

    cp "$patch_dir"/* "$pl_dir"
    "$split_patch_pl" "--$opt" "$pl_dir"/*

    cp "$patch_dir"/* "$rs_dir"
    "$split_patch_rs" "--$opt" "$rs_dir"/*

    diff -ru "$pl_dir" "$rs_dir"
done
