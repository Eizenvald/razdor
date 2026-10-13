#!/usr/bin/env python3
"""PreToolUse guard for Bash commands in the Razdor repo.

Reads the hook's JSON on stdin and exits 2 (with the reason on stderr) to block a command:

- cargo build/test/run/clippy/check/bench not pinned with `taskset -c 16-19`
  (the other cores belong to the owner and other sessions);
- the game started in a window (`target/*/razdor`, `cargo run`, `Razdor.exe`, `wine …Razdor`)
  unless under `xvfb-run`, with `DISPLAY` set to another (Xvfb) display in the same command,
  or with `RAZDOR_ALLOW_WINDOW=1` (the owner asked for a visible run);
- `git add` / `git commit` that would put the game's content in the repo (.lit .ugs .dtm .dtc
  .wav .ogg .mp3 .bik .rzsave, Rus_*.ini, _Global.ini);
- pushes to the frozen dt-feat, force pushes, `--no-verify`;
- recursive `rm` outside the scratch folders (/tmp/claude-*, /mnt/ai_models/razdor-scratch).

Commands are parsed, not grepped: `git push` inside a commit message or an `echo` is not a push;
`bash -c` / `sh -c` / `eval` strings are parsed too. The hook fails open on its own errors,
except that a command whose text pushes to dt-feat is blocked when it cannot be parsed.
Cases: guard-bash.cases.tsv, run by test-guard.sh.
"""

import json
import os
import re
import shlex
import subprocess
import sys

ALLOWED_CPUS = set(range(16, 20))
PIN_HINT = "CARGO_BUILD_JOBS=4 taskset -c 16-19 cargo …"
FROZEN = {"dt-feat"}
GAME_EXT = re.compile(r"\.(lit|ugs|dtm|dtc|wav|ogg|mp3|bik|rzsave)$", re.I)
GAME_INI = re.compile(r"(^|/)(rus_[^/]*|_global)\.ini$", re.I)
SCRATCH = ("/tmp/claude-", "/mnt/ai_models/razdor-scratch")
CARGO_PINNED = {"build", "b", "test", "t", "run", "r", "clippy", "check", "c", "bench"}
OPS = {";", "&&", "||", "|", "&", "|&", ";;", "(", ")", "\n"}


class Block(Exception):
    pass


def strip_heredocs(text):
    """Drops heredoc bodies: their lines are data, not commands."""
    out, lines, i = [], text.split("\n"), 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        tags = re.findall(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1", line)
        i += 1
        for _, tag in tags:
            while i < len(lines) and lines[i].strip() != tag:
                i += 1
            i += 1  # the terminator
    return "\n".join(out)


def segments(text):
    """Splits a command line into simple commands (lists of words)."""
    lex = shlex.shlex(strip_heredocs(text).replace("\\\n", " "), posix=True, punctuation_chars=";&|()\n")
    lex.whitespace = " \t\r"
    lex.whitespace_split = True
    lex.commenters = "#"
    seg, segs = [], []
    for tok in lex:
        if tok in OPS or (tok and set(tok) <= set(";&|()\n")):
            if seg:
                segs.append(seg)
            seg = []
        else:
            seg.append(tok)
    if seg:
        segs.append(seg)
    return segs


ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


def cpus(spec):
    out = set()
    for part in spec.split(","):
        if "-" in part:
            a, b = part.split("-", 1)
            out |= set(range(int(a), int(b) + 1))
        else:
            out.add(int(part))
    return out


def unwrap(words):
    """Strips assignments and wrappers; returns (argv, env, pinned, xvfb)."""
    env, pinned, xvfb = {}, False, False
    w = list(words)
    while w:
        head = os.path.basename(w[0])
        if ASSIGN.match(w[0]):
            k, v = w[0].split("=", 1)
            env[k] = v
            w = w[1:]
        elif head == "env":
            w = w[1:]
            while w and (w[0].startswith("-") or ASSIGN.match(w[0])):
                if ASSIGN.match(w[0]):
                    k, v = w[0].split("=", 1)
                    env[k] = v
                elif w[0] in ("-u", "-C", "--unset", "--chdir") and len(w) > 1:
                    w = w[1:]
                w = w[1:]
        elif head == "taskset":
            w = w[1:]
            spec = None
            while w and w[0].startswith("-"):
                flag = w[0]
                w = w[1:]
                if flag in ("-c", "--cpu-list") and w:
                    spec = w[0]
                    w = w[1:]
                elif flag.startswith("--cpu-list="):
                    spec = flag.split("=", 1)[1]
            if spec is None and w:
                w = w[1:]  # a hex mask: not the E-cores by list
            try:
                pinned = spec is not None and cpus(spec) <= ALLOWED_CPUS
            except ValueError:
                pinned = False
        elif head in ("xvfb-run",):
            xvfb = True
            w = w[1:]
            while w and w[0].startswith("-"):
                flag = w[0]
                w = w[1:]
                if flag in ("-s", "-n", "-e", "-f", "-p", "--server-args", "--server-num", "--error-file", "--auth-file", "--xauth-protocol") and w:
                    w = w[1:]
        elif head in ("sudo", "nohup", "setsid", "command", "exec", "time", "builtin"):
            w = w[1:]
            while w and w[0].startswith("-"):
                w = w[1:]
        elif head in ("nice", "ionice", "stdbuf", "timeout", "chrt"):
            w = w[1:]
            while w and w[0].startswith("-"):
                flag = w[0]
                w = w[1:]
                if flag in ("-n", "-c", "-k", "-s", "-o", "-e", "-i", "--signal", "--kill-after") and w:
                    w = w[1:]
            if head in ("timeout", "chrt") and w:
                w = w[1:]  # the duration / priority
        elif head in ("perf", "strace", "ltrace", "valgrind"):
            if "--" in w:
                w = w[w.index("--") + 1:]
                continue
            w = w[1:]
            if head == "perf" and w and w[0] in ("record", "stat", "trace"):
                w = w[1:]
            elif head == "perf":
                return [], env, pinned, xvfb
            takes = {"-F", "-o", "-e", "-c", "-p", "-t", "-C", "-m", "-s", "-O", "-u", "-E", "-a", "--call-graph", "--freq", "--output", "--event"}
            while w and w[0].startswith("-"):
                flag = w[0]
                w = w[1:]
                if flag in takes and w:
                    w = w[1:]
        else:
            break
    return w, env, pinned, xvfb


def git_out(args, cwd):
    try:
        r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, timeout=10)
        return r.stdout if r.returncode == 0 else ""
    except Exception:
        return ""


