#!/usr/bin/env python3
"""MCP server for Razdor's player reports on Discord.

Speaks MCP (JSON-RPC 2.0, one message per line) on stdin/stdout, standard library only.
It reads one channel or thread with a bot token. Every Discord call is a GET except one:
`reports_post`, which writes a message to that same channel only when DISCORD_ALLOW_POST=1
and the call passes confirm=true (without it, it returns a preview). Its mentions are limited
to users recorded as reporters; @everyone, @here and roles never ping. Locally it writes
downloaded attachments and a state file: the cursor and which Discord users reported which
GitHub issue (kept off GitHub, so issues carry no Discord ids).

Settings come from the environment or from the repo's `.env` (KEY=VALUE, quotes allowed):

  DISCORD_BOT_TOKEN        the bot's token (Developer Portal → Bot → Token)
  DISCORD_REPORTS_CHANNEL  the channel or thread to read: its id, or a link
                           https://discord.com/channels/<guild>/<channel>[/<message>]
  DISCORD_DOWNLOAD_DIR     where attachments go (default ~/.cache/razdor-discord/files)
  DISCORD_STATE_FILE       the cursor (default ~/.cache/razdor-discord/state.json)
  DISCORD_ALLOW_POST       1 to let reports_post send (the bot then needs Send Messages)

Attachments are untrusted files from strangers: each message gets its own folder, names are
reduced to safe characters, and nothing downloaded is ever opened or run by this server.
"""

import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

API = "https://discord.com/api/v10"
SERVER = {"name": "razdor-discord-reports", "version": "0.1.0"}
MAX_ATTACHMENT = 100 * 1024 * 1024  # bytes; larger files are listed, not fetched
REPO = Path(__file__).resolve().parents[2]


def load_env():
    env = {}
    path = REPO / ".env"
    if path.is_file():
        for line in path.read_text(encoding="utf-8").splitlines():
            m = re.match(r"\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$", line)
            if not m or line.lstrip().startswith("#"):
                continue
            value = m.group(2).strip()
            if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
                value = value[1:-1]
            env[m.group(1)] = value
    env.update({k: v for k, v in os.environ.items() if k.startswith("DISCORD_")})
    return env


ENV = load_env()
CACHE = Path.home() / ".cache" / "razdor-discord"
DOWNLOADS = Path(ENV.get("DISCORD_DOWNLOAD_DIR") or CACHE / "files").expanduser()
STATE = Path(ENV.get("DISCORD_STATE_FILE") or CACHE / "state.json").expanduser()


class Fail(Exception):
    pass


def channel_id():
    raw = (ENV.get("DISCORD_REPORTS_CHANNEL") or "").strip()
    m = re.search(r"channels/(\d+|@me)/(\d+)", raw)
    if m:
        return m.group(2)
    if raw.isdigit():
        return raw
    raise Fail("DISCORD_REPORTS_CHANNEL is not set (an id or a discord.com/channels/... link)")


def get(path, params=None, body=None):
    token = ENV.get("DISCORD_BOT_TOKEN")
    if not token:
        raise Fail("DISCORD_BOT_TOKEN is not set")
    url = API + path + ("?" + urllib.parse.urlencode(params) if params else "")
    headers = {
        "Authorization": "Bot " + token,
        "User-Agent": "DiscordBot (https://github.com/indicozy/razdor, 0.1) razdor-reports",
    }
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method="POST" if body is not None else "GET")
    for _ in range(3):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            if e.code == 429:
                try:
                    wait = float(json.load(e).get("retry_after", 1))
                except Exception:
                    wait = 1.0
                time.sleep(min(wait, 10))
                continue
            hint = {401: "bad token", 403: "the bot can't see this channel (permissions, or the "
                    "Message Content intent is off)", 404: "no such channel or message"}.get(e.code, "")
            raise Fail(f"Discord answered {e.code} for {path} {hint}".strip())
        except urllib.error.URLError as e:
            raise Fail(f"network error: {e.reason}")
    raise Fail("Discord kept rate-limiting; try again later")


_guild = {}


