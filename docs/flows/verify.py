"""Checks the code references in flow docs: every `path` exists, and every `Type::name`,
`name()` or `path:name` names a fn, struct, enum, trait, const or variant somewhere under src/.

    python3 -I docs/flows/verify.py docs/flows/*.md
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SRC = "\n".join(p.read_text(encoding="utf-8") for p in (ROOT / "src").rglob("*.rs"))
DEFINED = set(re.findall(r"\b(?:fn|struct|enum|trait|const|type|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)", SRC))
# Struct fields (`pub name: T`, `pub(crate) name: T`) count as items too.
DEFINED |= set(re.findall(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*[A-Z\[(&a-z]", SRC, re.M))
# Enum variants and struct fields count too: any identifier followed by `,` `{` `(` or `:` at a
# line start inside a definition. Cheap approximation: every identifier in the source.
WORDS = set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", SRC))

bad = []
for doc in sys.argv[1:]:
    text = pathlib.Path(doc).read_text(encoding="utf-8")
    for ref in re.findall(r"`([^`\n]+)`", text):
        ref = ref.strip()
        if re.fullmatch(r"(src|docs|tools|scripts|data)/[\w./-]+", ref):
            if not (ROOT / ref).exists():
                bad.append((doc, ref, "no such path"))
            continue
        m = re.fullmatch(r"(?:[\w/.]+\.rs:)?(?:[A-Z]\w*::)?([a-z_][a-z0-9_]*)(?:\(\))?", ref)
        if m and ("::" in ref or ref.endswith("()") or ".rs:" in ref or "_" in m.group(1)):
            if m.group(1) not in DEFINED:
                bad.append((doc, ref, "no such fn/item"))
            continue
        m = re.fullmatch(r"([A-Z]\w*)(?:::([A-Z]\w*))?", ref)
        if m and m.group(1) not in WORDS:
            bad.append((doc, ref, "no such type"))
for doc, ref, why in bad:
    print(f"{doc}: `{ref}`: {why}")
print("ok" if not bad else f"{len(bad)} bad reference(s)")
sys.exit(1 if bad else 0)