def is_game_content(path):
    return bool(GAME_EXT.search(path) or GAME_INI.search(path))


def check_paths(paths, what):
    bad = sorted({p for p in paths if p and is_game_content(p)})
    if bad:
        raise Block(f"{what} would put the game's content in the repo ({', '.join(bad[:5])}). Razdor is public: never commit the original's files, pictures, sounds, maps, ini files or saves.")


def check_git(argv, cwd):
    w = argv[1:]
    while w and w[0].startswith("-"):
        flag = w[0]
        w = w[1:]
        if flag in ("-C", "-c", "--git-dir", "--work-tree", "--namespace") and w:
            if flag == "-C":
                cwd = os.path.join(cwd, w[0])
            w = w[1:]
    if not w:
        return
    sub, args = w[0], w[1:]
    if "--no-verify" in args or (sub == "commit" and "-n" in args):
        raise Block("--no-verify skips the repo's git hooks; fix what they report instead.")
    if sub == "push":
        check_push(args, cwd)
    elif sub == "add":
        flags = [a for a in args if a.startswith("-")]
        paths = [a for a in args if not a.startswith("-") and a != "--"]
        check_paths(paths, "git add")
        spec = [] if any(f in ("-A", "--all") for f in flags) else paths
        if any(f in ("-A", "--all", "-u", "--update") for f in flags) or paths:
            out = git_out(["status", "--porcelain=v1", "--untracked-files=all", "--", *spec], cwd)
            check_paths([line[3:].split(" -> ")[-1].strip('"') for line in out.splitlines()], "git add")
    elif sub == "commit":
        staged = git_out(["diff", "--cached", "--name-only"], cwd).splitlines()
        if any(commits_all(a) for a in args):
            staged += git_out(["diff", "--name-only"], cwd).splitlines()
        check_paths(staged, "git commit")


def commits_all(arg):
    """`-a` / `--all`, alone or in a cluster like `-am` (letters after a value flag are its value)."""
    if arg in ("-a", "--all"):
        return True
    if not arg.startswith("-") or arg.startswith("--"):
        return False
    for ch in arg[1:]:
        if ch == "a":
            return True
        if ch in "mFCctS":
            return False
    return False


def current_branch(cwd):
    return git_out(["symbolic-ref", "--short", "-q", "HEAD"], cwd).strip()


def resolve_dest(ref, cwd):
    if ref in ("HEAD", "@", ""):
        return current_branch(cwd)
    full = git_out(["rev-parse", "--symbolic-full-name", ref], cwd).strip()
    if full.startswith("refs/heads/"):
        return full[len("refs/heads/"):]
    return ref[len("refs/heads/"):] if ref.startswith("refs/heads/") else ref


def check_push(args, cwd):
    for a in args:
        if a in ("-f", "--force", "--mirror", "--all") or a.startswith("--force-with-lease") or a.startswith("--force-if-includes") or (a.startswith("-") and not a.startswith("--") and "f" in a[1:]):
            raise Block(f"git push {a}: force, mirror and all-branch pushes are not allowed; push the branch normally.")
    takes = {"-o", "--push-option", "--repo", "--receive-pack", "--exec"}
    pos, skip = [], False
    for a in args:
        if skip:
            skip = False
            continue
        if a in takes:
            skip = True
        elif not a.startswith("-"):
            pos.append(a)
    refspecs = pos[1:]
    if not refspecs:
        dests = [current_branch(cwd)]
    else:
        dests = []
        for spec in refspecs:
            if spec.startswith("+"):
                raise Block(f"git push {spec}: a '+' refspec is a force push.")
            src, _, dst = spec.partition(":")
            dests.append(resolve_dest(dst if ":" in spec else src, cwd))
    for d in dests:
        if d in FROZEN:
            raise Block(f"dt-feat is frozen: nothing is pushed to it. Work goes to main (parity fixes also cherry-picked to dt-original).")