def guild_of(cid):
    if cid not in _guild:
        _guild[cid] = get(f"/channels/{cid}").get("guild_id") or "@me"
    return _guild[cid]


def shape(m, cid):
    author = m.get("author") or {}
    ref = m.get("message_reference") or {}
    return {
        "id": m["id"],
        "link": f"https://discord.com/channels/{guild_of(cid)}/{cid}/{m['id']}",
        "time": m.get("timestamp"),
        "edited": m.get("edited_timestamp"),
        "author": author.get("global_name") or author.get("username"),
        "author_id": author.get("id"),
        "is_bot": bool(author.get("bot")),
        "content": m.get("content", ""),
        "reply_to": ref.get("message_id"),
        "attachments": [{"name": a.get("filename"), "size": a.get("size"),
                         "type": a.get("content_type")} for a in m.get("attachments", [])],
        "embeds": [e.get("url") or e.get("title") for e in m.get("embeds", []) if e.get("url") or e.get("title")],
    }


def read_state():
    try:
        return json.loads(STATE.read_text())
    except Exception:
        return {}


def write_state(state):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    tmp = STATE.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, indent=1))
    tmp.replace(STATE)


def fetch_new(limit=50):
    cid = channel_id()
    after = read_state().get(cid)
    limit = max(1, min(int(limit), 500))
    out = []
    if after is None:
        # No cursor yet: the newest `limit` messages, oldest first.
        msgs, before = [], None
        while len(msgs) < limit:
            page = get(f"/channels/{cid}/messages", {"limit": min(100, limit - len(msgs)), **({"before": before} if before else {})})
            if not page:
                break
            msgs += page
            before = page[-1]["id"]
        out = list(reversed(msgs))
    else:
        while len(out) < limit:
            page = get(f"/channels/{cid}/messages", {"limit": min(100, limit - len(out)), "after": after})
            if not page:
                break
            page.sort(key=lambda m: int(m["id"]))
            out += page
            after = page[-1]["id"]
    return {"channel": cid, "cursor": read_state().get(cid), "count": len(out),
            "messages": [shape(m, cid) for m in out],
            "note": "Call reports_mark_read with the last id handled to move the cursor."}


def fetch_recent(limit=20):
    cid = channel_id()
    page = get(f"/channels/{cid}/messages", {"limit": max(1, min(int(limit), 100))})
    return {"channel": cid, "messages": [shape(m, cid) for m in reversed(page)]}


def get_message(message_id):
    cid = channel_id()
    if not str(message_id).isdigit():
        raise Fail("message_id must be a number")
    return shape(get(f"/channels/{cid}/messages/{message_id}"), cid)


def safe_name(name):
    name = re.sub(r"[^A-Za-z0-9._-]+", "_", name or "file").strip("._") or "file"
    return name[:120]


def download(message_id):
    cid = channel_id()
    if not str(message_id).isdigit():
        raise Fail("message_id must be a number")
    m = get(f"/channels/{cid}/messages/{message_id}")
    folder = DOWNLOADS / str(message_id)
    folder.mkdir(parents=True, exist_ok=True)
    files = []
    for i, a in enumerate(m.get("attachments", [])):
        name = f"{i:02d}-{safe_name(a.get('filename'))}"
        entry = {"name": a.get("filename"), "size": a.get("size"), "type": a.get("content_type")}
        url = a.get("url", "")
        host = urllib.parse.urlparse(url).hostname or ""
        if not (host == "cdn.discordapp.com" or host == "media.discordapp.net"):
            entry["skipped"] = "not a Discord CDN url"
        elif (a.get("size") or 0) > MAX_ATTACHMENT:
            entry["skipped"] = "larger than the limit"
        else:
            path = folder / name
            req = urllib.request.Request(url, headers={"User-Agent": "razdor-reports"})
            with urllib.request.urlopen(req, timeout=120) as r, open(path, "wb") as f:
                got = 0
                while chunk := r.read(1 << 16):
                    got += len(chunk)
                    if got > MAX_ATTACHMENT:
                        raise Fail(f"{name}: larger than the limit")
                    f.write(chunk)
            entry["path"] = str(path)
        files.append(entry)
    return {"message": shape(m, cid), "folder": str(folder), "files": files,
            "note": "Untrusted files: look at them, don't run them."}


