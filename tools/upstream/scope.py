#!/usr/bin/env python3
"""Turn a checked-out KONTRA tree into CONTRA's scope, in place.

    python3 -I tools/upstream/scope.py <tree>

Used to catch CONTRA up with KONTRA: the same transform applied to KONTRA's
old base and its new head gives a diff in CONTRA's terms (see README.md here).

- Deletes what CONTRA does not carry: other formats (UVI, SFZ), Lua, the
  website, release/CI/publishing, the old author's workflow, Python/shell
  tooling, and docs other than the Kontakt reverse-engineering evidence.
- Leaves the vendored plugin/UI frameworks and Cargo.lock out entirely:
  CONTRA vendors and patches its own copies, handled by hand.
- Renames kontra -> contra (KONTRA, Kontra, kontra) in file contents and
  paths, except evidence, license texts and NOTICE.
"""
import os
import re
import shutil
import sys

DELETE = [
    ".claude/agents", ".claude/skills/v3-review", ".claude/skills/v3-task",
    ".coderabbit.yaml", ".impeccable", ".ignore", ".github", ".cargo/config.toml",
    "website", "handoff", "audits", "release-fixes.json", "SOURCE_COMMIT",
    "CHANGELOG.md", "ROADMAP.md", "PRODUCT.md", "DESIGN.md", "CONTRIBUTING.md",
    "README.md", "CLAUDE.md", "AGENTS.md",
    "crates/format/uvi", "crates/format/sfz", "crates/format/native", "crates/script/lua",
    "vendor/uvi-file", "vendor/derpcat-flight-recorder", "vendor/buffr-durable-file",
    "licenses/BUFFR", "tools/xtask",
    # The frameworks and the lockfile: CONTRA's own vendored copies, by hand.
    "vendor/MOOSE-PATCHES.md", "vendor/moose-state-queue.rs", "vendor/moose-state-queue-tests.rs",
    "vendor/vello", "vendor/xim-rs", "Cargo.lock",
]
DELETE_PREFIX = ["vendor/moose", "vendor/mui"]
# Under docs/ and tools/, only these survive.
KEEP_DOCS = ["docs/re/", "docs/ALTERNATING_LOOPS.md", "docs/EDITOR_PREFERENCES.md", "docs/LEGAL.md",
             "docs/UI_NATIVE_PRESERVATION.md", "docs/shared-note-expression.md", "docs/KSP_"]
KEEP_TOOLS = ["tools/inspect/", "tools/ui-shots/", "tools/kontra-window/", "tools/contra-window/",
              "tools/clap-probe/", "tools/vst3-probe/", "tools/upstream/"]
NO_RENAME = ["docs/re/", "licenses/", "LICENSE", "NOTICE"]
DROP_EXT = (".py", ".sh", ".lua", ".java", ".mjs", ".js", ".cpp", ".html")


def doomed(rel):
    if rel in DELETE or any(rel.startswith(d + "/") for d in DELETE):
        return True
    if any(rel.startswith(p) for p in DELETE_PREFIX):
        return True
    if rel.startswith("docs/"):
        if not any(rel.startswith(k) for k in KEEP_DOCS):
            return True
        if re.search(r"(?i)uvi|falcon", os.path.basename(rel)) or rel.endswith(".py"):
            return True
    if rel.startswith("tools/") and not any(rel.startswith(k) for k in KEEP_TOOLS):
        return True
    if rel.startswith("tools/") and rel.endswith(DROP_EXT):
        return True
    return False


def rename_text(s):
    return s.replace("KONTRA", "CONTRA").replace("Kontra", "Contra").replace("kontra", "contra")


def main(tree):
    files = []
    for root, dirs, names in os.walk(tree):
        dirs[:] = [d for d in dirs if d != ".git"]
        for n in names:
            files.append(os.path.relpath(os.path.join(root, n), tree))
    for rel in files:
        path = os.path.join(tree, rel)
        if doomed(rel):
            os.remove(path)
            continue
        if any(rel == k or rel.startswith(k) for k in NO_RENAME):
            continue
        with open(path, "rb") as f:
            data = f.read()
        if b"\0" not in data:
            try:
                text = data.decode("utf-8")
            except UnicodeDecodeError:
                text = None
            if text is not None:
                new = rename_text(text)
                if new != text:
                    with open(path, "w", encoding="utf-8") as f:
                        f.write(new)
        new_rel = rename_text(rel)
        if new_rel != rel:
            dest = os.path.join(tree, new_rel)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            shutil.move(path, dest)
    # Remove directories left empty.
    for root, dirs, names in os.walk(tree, topdown=False):
        if ".git" in root.split(os.sep):
            continue
        if root != tree and not os.listdir(root):
            os.rmdir(root)


if __name__ == "__main__":
    main(sys.argv[1])