def check_rm(argv, cwd):
    flags = "".join(a[1:] for a in argv[1:] if a.startswith("-") and not a.startswith("--"))
    recursive = "r" in flags or "R" in flags or "--recursive" in argv
    if not recursive:
        return
    targets = [a for a in argv[1:] if not a.startswith("-")]
    for t in targets:
        if "$" in t or "*" in t or "?" in t or t.startswith("~"):
            p = t
        else:
            p = os.path.normpath(os.path.join(cwd, t))
        if not p.startswith(SCRATCH):
            raise Block(f"rm -r {t}: recursive deletes only inside /tmp/claude-* or /mnt/ai_models/razdor-scratch. Ask the owner (or queue it for them) before deleting anything else.")


def launches_game(argv):
    if not argv:
        return False
    head = argv[0]
    base = os.path.basename(head)
    if base == "cargo":
        rest = [a for a in argv[1:] if not a.startswith("+")]
        sub = next((a for a in rest if not a.startswith("-")), "")
        if sub in ("run", "r"):
            if "--example" in rest or any(a.startswith("--example=") for a in rest):
                return False
            if "--bin" in rest:
                i = rest.index("--bin")
                return i + 1 < len(rest) and rest[i + 1] == "razdor"
            return True
        return False
    if base in ("wine", "wine64", "wine-stable"):
        return any(os.path.basename(a).lower() in ("razdor.exe", "razdor") for a in argv[1:])
    if base in ("razdor", "razdor-macos", "Razdor.exe") and ("/" in head or base == "Razdor.exe"):
        return True
    return False


def check_segment(words, cwd, exported):
    argv, env, pinned, xvfb = unwrap(words)
    if not argv:
        if env and not words[len(env):]:
            exported.update(env)  # a bare assignment holds for the rest of the line
        return
    head = os.path.basename(argv[0])
    if head == "export":
        for a in argv[1:]:
            if ASSIGN.match(a):
                k, v = a.split("=", 1)
                exported[k] = v
        return
    if head == "cd" and len(argv) > 1:
        return
    if head in ("bash", "sh", "zsh", "dash") and "-c" in argv:
        i = argv.index("-c")
        if i + 1 < len(argv):
            check_text(argv[i + 1], cwd)
        return
    if head == "eval":
        check_text(" ".join(argv[1:]), cwd)
        return
    allenv = {**exported, **env}
    if head == "cargo":
        rest = [a for a in argv[1:] if not a.startswith("+")]
        sub = next((a for a in rest if not a.startswith("-")), "")
        if sub in CARGO_PINNED and not pinned:
            raise Block(f"cargo {sub} must run on the E-cores: {PIN_HINT} (cores 0–15 are the owner's and other sessions').")
    if launches_game(argv):
        display = allenv.get("DISPLAY")
        if not (xvfb or allenv.get("RAZDOR_ALLOW_WINDOW") == "1" or (display and display not in (":0", ":0.0", os.environ.get("DISPLAY", ":0")))):
            raise Block("This starts Razdor in a window on the owner's desktop. Run it off-screen under xvfb-run (see the offscreen-snapshot skill), or add RAZDOR_ALLOW_WINDOW=1 only when the owner asked for a visible run.")
    if head == "git":
        check_git(argv, cwd)
    elif head == "rm":
        check_rm(argv, cwd)


def check_text(text, cwd):
    exported = {}
    for seg in segments(text):
        if seg and seg[0] == "cd" and len(seg) > 1 and "$" not in seg[1]:
            cwd = os.path.normpath(os.path.join(cwd, os.path.expanduser(seg[1])))
            continue
        check_segment(seg, cwd, exported)


def main():
    raw = sys.stdin.read()
    try:
        data = json.loads(raw)
        cmd = data.get("tool_input", {}).get("command", "")
        cwd = data.get("cwd") or os.getcwd()
    except Exception:
        return 0
    try:
        check_text(cmd, cwd)
    except Block as b:
        print(f"Blocked by .claude/hooks/guard-bash.py: {b}", file=sys.stderr)
        return 2
    except Exception:
        if re.search(r"\bgit\b[^\n]*\bpush\b[^\n]*dt-feat", cmd):
            print("Blocked by .claude/hooks/guard-bash.py: dt-feat is frozen (the command could not be parsed, so a push to it is refused).", file=sys.stderr)
            return 2
        return 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
