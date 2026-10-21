# /// script
# dependencies = []
# ///
"""Download a small, fixed sample of public recordings as an Owl test set.

Usage: uv run scripts/eval/fetch_public.py [OUT_DIR] [--per-set N] [--seed S]

Sets (all open on Hugging Face, no account needed):
  cv-zh-tw   Common Voice 19 zh-TW test (Taiwanese Mandarin, CC0)
  ascend     ASCEND test, Mandarin-English mixed speech only (CC BY-SA 4.0)
  librispeech  LibriSpeech test-clean (English, CC BY 4.0)

Writes OUT_DIR/clips/*.wav (16 kHz mono 16-bit, via macOS afconvert) and
OUT_DIR/set.jsonl for `handy --eval OUT_DIR`. Each item's answer key is the
human transcript, so these score hearing, not cleanup.
"""
import argparse
import json
import random
import subprocess
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

ROWS = "https://datasets-server.huggingface.co/rows"
SETS = {
    "cv-zh-tw": ("JacobLinCool/common_voice_19_0_zh-TW", "default", "test", "sentence", None),
    "ascend": ("CAiRE/ASCEND", "main", "test", "transcription", ("language", "mixed")),
    "librispeech": ("openslr/librispeech_asr", "all", "test.clean", "text", None),
}


def rows(dataset, config, split, offset, length):
    query = urllib.parse.urlencode(
        {"dataset": dataset, "config": config, "split": split, "offset": offset, "length": length}
    )
    with urllib.request.urlopen(f"{ROWS}?{query}", timeout=60) as reply:
        return json.load(reply)


def to_wav(url, out):
    with tempfile.NamedTemporaryFile(suffix=Path(urllib.parse.urlparse(url).path).suffix) as tmp:
        with urllib.request.urlopen(url, timeout=60) as reply:
            tmp.write(reply.read())
        tmp.flush()
        subprocess.run(
            ["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", tmp.name, str(out)],
            check=True,
            capture_output=True,
        )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", nargs="?", default=str(Path.home() / "Documents/Projects/Owl-evals/public-v1"))
    ap.add_argument("--per-set", type=int, default=100)
    ap.add_argument("--seed", type=int, default=1)
    args = ap.parse_args()
    out = Path(args.out)
    if out.exists():
        raise SystemExit(f"{out} exists; pick a new name so old results keep their set")
    (out / "clips").mkdir(parents=True)
    rng = random.Random(args.seed)
    lines = []
    for name, (dataset, config, split, field, keep) in SETS.items():
        total = rows(dataset, config, split, 0, 1)["num_rows_total"]
        picked = 0
        # Pages of 100 from random places until the sample is full.
        for offset in rng.sample(range(0, total, 100), k=min(20, (total + 99) // 100)):
            page = rows(dataset, config, split, offset, 100)["rows"]
            rng.shuffle(page)
            for r in page:
                row = r["row"]
                if keep and row.get(keep[0]) != keep[1]:
                    continue
                text = (row.get(field) or "").strip()
                audio = (row.get("audio") or [{}])[0].get("src")
                if not text or not audio:
                    continue
                item_id = f"{name}-{r['row_idx']}"
                wav = out / "clips" / f"{item_id}.wav"
                try:
                    to_wav(audio, wav)
                except Exception as e:  # one bad file shouldn't stop the set
                    print(f"skip {item_id}: {e}")
                    continue
                lines.append(
                    {
                        "id": item_id,
                        "kind": "speech",
                        "wav": f"clips/{wav.name}",
                        "hearing": text,
                        "source": f"public:{name}",
                    }
                )
                picked += 1
                if picked >= args.per_set:
                    break
            if picked >= args.per_set:
                break
        print(f"{name}: {picked}")
    (out / "set.jsonl").write_text("".join(json.dumps(l, ensure_ascii=False) + "\n" for l in lines))
    print(f"{len(lines)} clips in {out}")


if __name__ == "__main__":
    main()
