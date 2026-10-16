import { listen } from "@tauri-apps/api/event";
import { BookPlus, Check } from "lucide-react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import type { ModelStateEvent } from "@/lib/types/events";
import { getLanguageDirection } from "@/lib/utils/rtl";
import owlBody from "./owl/body.png";
import owlEyes from "./owl/eyes.png";
import owlPupils from "./owl/pupils.png";

type OverlayState =
  | "recording"
  | "streaming"
  | "transcribing"
  | "processing"
  | "suggestion";

// A card's edge (a `width`x52 pill, 1px in so a 2px stroke stays inside),
// starting at the top center and running clockwise.
export const ring = (width: number) =>
  `M${width / 2} 1H${width - 26}A25 25 0 0 1 ${width - 26} 51H26A25 25 0 0 1 26 1Z`;
// The suggestion card is 300 wide.
const RING = ring(300);

// Owl learn-from-edits: a fix offered with a countdown (correction_watch.rs).
interface EditSuggestion {
  from: string;
  to: string;
  seconds: number;
}

// Number of reactive bars in the waveform (the simple, smoothed style shared by
// every overlay form). Mic levels arrive as 16 FFT buckets; we take the first N.
const WAVE_BARS = 9;
// Taller in the middle, like a voice, so even uneven levels read as one shape.
const WAVE_ENVELOPE = [0.5, 0.68, 0.84, 0.96, 1, 0.96, 0.84, 0.68, 0.5];

// Owl mascot layers (scripts/owl-icons/build-icons.swift), drawn as masks so
// the owl takes the text color, like the menu-bar icon.
const mask = (url: string): React.CSSProperties => ({
  WebkitMaskImage: `url(${url})`,
  maskImage: `url(${url})`,
});
const rand = (min: number, max: number) => min + Math.random() * (max - min);

