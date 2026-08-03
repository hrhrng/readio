#!/bin/sh
set -eu

skill="${1:-../../skills/setup-readio/SKILL.md}"
reference="$(dirname "$skill")/references/configuration.md"

[ -f "$skill" ] || { echo "setup-readio SKILL.md is missing" >&2; exit 1; }
[ -f "$reference" ] || { echo "setup-readio configuration reference is missing" >&2; exit 1; }

characters="$(wc -m < "$skill" | tr -d ' ')"
[ "$characters" -le 10000 ] || {
    echo "setup-readio SKILL.md has $characters characters; maximum is 10000" >&2
    exit 1
}

grep -Fq 'name: setup-readio' "$skill"
grep -Fq 'apps/tui/scripts/install.ps1' "$skill"
grep -Fq 'references/configuration.md' "$skill"
grep -Fq '%USERPROFILE%\.local\bin\readio.exe' "$skill"

printf 'setup-readio skill contract ok (%s characters)\n' "$characters"
