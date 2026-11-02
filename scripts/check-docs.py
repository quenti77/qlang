#!/usr/bin/env python3
"""Compares the documentation with what qlang does. Prints every gap, exits 1 if there is one.

    cargo build --release && scripts/check-docs.py

`QLANG` names the binary (default: target/release/qlang).
"""
import glob, json, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QLANG = os.environ.get("QLANG", os.path.join(ROOT, "target/release/qlang"))
DOCS = [os.path.join(ROOT, "README.md")] + sorted(glob.glob(os.path.join(ROOT, "docs/*.md")))
TEXT = {p: open(p, encoding="utf-8").read() for p in DOCS}
ALL = "\n".join(TEXT.values())
gaps = []


def gap(kind, what):
    gaps.append(f"{kind}: {what}")


def run(*args):
    r = subprocess.run([QLANG, *args], capture_output=True, text=True)
    return r.stdout + r.stderr


# 1. the options of every command are documented, and the options documented exist
COMMANDS = "run check serve test fmt doc lint repl dap lsp init add install update remove search migrate self-update".split()
helps = run("--help") + "".join(run("help", c) for c in COMMANDS)
for c in COMMANDS:
    for opt in sorted(set(re.findall(r"(?<![\w-])--[a-z][a-z0-9-]+", run("help", c)))):
        if opt not in ALL:
            gap("option not documented", f"{opt} (qlang {c})")
# options that belong to other programs: install.sh, cargo, cargo test
FOREIGN = {"--vscode", "--dir", "--ignored", "--profile", "--version", "--release", "--target", "--features"}
for opt in sorted(set(re.findall(r"`(--[a-z][a-z0-9-]+)", ALL))):
    if opt not in helps and opt not in FOREIGN:
        gap("option documented but unknown", opt)

# 2. environment variables
sources = "".join(open(p, encoding="utf-8").read() for p in glob.glob(os.path.join(ROOT, "crates/*/src/**/*.rs"), recursive=True))
for var in sorted(set(re.findall(r'"(QLANG_[A-Z_]+)"', sources))):
    if var not in ALL:
        gap("environment variable not documented", var)
for var in sorted(set(re.findall(r"`(QLANG_[A-Z_]+)`", ALL))):
    if var not in sources:
        gap("environment variable documented but unknown", var)

# 3. everything the prelude provides is named in the language reference
prelude = json.loads(run("doc", "--prelude"))
language = TEXT[os.path.join(ROOT, "docs/language.md")]
names = {f["name"] for f in prelude["functions"]} | {t["name"] for t in prelude["types"]} | {n["name"] for n in prelude["namespaces"]}
# helpers of the prelude that its own functions wrap (`Json.parse`, `Panic.guard`)
INTERNAL = {"JsonParser", "panic_guard"}
for name in sorted(names - INTERNAL):
    if not re.search(rf"(?<![\w]){re.escape(name)}(?![\w])", language):
        gap("prelude name missing from docs/language.md", name)

# 4. the code blocks of the language reference have no syntax error (some blocks are not code)
NOT_CODE = ('--" Distance entre deux points', "qlang serve ", "2026-", "let const fun")
blocks, cur, in_block = [], [], None
for line in language.split("\n"):
    if line.startswith("```"):
        if in_block is None:
            in_block, cur = line[3:].strip(), []
        else:
            if not in_block:
                blocks.append("\n".join(cur))
            in_block = None
    elif in_block is not None:
        cur.append(line)
tmp = os.path.join(os.environ.get("TMPDIR", "/tmp"), "check-docs-block.q")
for b in blocks:
    if b.startswith(NOT_CODE):
        continue
    open(tmp, "w", encoding="utf-8").write(b)
    for code, msg in re.findall(r"error\[([PL]\d+)\]: (.*)", run("check", tmp)):
        gap("syntax error in a code block of docs/language.md", f"{code} {msg} | {b.splitlines()[0][:60]}")
        break

# 5. relative links point to a file
for path, text in TEXT.items():
    for target in re.findall(r"\]\(([^)#\s]+)(?:#[^)]*)?\)", text):
        if re.match(r"[a-z]+:", target):
            continue
        if not os.path.exists(os.path.normpath(os.path.join(os.path.dirname(path), target))):
            gap("broken link", f"{os.path.relpath(path, ROOT)} -> {target}")

for g in gaps:
    print(g)
print(f"{len(gaps)} gap(s)")
sys.exit(1 if gaps else 0)
