"""Label the dictation set by hand in a browser: what you said, and what you wanted typed.

Usage: python3 scripts/eval/label.py [--set DIR] [--run NAME] [--port N]

Opens a page listing the speech clips. Each clip shows Wispr's and Owl's raw
transcripts (Owl's from results/RUN.json) with the words they disagree on
marked. You write two answers per clip, and they're saved to SET/labels.json:
  verbatim: the words you said
  clean:    the text you wanted typed (fillers out, corrections applied,
            nothing you typed in afterwards)
labels.json is kept apart from set.jsonl, so rebuilding the set never wipes it.
The server listens on this computer only.
"""
import argparse, json, os, re, tempfile, webbrowser
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

HOME = Path.home()
UI = Path(__file__).with_name("label.html")


def read_run(path):
    """A run's model and Owl's (raw, cleaned) text per clip: `owl --eval` results,
    or the older run.py ones."""
    d = json.loads(path.read_text())
    if "items" in d:
        return d.get("model"), {i["id"]: (i["heard"], i["output"]) for i in d["items"]}
    return d["summary"].get("model"), {c["id"]: (c.get("text"), c.get("post_processed")) for c in d["clips"]}


def pick_run(set_dir, model):
    """The newest run of the set's own model, so Owl's text is the one being tuned."""
    runs = sorted((set_dir / "results").glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True)
    for p in runs:
        if read_run(p)[0] == model:
            return p
    return runs[0] if runs else None


def save_json(path, data):
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix=".labels-")
    with os.fdopen(fd, "w") as f:
        json.dump(data, f, ensure_ascii=False, indent=1)
    os.replace(tmp, path)  # a crash mid-write never leaves half a labels file


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default=str(HOME / "Documents/Projects/Owl-evals/dictation-v1"))
    ap.add_argument("--run", help="results/RUN.json to show Owl's text from; default: newest run of the set's model")
    ap.add_argument("--port", type=int, default=8765)
    args = ap.parse_args()
    set_dir = Path(args.set)
    model = json.loads((set_dir / "settings_store.json").read_text())["settings"]["selected_model"]
    run = set_dir / "results" / f"{args.run}.json" if args.run else pick_run(set_dir, model)
    owl = read_run(run)[1] if run else {}
    clips = [json.loads(l) for l in open(set_dir / "set.jsonl")]
    clips = [c for c in clips if c["kind"] == "speech"]
    wavs = {Path(c["wav"]).name: set_dir / c["wav"] for c in clips}
    labels_path = set_dir / "labels.json"
    labels = json.loads(labels_path.read_text()) if labels_path.exists() else {}

    def payload():
        return dict(
            run=run.stem if run else None,
            clips=[dict(id=c["id"], when=c["when"], secs=c["secs"], wav=Path(c["wav"]).name,
                        wispr_raw=c["hearing"], wispr_final=c["wispr_final"], kept=c["final"],
                        edited=c["edited"], owl_raw=owl.get(c["id"], (None, None))[0],
                        owl_final=owl.get(c["id"], (None, None))[1],
                        label=labels.get(c["id"])) for c in clips])

    class Handler(BaseHTTPRequestHandler):
        def send(self, code, body, ctype):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            if self.path == "/":
                self.send(200, UI.read_bytes(), "text/html; charset=utf-8")
            elif self.path == "/api/clips":
                self.send(200, json.dumps(payload(), ensure_ascii=False).encode(), "application/json")
            elif self.path.startswith("/audio/") and self.path[7:] in wavs:
                # shortcut: no Range support, so the player can't seek; fine for short clips.
                self.send(200, wavs[self.path[7:]].read_bytes(), "audio/wav")
            else:
                self.send(404, b"not found", "text/plain")

        def do_POST(self):
            if self.path != "/api/label":
                return self.send(404, b"not found", "text/plain")
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if body.get("id") not in {c["id"] for c in clips} or body.get("status") not in ("done", "skip"):
                return self.send(400, b"bad label", "text/plain")
            labels[body["id"]] = dict(
                status=body["status"],
                verbatim=re.sub(r"\s+", " ", body.get("verbatim") or "").strip(),
                clean=re.sub(r"\s+", " ", body.get("clean") or "").strip(),
                note=(body.get("note") or "").strip(),
                at=datetime.now(timezone.utc).isoformat(timespec="seconds"))
            save_json(labels_path, labels)
            self.send(200, json.dumps(labels[body["id"]], ensure_ascii=False).encode(), "application/json")

        def log_message(self, *_):
            pass

    server = HTTPServer(("127.0.0.1", args.port), Handler)
    url = f"http://127.0.0.1:{args.port}/"
    print(f"{len(clips)} speech clips, {len(labels)} labeled; Owl's text from {run.name if run else 'no run'}")
    print(f"labeling at {url}  (Ctrl+C to stop)")
    if not os.environ.get("NO_BROWSER"):
        webbrowser.open(url)
    server.serve_forever()


if __name__ == "__main__":
    main()