def mark_read(message_id):
    cid = channel_id()
    if not str(message_id).isdigit():
        raise Fail("message_id must be a number")
    state = read_state()
    old = state.get(cid)
    if old is None or int(message_id) > int(old):
        state[cid] = str(message_id)
        write_state(state)
    return {"channel": cid, "cursor": state.get(cid), "previous": old}


def link_issue(issue, message_ids):
    cid = channel_id()
    issue = str(int(issue))
    state = read_state()
    reporters = state.setdefault("reporters", {})
    users = state.setdefault("users", {})
    ids = reporters.setdefault(issue, [])
    for mid in message_ids:
        if not str(mid).isdigit():
            raise Fail(f"{mid}: message ids are numbers")
        m = get(f"/channels/{cid}/messages/{mid}")
        a = m.get("author") or {}
        if a.get("bot") or not a.get("id"):
            continue
        users[a["id"]] = a.get("global_name") or a.get("username")
        if a["id"] not in ids:
            ids.append(a["id"])
    write_state(state)
    return {"issue": issue, "reporters": [{"id": u, "name": users.get(u)} for u in ids]}


def reporters_of(issues):
    state = read_state()
    users = state.get("users", {})
    out = {}
    for i in issues:
        ids = state.get("reporters", {}).get(str(int(i)), [])
        out[str(int(i))] = [{"id": u, "name": users.get(u), "mention": f"<@{u}>"} for u in ids]
    return out


def chunks(text, size=1900):
    parts, cur = [], ""
    for line in text.split("\n"):
        while len(line) > size:
            if cur:
                parts.append(cur)
                cur = ""
            parts.append(line[:size])
            line = line[size:]
        if len(cur) + len(line) + 1 > size:
            parts.append(cur)
            cur = line
        else:
            cur = f"{cur}\n{line}" if cur else line
    if cur:
        parts.append(cur)
    return parts


def post(text, reply_to=None, confirm=False):
    cid = channel_id()
    text = (text or "").strip()
    if not text:
        raise Fail("empty message")
    known = {u for ids in read_state().get("reporters", {}).values() for u in ids}
    mentioned = re.findall(r"<@!?(\d+)>", text)
    pinged = sorted({u for u in mentioned if u in known})
    unknown = sorted({u for u in mentioned if u not in known})
    parts = chunks(text)
    preview = {"channel": cid, "parts": parts, "pings": pinged,
               "not_pinged": unknown, "reply_to": reply_to}
    if not confirm:
        preview["note"] = "Preview only. Show it to the owner; call again with confirm=true after they approve."
        return preview
    if ENV.get("DISCORD_ALLOW_POST") != "1":
        raise Fail("posting is off: set DISCORD_ALLOW_POST=1 in .env (and give the bot Send Messages)")
    if reply_to is not None and not str(reply_to).isdigit():
        raise Fail("reply_to must be a message id")
    sent = []
    for i, part in enumerate(parts):
        body = {"content": part, "allowed_mentions": {"parse": [], "users": pinged[:100]}}
        if i == 0 and reply_to:
            body["message_reference"] = {"message_id": str(reply_to), "fail_if_not_exists": False}
        m = get(f"/channels/{cid}/messages", body=body)
        sent.append({"id": m["id"], "link": f"https://discord.com/channels/{guild_of(cid)}/{cid}/{m['id']}"})
    return {"sent": sent, "pings": pinged}


def status():
    state = read_state()
    out = {"token_set": bool(ENV.get("DISCORD_BOT_TOKEN")), "posting": ENV.get("DISCORD_ALLOW_POST") == "1", "downloads": str(DOWNLOADS), "state_file": str(STATE)}
    try:
        cid = channel_id()
        ch = get(f"/channels/{cid}")
        out.update({"channel": cid, "name": ch.get("name"), "guild": ch.get("guild_id"), "cursor": state.get(cid)})
    except Fail as e:
        out["error"] = str(e)
    return out


