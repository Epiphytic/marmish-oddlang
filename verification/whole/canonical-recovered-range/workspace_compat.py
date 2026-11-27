"""Native range tree, closure and digest adapters for the endpoint and role gates."""
from pathlib import Path
import runpy

WHOLE = Path(__file__).resolve().parents[1]
BASE = "workspace-v7-canonical-range-data"
COMBINED = runpy.run_path(str(WHOLE / "canonical-public-data/workspace_compat.py"))
AUDIT, JOURNAL, data, sha, DARWIN, LINUX = (
    COMBINED[k] for k in ("AUDIT", "JOURNAL", "data", "digest", "DARWIN", "LINUX")
)
ASSEMBLY = COMBINED["lane"]("workspace")
tool = COMBINED["SIX"]["product_witness"]


def hashed(value):
    return tool("range-hash", current=value)


def tree(root):
    return tool("range-tree", root)


def lock_delta(old, new, path=""):
    return tool("range-lock-delta", path, current={"old": old, "new": new})


def library_closure(raw, target, host):
    return tool("range-closure", target, host, current=raw.decode() if isinstance(raw, bytes) else raw)


PRIOR = {"library_closure": library_closure}
