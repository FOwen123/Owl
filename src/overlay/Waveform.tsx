import React, { useEffect, useRef } from "react";
import "./Waveform.css";

// Number of reactive bars in the waveform (the simple, smoothed style shared by
// every overlay form). Mic levels arrive as 16 FFT buckets; we take the first N.
const WAVE_BARS = 9;
// Taller in the middle, like a voice, so even uneven levels read as one shape.
const WAVE_ENVELOPE = [0.5, 0.68, 0.84, 0.96, 1, 0.96, 0.84, 0.68, 0.5];

/** The dictation waveform, as in the floating bar. `levels` holds the latest
 * mic levels; until `ready` it shows the muted arming pulse. `onLoudest` gets
 * the loudest bar every frame (the overlay's owl bobs with it). */
export const Waveform: React.FC<{
  levels: React.RefObject<number[]>;
  ready: boolean;
  onLoudest?: (level: number) => void;
}> = ({ levels, ready, onLoudest }) => {
  const bars = useRef<(HTMLElement | null)[]>([]);
  // Read by the loop, so a change doesn't restart it.
  const latest = useRef({ ready, onLoudest });
  latest.current = { ready, onLoudest };

  // One loop, once per screen refresh, eases the bars toward the latest mic
  // levels. Writing styles directly (no re-render, no CSS transition restarted
  // by every level event) is what keeps it smooth.
  useEffect(() => {
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const shown = Array(WAVE_BARS).fill(0);
    let last = performance.now();
    // Moves `from` toward `to`, `rate` per second, frame-rate independent.
    const ease = (from: number, to: number, rate: number, dt: number) =>
      from + (to - from) * (1 - Math.exp(-rate * dt));

    let raf = requestAnimationFrame(function tick(now) {
      const dt = Math.min(now - last, 100) / 1000;
      last = now;
      // Rise fast, fall slowly, so bars flow instead of flickering, plus a slow
      // ripple so a pause never looks frozen.
      let loudest = 0;
      for (let i = 0; i < WAVE_BARS; i++) {
        const target = latest.current.ready ? levels.current?.[i] || 0 : 0;
        shown[i] = ease(shown[i], target, target > shown[i] ? 18 : 5, dt);
        loudest = Math.max(loudest, shown[i]);
        const ripple = still ? 1 : 1 + 0.12 * Math.sin(now / 220 - i * 0.6);
        const level =
          (0.3 + Math.pow(shown[i], 0.7) * 0.6 * WAVE_ENVELOPE[i]) * ripple;
        const bar = bars.current[i];
        if (bar) bar.style.transform = `scaleY(${Math.min(1, level)})`;
      }
      latest.current.onLoudest?.(loudest);
      raf = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(raf);
  }, [levels]);

  return (
    <div className={`swave ${ready ? "ready" : "arming"}`} aria-hidden>
      {Array.from({ length: WAVE_BARS }, (_, i) => (
        <i
          key={i}
          ref={(el) => {
            bars.current[i] = el;
          }}
        />
      ))}
    </div>
  );
};
