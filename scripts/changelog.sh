#!/usr/bin/env bash
# Builds CHANGELOG.md from changelog/: the introduction (changelog/README.md up to its
# marker line), "## Не выпущено" with changelog/unreleased.md, then every changelog/X.Y.Z.md,
# newest first. `--check` builds it aside and fails if CHANGELOG.md differs.
set -euo pipefail
cd "$(dirname "$0")/.."

build() {
    sed '/^<!-- Выше — вступление CHANGELOG.md/,$d' changelog/README.md
    printf '## Не выпущено\n'
    cat changelog/unreleased.md
    for v in $(ls changelog | sed -n 's/^\([0-9]*\.[0-9]*\.[0-9]*\)\.md$/\1/p' | sort -t. -k1,1nr -k2,2nr -k3,3nr); do
        cat "changelog/$v.md"
    done
}

if [ "${1:-}" = "--check" ]; then
    if ! build | cmp -s - CHANGELOG.md; then
        echo "CHANGELOG.md is out of date: run scripts/changelog.sh" >&2
        exit 1
    fi
else
    build > CHANGELOG.md.new
    mv CHANGELOG.md.new CHANGELOG.md
fi
