// Dev-only: plays the recording overlay in a browser with fake mic levels.
// `bun run dev`, then open http://localhost:1420/src/overlay/index.html?mock
// (add `&state=transcribing` for the working pill).
import { emit } from "@tauri-apps/api/event";

export function playMockOverlay() {
  const state = new URLSearchParams(location.search).get("state");
  document.body.style.background = "#3a3a3a";
  setTimeout(async () => {
    await emit("show-overlay", state ?? "recording");
    // Arming (sleepy owl) for a moment, as when the mic is starting.
    setTimeout(() => emit("recording-ready"), 1200);
    // Talk for ~4s, pause for ~3s, repeat.
    setInterval(() => {
      const t = performance.now() / 1000;
      const talking = t % 7 < 4;
      emit(
        "mic-level",
        Array.from({ length: 16 }, (_, i) =>
          talking
            ? 0.25 + 0.6 * Math.random() * Math.abs(Math.sin(t * 6 + i))
            : 0,
        ),
      );
    }, 42);
  }, 300);
}
