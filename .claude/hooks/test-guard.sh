#!/bin/sh
# Runs guard-bash.py on every case of guard-bash.cases.tsv against fixture repos in a temp dir.
# Usage: sh .claude/hooks/test-guard.sh   (exit 1 if any case fails)
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fixture() { # name branch staged-file
    git init -q -b main "$tmp/$1"
    git -C "$tmp/$1" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
    git -C "$tmp/$1" branch dt-feat
    [ "$2" = main ] || git -C "$tmp/$1" checkout -q "$2"
    mkdir -p "$tmp/$1/$(dirname "$3")"
    echo x > "$tmp/$1/$3"
    git -C "$tmp/$1" add "$3"
}
fixture clean main src/ok.rs
fixture dirty main data/x.lit
fixture feat dt-feat src/ok.rs
mkdir -p "$tmp/clean/data" "$tmp/clean/saves" "$tmp/clean/docs"
touch "$tmp/clean/data/x.lit" "$tmp/clean/saves/backrok.rzsave" "$tmp/clean/Rus_Units.ini" "$tmp/clean/docs/_Global.ini"
pass=0 fail=0
while IFS='	' read -r expect repo cmd; do
    case "$expect" in ''|'#'*) continue ;; esac
    json=$(CMD="$cmd" CWD="$tmp/$repo" python3 -I -c 'import json,os;print(json.dumps({"tool_name":"Bash","cwd":os.environ["CWD"],"tool_input":{"command":os.environ["CMD"].replace("\\n","\n")}}))')
    printf '%s' "$json" | DISPLAY=:0 python3 -I "$here/guard-bash.py" 2>/dev/null
    code=$?
    got=allow; [ $code -eq 2 ] && got=block
    if [ "$got" = "$expect" ]; then pass=$((pass + 1)); else fail=$((fail + 1)); echo "FAIL (expected $expect, got $got): [$repo] $cmd"; fi
done < "$here/guard-bash.cases.tsv"
echo "$pass passed, $fail failed"
[ $fail -eq 0 ]
