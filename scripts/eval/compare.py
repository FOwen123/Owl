# /// script
# dependencies = []
# ///
"""Compare two `handy --eval` runs: what moved, and which items got worse.

Usage: uv run scripts/eval/compare.py SET_DIR OLD NEW [--top N]
OLD and NEW are run names (SET_DIR/results/NAME.json) or paths to them.
"""
import argparse
import json
from pathlib import Path


def load(name, set_dir):
    path = Path(name) if name.endswith(".json") else set_dir / "results" / f"{name}.json"
    return json.loads(path.read_text())


def rows(summary):
    out = {f"all.{k}": v for k, v in summary["all"].items()}
    for source, s in summary["by_source"].items():
        out.update({f"{source}.{k}": v for k, v in s.items()})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("set", type=Path)
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("--top", type=int, default=10)
    args = ap.parse_args()
    old, new = load(args.old, args.set), load(args.new, args.set)

    a, b = rows(old["summary"]), rows(new["summary"])
    print(f"| | {args.old} | {args.new} |\n|---|---|---|")
    for k in list(a) + [k for k in b if k not in a]:
        if a.get(k) != b.get(k) or k.endswith("_error"):
            print(f"| {k} | {a.get(k)} | {b.get(k)} |")

    by_id = {i["id"]: i for i in old["items"]}
    for field, label in (("hearing_edits", "heard"), ("final_edits", "output")):
        moves = []
        for item in new["items"]:
            before = by_id.get(item["id"])
            if before is None:
                continue
            moves.append((item[field][0] - before[field][0], item, before))
        worse = sorted((m for m in moves if m[0] > 0), key=lambda m: -m[0])
        better = sum(m[0] < 0 for m in moves)
        print(f"\n### {field}: {len(worse)} worse, {better} better, {len(moves) - len(worse) - better} unchanged")
        for d, item, before in worse[: args.top]:
            print(f"\n+{d} edits, {item['id']}\n  {args.old}: {before[label]}\n  {args.new}: {item[label]}")


if __name__ == "__main__":
    main()
