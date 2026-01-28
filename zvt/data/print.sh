#!/usr/bin/env bash

for file in *.blob; do
    # Skip if no .blob files exist
    [ -e "$file" ] || { echo "No .blob files found."; exit 1; }

    echo "===== $file ====="
    hexdump -C "$file"
    echo
done