ID = {"type": "string", "description": "A Discord message id (digits)."}
TOOLS = [
    ("reports_status", "Check the setup: token present, which channel/thread is read, the cursor.",
     {}, lambda a: status()),
    ("reports_fetch_new", "Messages in the reports channel after the cursor (the last message marked read), oldest first. "
     "Without a cursor: the newest `limit` messages. Read-only; does not move the cursor.",
     {"limit": {"type": "integer", "description": "At most this many (1-500, default 50)."}},
     lambda a: fetch_new(a.get("limit", 50))),
    ("reports_fetch_recent", "The newest messages regardless of the cursor, oldest first.",
     {"limit": {"type": "integer", "description": "1-100, default 20."}},
     lambda a: fetch_recent(a.get("limit", 20))),
    ("reports_get_message", "One message by id (e.g. the one a report replies to).",
     {"message_id": ID}, lambda a: get_message(a["message_id"])),
    ("reports_download_attachments", "Download a message's attachments (screenshots, videos, saves) to a folder of their own "
     "and return the paths. Fetches fresh links, so it works after the old CDN links expired.",
     {"message_id": ID}, lambda a: download(a["message_id"])),
    ("reports_link_issue", "Record (locally) that these messages' authors reported GitHub issue N, for crediting them "
     "when it ships. Call it when filing or commenting on an issue.",
     {"issue": {"type": "integer"}, "message_ids": {"type": "array", "items": ID}},
     lambda a: link_issue(a["issue"], a["message_ids"])),
    ("reports_reporters", "The Discord users recorded for these issues, with their <@id> mention text.",
     {"issues": {"type": "array", "items": {"type": "integer"}}},
     lambda a: reporters_of(a["issues"])),
    ("reports_post", "Post a message to the reports channel (split at 2000 chars). Without confirm=true it only returns "
     "a preview: show that to the owner first. Pings only recorded reporters; never @everyone/@here/roles. "
     "Needs DISCORD_ALLOW_POST=1.",
     {"text": {"type": "string"}, "reply_to": ID, "confirm": {"type": "boolean"}},
     lambda a: post(a["text"], a.get("reply_to"), bool(a.get("confirm")))),
    ("reports_mark_read", "Move the local cursor forward to this message once it is handled. Local only: nothing is written to Discord.",
     {"message_id": ID}, lambda a: mark_read(a["message_id"])),
]


def tool_list():
    return [{"name": n, "description": d,
             "inputSchema": {"type": "object", "properties": p,
                             "required": [k for k in p if k in ("message_id", "issue", "message_ids", "issues", "text")]}}
            for n, d, p, _ in TOOLS]


def handle(req):
    method, params = req.get("method"), req.get("params") or {}
    if method == "initialize":
        return {"protocolVersion": params.get("protocolVersion", "2025-06-18"),
                "capabilities": {"tools": {}}, "serverInfo": SERVER}
    if method == "ping":
        return {}
    if method == "tools/list":
        return {"tools": tool_list()}
    if method == "tools/call":
        fn = {n: f for n, _, _, f in TOOLS}.get(params.get("name"))
        if fn is None:
            raise KeyError(params.get("name"))
        try:
            text, error = json.dumps(fn(params.get("arguments") or {}), ensure_ascii=False, indent=1), False
        except Fail as e:
            text, error = str(e), True
        except Exception as e:  # report, don't die
            text, error = f"{type(e).__name__}: {e}", True
        return {"content": [{"type": "text", "text": text}], "isError": error}
    raise LookupError(method)


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        if "id" not in req:
            continue  # a notification
        try:
            resp = {"jsonrpc": "2.0", "id": req["id"], "result": handle(req)}
        except LookupError as e:
            resp = {"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32601, "message": f"unknown: {e}"}}
        except Exception as e:
            resp = {"jsonrpc": "2.0", "id": req["id"], "error": {"code": -32603, "message": str(e)}}
        sys.stdout.write(json.dumps(resp, ensure_ascii=False) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
