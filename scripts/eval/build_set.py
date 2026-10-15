# /// script
# dependencies = ["rapidfuzz"]
# ///
"""Build a fixed dictation test set from Wispr Flow's history.

Usage: uv run scripts/eval/build_set.py [OUT_DIR] [--speech N] [--seed S]

Writes OUT_DIR/clips/*.wav, OUT_DIR/set.jsonl and a frozen copy of Owl's
settings (API keys included, so the folder is private to your account).
Each speech clip carries two answer keys:
  hearing: Wispr's transcript before its AI cleanup
  final:   the text you kept (Wispr's read-back of your edit, else what it pasted)
Clips Wispr heard nothing in become "should be empty" clips.
"""
import argparse, json, os, random, re, shutil, sqlite3, wave
from pathlib import Path
from rapidfuzz.distance import Levenshtein

HOME = Path.home()
WISPR = HOME / "Library/Application Support/Wispr Flow/flow.sqlite"
OWL_SETTINGS = HOME / "Library/Application Support/com.hao.owl/settings_store.json"
# A read-back this different from what was pasted is mostly other typing in
# the same text box, not a correction.
MAX_EDIT = 0.2


def squash(s):
    return re.sub(r"\s+", " ", s or "").strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", nargs="?", default=str(HOME / "Documents/Projects/Owl-evals/dictation-v1"))
    ap.add_argument("--speech", type=int, default=150)
    ap.add_argument("--seed", type=int, default=1)
    args = ap.parse_args()
    out = Path(args.out)
    if out.exists():
        raise SystemExit(f"{out} exists; pick a new name so old results keep their set")
    (out / "clips").mkdir(parents=True)
    os.chmod(out.parent, 0o700)
    os.chmod(out, 0o700)

    db = sqlite3.connect(f"file:{WISPR}?mode=ro", uri=True)
    rows = db.execute(
        "select transcriptEntityId, timestamp, duration, status, asrText, formattedText, editedText"
        " from History where audio is not null order by timestamp"
    ).fetchall()

    speech, empty = [], []
    for id_, ts, dur, status, asr, formatted, edited in rows:
        if not squash(asr):
            # Wispr's "error" rows may have had speech it failed on.
            if status in ("raw_transcript", "dismissed"):
                empty.append(dict(id=id_, kind="empty", when=ts, secs=dur))
            continue
        if not squash(formatted):
            continue
        was_edited = bool(squash(edited)) and squash(edited) != squash(formatted)
        if was_edited and Levenshtein.normalized_distance(squash(formatted), squash(edited)) >= MAX_EDIT:
            continue
        speech.append(dict(id=id_, kind="speech", when=ts, secs=dur, hearing=squash(asr),
                           final=squash(edited if was_edited else formatted),
                           wispr_final=squash(formatted), edited=was_edited))

    random.Random(args.seed).shuffle(speech)
    picked = sorted(speech[: args.speech], key=lambda c: c["when"]) + empty

    kept = []
    for clip in picked:
        (blob,) = db.execute("select audio from History where transcriptEntityId=?", (clip["id"],)).fetchone()
        path = out / "clips" / f"{clip['id']}.wav"
        path.write_bytes(blob)
        with wave.open(str(path)) as w:
            if (w.getframerate(), w.getnchannels(), w.getsampwidth()) != (16000, 1, 2):
                path.unlink()
                continue
        clip["wav"] = f"clips/{path.name}"
        kept.append(clip)

    with open(out / "set.jsonl", "w") as f:
        for clip in kept:
            f.write(json.dumps(clip, ensure_ascii=False) + "\n")
    shutil.copy(OWL_SETTINGS, out / "settings_store.json")
    os.chmod(out / "settings_store.json", 0o600)
    n_speech = sum(c["kind"] == "speech" for c in kept)
    print(f"{out}: {n_speech} speech clips "
          f"({sum(c.get('edited', False) for c in kept)} with your edits), "
          f"{len(kept) - n_speech} empty clips, from {len(speech)} eligible")


if __name__ == "__main__":
    main()
