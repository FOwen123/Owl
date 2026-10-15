# /// script
# dependencies = ["rapidfuzz", "opencc"]
# ///
"""Run an Owl build over a dictation test set and score it.

Usage: uv run scripts/eval/run.py BINARY NAME [--set DIR] [--model ID] [--no-ai] [--fresh-ai]

BINARY is an Owl executable (src-tauri/target/debug/handy, or Owl.app's).
Results go to SET/results/NAME.json. The build runs in a throwaway home
folder holding the set's frozen settings, so your live settings never move
the score and are never touched.
"""
import argparse, hashlib, json, os, re, shutil, subprocess, tempfile, time
from pathlib import Path
from opencc import OpenCC
from rapidfuzz.distance import Levenshtein

HOME = Path.home()
T2S, S2T = OpenCC("t2s"), OpenCC("s2t")
TOKEN = re.compile(r"[㐀-鿿豈-﫿]|[a-z0-9]+(?:'[a-z]+)?")
# Scripts a dictation of yours should contain: ASCII, CJK, and punctuation.
EXPECTED = re.compile(r"[\x00-\x7f -⁯　-〿㐀-鿿豈-﫿＀-￯ -¿×÷]")


def tokens(text):
    return TOKEN.findall(T2S.convert(text or "").lower())


def owl(binary, home, wav, model, ai):
    cmd = [binary, "-f", wav, "--json", "--model", model] + (["--post-process"] if ai else [])
    env = dict(os.environ, HOME=str(home))
    p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
    line = next((l for l in reversed(p.stdout.splitlines()) if l.startswith("{")), None)
    if p.returncode or not line:
        raise RuntimeError(f"{wav}: exit {p.returncode}: {p.stderr.strip().splitlines()[-1:]}")
    return json.loads(line)


def make_home(set_dir):
    """A home folder with the frozen settings and the downloaded models."""
    home = Path(tempfile.mkdtemp(prefix="home-", dir=set_dir))
    app = home / "Library/Application Support/com.hao.owl"
    (app / "models").mkdir(parents=True)  # empty: startup tidies this folder
    shutil.copy(set_dir / "settings_store.json", app)
    (home / ".cache").mkdir()
    (home / ".cache/huggingface").symlink_to(HOME / ".cache/huggingface")
    return home


def rate(pairs):
    """Token edit distance over reference length, summed across clips."""
    pairs = list(pairs)
    errs = sum(Levenshtein.distance(tokens(h), tokens(r)) for h, r in pairs)
    total = sum(len(tokens(r)) for _, r in pairs)
    return round(errs / max(total, 1), 4)


def score(clips, settings):
    speech = [c for c in clips if c["kind"] == "speech"]
    empty = [c for c in clips if c["kind"] == "empty"]
    ai = [c for c in speech if c.get("post_processed") is not None]
    words = settings.get("custom_words", [])

    def recall(field):
        hit = seen = 0
        for c in speech:
            for w in words:
                if w.lower() in c["final"].lower():
                    seen += 1
                    hit += w.lower() in (c.get(field) or "").lower()
        return dict(found=hit, expected=seen)

    def wrong_script(field):
        # Owl is set to write Traditional; count characters only Simplified uses.
        cjk = bad = 0
        for c in speech:
            for ch in re.findall(r"[㐀-鿿]", c.get(field) or ""):
                cjk += 1
                bad += S2T.convert(ch) != ch
        return dict(simplified_chars=bad, chinese_chars=cjk)

    def strays(field):
        return [c["id"] for c in clips if re.search(r"[^\W\d_]", "".join(
            ch for ch in (c.get(field) or "") if not EXPECTED.match(ch)))]

    out = dict(
        speech_clips=len(speech),
        hearing_error=rate((c["text"], c["hearing"]) for c in speech),
        final_error=rate((c["post_processed"], c["final"]) for c in ai) if ai else None,
        wispr_final_error=rate((c["wispr_final"], c["final"]) for c in speech),
        words_hearing=recall("text"),
        words_final=recall("post_processed") if ai else None,
        empty_clips=len(empty),
        empty_ok=sum(not tokens(c["text"]) for c in empty),
        empty_ok_final=sum(not tokens(c.get("post_processed") or "") for c in empty) if ai else None,
        script_hearing=wrong_script("text"),
        script_final=wrong_script("post_processed") if ai else None,
        stray_alphabet_hearing=strays("text"),
        stray_alphabet_final=strays("post_processed") if ai else None,
        ai_left_unchanged=sum(c["post_processed"] == c["text"] for c in ai) if ai else None,
        audio_secs=round(sum(c["audio_secs"] for c in clips), 1),
        speed_x_realtime=round(sum(c["audio_secs"] for c in clips) / max(sum(c["best_ms"] for c in clips) / 1000, 1e-9), 1),
    )
    if settings.get("chinese_script") != "traditional":
        out.pop("script_hearing"), out.pop("script_final")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("name")
    ap.add_argument("--set", default=str(HOME / "Documents/Projects/Owl-evals/dictation-v1"))
    ap.add_argument("--model", help="default: the model in the frozen settings")
    ap.add_argument("--no-ai", action="store_true", help="hearing only; no AI cleanup calls")
    ap.add_argument("--fresh-ai", action="store_true", help="ignore saved AI replies")
    args = ap.parse_args()
    set_dir = Path(args.set)
    settings_raw = (set_dir / "settings_store.json").read_bytes()
    settings = json.loads(settings_raw)["settings"]
    model = args.model or settings["selected_model"]
    clips = [json.loads(l) for l in open(set_dir / "set.jsonl")]
    # shortcut: the key doesn't cover Owl's own AI-step code, so pass
    # --fresh-ai after changing actions.rs or cleanup_guard.rs.
    cache_path = set_dir / "ai_cache.json"
    cache = json.loads(cache_path.read_text()) if cache_path.exists() else {}
    settings_hash = hashlib.sha256(settings_raw + model.encode()).hexdigest()

    home = make_home(set_dir)
    started = time.time()
    try:
        for i, c in enumerate(clips, 1):
            wav = str(set_dir / c["wav"])
            r = owl(args.binary, home, wav, model, ai=False)
            c.update(text=r["text"], audio_secs=r["audio_secs"], best_ms=r["best_ms"])
            if args.no_ai:
                pass
            elif tokens(r["text"]):
                key = hashlib.sha256(f"{settings_hash}\0{r['text']}".encode()).hexdigest()
                if args.fresh_ai or key not in cache:
                    cache[key] = owl(args.binary, home, wav, model, ai=True)["post_processed"]
                c["post_processed"] = cache[key]
            else:
                c["post_processed"] = r["text"]  # nothing heard, so nothing to clean
            print(f"\r{i}/{len(clips)}", end="", flush=True)
    finally:
        shutil.rmtree(home)
        if cache:
            cache_path.write_text(json.dumps(cache, ensure_ascii=False))
    print()

    summary = score(clips, settings)
    summary.update(model=model, run_secs=round(time.time() - started))
    (set_dir / "results").mkdir(exist_ok=True)
    out = set_dir / "results" / f"{args.name}.json"
    out.write_text(json.dumps(dict(summary=summary, clips=clips), ensure_ascii=False, indent=1))
    print(json.dumps(summary, ensure_ascii=False, indent=1))
    print(f"saved {out}")


if __name__ == "__main__":
    main()
