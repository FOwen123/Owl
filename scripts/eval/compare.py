# /// script
# dependencies = ["rapidfuzz", "opencc"]
# ///
"""Compare two eval runs: what moved, and which clips got worse.

Usage: uv run scripts/eval/compare.py OLD NEW [--set DIR] [--top N]
OLD and NEW are run names (or paths to results/*.json).
"""
import argparse, json, sys
from pathlib import Path
from rapidfuzz.distance import Levenshtein

sys.path.insert(0, str(Path(__file__).parent))
from run import HOME, tokens  # noqa: E402


def load(name, set_dir):
    path = Path(name) if name.endswith(".json") else set_dir / "results" / f"{name}.json"
    return json.loads(path.read_text())


def flat(summary):
    out = {}
    for k, v in summary.items():
        if isinstance(v, dict):
            out.update({f"{k}.{kk}": vv for kk, vv in v.items()})
        elif isinstance(v, list):
            out[f"{k} (clips)"] = len(v)
        else:
            out[k] = v
    return out


def errors(c, field, key):
    if c["kind"] == "empty":
        return len(tokens(c.get(field)))  # anything heard in silence is made up
    return Levenshtein.distance(tokens(c.get(field)), tokens(c[key]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("--set", default=str(HOME / "Documents/Projects/Owl-evals/dictation-v1"))
    ap.add_argument("--top", type=int, default=10)
    args = ap.parse_args()
    set_dir = Path(args.set)
    old, new = load(args.old, set_dir), load(args.new, set_dir)

    a, b = flat(old["summary"]), flat(new["summary"])
    print(f"| | {args.old} | {args.new} |\n|---|---|---|")
    for k in list(a) + [k for k in b if k not in a]:
        if a.get(k) != b.get(k) or k in ("hearing_error", "final_error"):
            print(f"| {k} | {a.get(k)} | {b.get(k)} |")

    by_id = {c["id"]: c for c in old["clips"]}
    for field, key in (("text", "hearing"), ("post_processed", "final")):
        moves = []
        for c in new["clips"]:
            o = by_id.get(c["id"])
            if o is None or o.get(field) is None or c.get(field) is None:
                continue
            d = errors(c, field, key) - errors(o, field, key)
            moves.append((d, c, o))
        if not moves:
            continue
        worse = sorted((m for m in moves if m[0] > 0), key=lambda m: -m[0])
        better = sum(m[0] < 0 for m in moves)
        print(f"\n### {field}: {len(worse)} clips worse, {better} better, "
              f"{len(moves) - len(worse) - better} unchanged")
        for d, c, o in worse[: args.top]:
            ref = c.get(key, "(should be empty)")
            print(f"\n+{d} errors, {c['id'][:8]}\n  expected: {ref}\n  {args.old}: {o[field]}\n  {args.new}: {c[field]}")


if __name__ == "__main__":
    main()
