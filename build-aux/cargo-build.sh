#!/bin/sh
# Builds the Rust binary with Cargo and copies it where Meson expects it.
# Usage: cargo-build.sh <cargo> <manifest> <target-dir> <debug|release> <output>
set -eu

cargo=$1
manifest=$2
target_dir=$3
profile=$4
output=$5

if [ "$profile" = release ]; then
	"$cargo" build --manifest-path "$manifest" --target-dir "$target_dir" --release
else
	"$cargo" build --manifest-path "$manifest" --target-dir "$target_dir"
fi

cp "$target_dir/$profile/lectern" "$output"