// Only call out a model load in the Live preview once it has run this long.
// Warm loads finish in well under this (~0.2s on Apple Silicon, ~1.5s on a
// Windows CPU backend), so the common case never flashes a loading notice while
// the user is speaking; cold loads can take 40s+.
const SLOW_MODEL_LOAD_MS = 2000;

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // `Stream::play()` returning does not mean hardware callbacks are flowing.
  // Stay visually in an arming state until the backend processes the first
  // actual microphone sample chunk.
  const [captureReady, setCaptureReady] = useState(false);
  // Recording starts while the model loads in the background. Say so, otherwise
  // a cold start looks like an empty Live preview that ignores speech, then a
  // hung "Transcribing..." spinner. Once recording stops, any in-flight load is
  // the wait, so the working label reflects it immediately; the Live notice
  // waits for the load to be slow so fast loads don't flash it mid-speech.
  const [modelLoading, setModelLoading] = useState(false);
  const [modelLoadSlow, setModelLoadSlow] = useState(false);
  const modelLoadTimerRef = useRef<ReturnType<typeof setTimeout>>();
  // Latched per Live session: once the loading notice has opened the panel, keep
  // it open after the load so it doesn't collapse and reopen when text arrives.
  const [loadNoticeShown, setLoadNoticeShown] = useState(false);
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = useState<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);
  const [suggestion, setSuggestion] = useState<EditSuggestion | null>(null);
  const [suggestionAdded, setSuggestionAdded] = useState(false);

  // Latest mic levels, read by the animation loop below. Kept out of React state
  // so ~24 level events a second don't re-render the overlay.
  const levelsRef = useRef<number[]>(Array(16).fill(0));
  // What the animation loop needs to know, refreshed on every render.
  const motionRef = useRef({ ready: false, working: false });
  const barsRef = useRef<(HTMLElement | null)[]>([]);
  const owlRef = useRef<HTMLSpanElement>(null);
  const pupilsRef = useRef<HTMLSpanElement>(null);
  const lidRef = useRef<HTMLSpanElement>(null);
  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        const overlayState = event.payload as OverlayState;
        // Reset synchronously before settings I/O. A fast microphone can emit
        // recording-ready while the awaits below are in flight; resetting after
        // them would overwrite that event and leave the overlay stuck arming.
        if (overlayState === "recording" || overlayState === "streaming") {
          setCaptureReady(false);
          levelsRef.current = Array(16).fill(0);
          setStreamText({ committed: "", tentative: "" });
        }

        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match.
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(
              settings.data.overlay_position === "top" ? "top" : "bottom",
            );
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        setState(overlayState);
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setElapsed(0);
          setLoadNoticeShown(false);
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setCaptureReady(false);
      });

      const unlistenSuggestion = await listen<EditSuggestion>(
        "edit-suggestion",
        (event) => {
          setSuggestion(event.payload);
          setSuggestionAdded(false);
        },
      );

      const unlistenSuggestionAdded = await listen(
        "edit-suggestion-added",
        () => setSuggestionAdded(true),
      );

      const unlistenReady = await listen("recording-ready", () => {
        setElapsed(0);
        setCaptureReady(true);
      });

      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        levelsRef.current = event.payload;
      });

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      // The backend ends every `loading_started` with exactly one of completed
      // or failed, so only those end a load. Other events (e.g. `unloaded` from
      // the idle watcher) aren't ordered against an in-flight load and must not
      // clear it.
      const unlistenModel = await listen<ModelStateEvent>(
        "model-state-changed",
        (event) => {
          const type = event.payload.event_type;
          if (type === "loading_started") {
            clearTimeout(modelLoadTimerRef.current);
            setModelLoading(true);
            setModelLoadSlow(false);
            modelLoadTimerRef.current = setTimeout(
              () => setModelLoadSlow(true),
              SLOW_MODEL_LOAD_MS,
            );
          } else if (
            type === "loading_completed" ||
            type === "loading_failed"
          ) {
            clearTimeout(modelLoadTimerRef.current);
            setModelLoading(false);
            setModelLoadSlow(false);
          }
        },
      );

      return () => {
        unlistenShow();
        unlistenHide();
        unlistenSuggestion();
        unlistenSuggestionAdded();
        unlistenReady();
        unlistenLevel();
        unlistenStream();
        unlistenPhase();
        unlistenModel();
        clearTimeout(modelLoadTimerRef.current);
      };
    };

    setupEventListeners();
  }, []);

  // One loop, once per screen refresh, eases the waveform toward the latest mic
  // levels and animates the owl. Writing styles directly (no re-render, no CSS
  // transition restarted by every level event) is what keeps it smooth.
  useEffect(() => {
    if (!isVisible || state === "suggestion") return;
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const shown = Array(WAVE_BARS).fill(0);
    let last = performance.now();
    let voice = 0;
    let lid = 0;
    let blinkAt = last + rand(1500, 4000);
    let glanceAt = last + rand(2500, 5000);
    let glanceEnd = 0;
    let look = { x: 0, y: 0 };
    const eye = { x: 0, y: 0 };
    // Moves `from` toward `to`, `rate` per second, frame-rate independent.
    const ease = (from: number, to: number, rate: number, dt: number) =>
      from + (to - from) * (1 - Math.exp(-rate * dt));

    let raf = requestAnimationFrame(function tick(now) {
      const dt = Math.min(now - last, 100) / 1000;
      last = now;
      const { ready, working } = motionRef.current;

      // Waveform: rise fast, fall slowly, so bars flow instead of flickering,
      // plus a slow ripple so a pause never looks frozen.
      let loudest = 0;
      for (let i = 0; i < WAVE_BARS; i++) {
        const target = ready ? levelsRef.current[i] || 0 : 0;
        shown[i] = ease(shown[i], target, target > shown[i] ? 18 : 5, dt);
        loudest = Math.max(loudest, shown[i]);
        const ripple = still ? 1 : 1 + 0.12 * Math.sin(now / 220 - i * 0.6);
        const level =
          (0.3 + Math.pow(shown[i], 0.7) * 0.6 * WAVE_ENVELOPE[i]) * ripple;
        const bar = barsRef.current[i];
        if (bar) bar.style.transform = `scaleY(${Math.min(1, level)})`;
      }

      // Owl: sleepy until the mic is live, blinks now and then, glances around
      // while you're quiet, looks side to side while transcribing.
      voice = ease(voice, loudest, 10, dt);
      let lidTarget = ready || working ? 0 : 0.5;
      if (!still && now >= blinkAt) {
        const t = (now - blinkAt) / 200;
        if (t < 1) lidTarget = Math.max(lidTarget, 1 - Math.abs(2 * t - 1));
        else blinkAt = now + (Math.random() < 0.2 ? 150 : rand(2500, 6000));
      }
      lid = ease(lid, lidTarget, 40, dt);
      if (working) {
        look = { x: still ? 0 : 0.06 * Math.sin(now / 450), y: 0 };
      } else if (!still && voice < 0.15 && now >= glanceAt) {
        look = { x: rand(-0.06, 0.06), y: rand(-0.03, 0.03) };
        glanceEnd = now + rand(600, 1400);
        glanceAt = now + rand(3000, 7000);
      } else if (now >= glanceEnd || voice >= 0.15) {
        look = { x: 0, y: 0 };
      }
      eye.x = ease(eye.x, look.x, 12, dt);
      eye.y = ease(eye.y, look.y, 12, dt);

      const breath = still ? 1 : 1 + 0.025 * Math.sin(now / 600);
      if (owlRef.current)
        owlRef.current.style.transform = `translateY(${-voice * 1.5}px) scale(${breath})`;
      if (pupilsRef.current)
        pupilsRef.current.style.transform = `translate(${eye.x * 100}%, ${eye.y * 100}%)`;
      if (lidRef.current) lidRef.current.style.transform = `scaleY(${lid})`;

      raf = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(raf);
  }, [isVisible, state]);

  // Elapsed capture timer starts only once microphone samples are flowing.
  useEffect(() => {
    if (state !== "streaming" || !isVisible || !captureReady) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible, captureReady]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // `session` re-runs this when a new Live session starts mid-load (e.g. a
  // retry during a cold load), since the reset doesn't change the other deps.
  useEffect(() => {
    if (modelLoadSlow && state === "streaming") setLoadNoticeShown(true);
  }, [modelLoadSlow, state, session]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  motionRef.current = {
    ready: captureReady,
    working:
      state === "transcribing" ||
      state === "processing" ||
      (state === "streaming" && phase === "working"),
  };

  if (!isVisible) return null;

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  const transcribingLabel = modelLoading
    ? t("overlay.loadingModel")
    : t("overlay.transcribing");

  // ---- Shared building blocks (one visual language for every overlay form) ----
  const waveform = (
    <div className={`swave ${captureReady ? "ready" : "arming"}`}>
      {Array.from({ length: WAVE_BARS }, (_, i) => (
        <i
          key={i}
          ref={(el) => {
            barsRef.current[i] = el;
          }}
        />
      ))}
    </div>
  );

  // The menu-bar owl: body, then pupils and lids clipped to the eye openings.
  const owl = (
    <span className="sowl" ref={owlRef} aria-hidden="true">
      <span className="sowl-layer" style={mask(owlBody)} />
      <span className="sowl-layer sowl-eyes" style={mask(owlEyes)}>
        <span
          className="sowl-layer sowl-ink"
          ref={pupilsRef}
          style={mask(owlPupils)}
        />
        <span className="sowl-layer sowl-ink sowl-lid" ref={lidRef} />
      </span>
    </span>
  );

  const cancelBtn = (
    <button
      className="sx"
      aria-label="cancel"
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  // cancel (left) | waveform (center) | timer + owl (right) — same structure
  // for pill & panel, so the Live morph is a pure width change.
  const listeningRow = (showTimer: boolean, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">{showCancel && cancelBtn}</div>
      {waveform}
      <div className="sbase-r">
        {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
        {owl}
      </div>
    </div>
  );

  // cancel (left) | label (center) | owl (right) — same 3-zone grid as the
  // listening row, so the cancel button and owl stay put and the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">{showCancel && cancelBtn}</div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r">{owl}</div>
    </div>
  );

  // ---- Owl: "Adding to dictionary" with a countdown line; X skips it ----
  if (state === "suggestion" && suggestion) {
    const dismiss = () => {
      setIsVisible(false);
      commands.dismissEditSuggestion({
        from: suggestion.from,
        to: suggestion.to,
      });
    };
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
      >
        <div
          key={`${suggestion.from}→${suggestion.to}`}
          className={`scard ssug ${suggestionAdded ? "added" : ""}`}
        >
          <span className="ssug-book" aria-hidden="true">
            {suggestionAdded ? <Check /> : <BookPlus />}
          </span>
          <div className="ssug-text">
            <span className="ssug-label">
              {suggestionAdded
                ? t("overlay.addedToDictionary")
                : t("overlay.addingToDictionary")}
            </span>
            <span className="ssug-pair">
              {suggestion.from && (
                <>
                  <span className="ssug-from">{suggestion.from}</span>
                  <span className="ssug-arrow">→</span>
                </>
              )}
              <span className="ssug-to">{suggestion.to}</span>
            </span>
          </div>
          {!suggestionAdded && (
            <button
              className="sx"
              aria-label={t("overlay.dontAdd")}
              onClick={dismiss}
            >
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path
                  d="M4 4 L12 12 M12 4 L4 12"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          )}
          <svg className="ssug-ring" viewBox="0 0 300 52" aria-hidden="true">
            <path className="ssug-track" d={RING} pathLength={100} />
            {!suggestionAdded && (
              <path
                className="ssug-countdown"
                d={RING}
                pathLength={100}
                style={{ animationDuration: `${suggestion.seconds}s` }}
              />
            )}
          </svg>
        </div>
      </div>
    );
  }

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // While listening, a slow model load opens the panel with a notice where the
    // text would be, so speech that isn't transcribing yet doesn't look ignored.
    const loadingNotice = modelLoadSlow && !working && !hasText;
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText || loadingNotice || (loadNoticeShown && !working);
    const collapsed = working && !hasText;

    return (
      <div dir={direction} className={`ov-stage ${position}`}>
        <div
          key={session}
          className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
            isVisible ? "" : "leaving"
          }`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                ref={capRef}
                onScroll={handleStreamScroll}
              >
                <p>
                  {loadingNotice && (
                    <span className="sloading">
                      {t("overlay.loadingModel")}
                    </span>
                  )}
                  <span className="committed">
                    {streamText.committed ? streamText.committed + " " : ""}
                  </span>
                  <span className="tentative">{streamText.tentative}</span>
                  {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                  {!working && !loadingNotice && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                workKind === "polishing"
                  ? t("overlay.processing")
                  : transcribingLabel,
                true,
              )
            : listeningRow(open, true)}
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing). Never both. The pill animates its
  // width between them; the cancel button is in both rows so it stays put.
  const working = state === "transcribing" || state === "processing";
  const workLabel =
    state === "processing" ? t("overlay.processing") : transcribingLabel;

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
    >
      <div
        className={`scard compact ${working && isVisible ? "cworking" : ""}`}
      >
        {working ? workingRow(workLabel, true) : listeningRow(false, true)}
      </div>
    </div>
  );
};

export default RecordingOverlay;
