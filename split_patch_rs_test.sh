#!/usr/bin/env sh
set -eux

# path to perl version of split-patch
SPLIT_PATCH_PL="/opt/chj/bin/split-patch";
# path to rust version of split-patch
SPLIT_PATCH_RS=...;

DIR="/tmp"
# commit sha for `git format-patch`
SHA=...

PATCH_DIR=$DIR/PATCHES
mkdir -p "$PATCH_DIR"
rm -rf "${PATCH_DIR:?}"/*

git format-patch -o "$PATCH_DIR" "$SHA"

for opt in "hunks" "changes" "quiet" ; do
    echo "Working on option: $opt"
    pl_dir="$DIR/pl/$opt"
    mkdir -p "$pl_dir"
    rm -rf "${pl_dir:?}"/*
    rs_dir="$DIR/rs/$opt"
    mkdir -p "$rs_dir"
    rm -rf "${rs_dir:?}"/*

    cp "$PATCH_DIR"/* "$pl_dir"
    "$SPLIT_PATCH_PL" "--$opt" "$pl_dir"/*

    cp "$PATCH_DIR"/* "$rs_dir"
    "$SPLIT_PATCH_RS" "--$opt" "$rs_dir"/*

    diff -ru "$pl_dir" "$rs_dir"
done
