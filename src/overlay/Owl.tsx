import React, { useEffect, useId, useRef } from "react";
import body from "./owl/body.png";
import eyes from "./owl/eyes.png";
import footLeft from "./owl/foot-left.png";
import footRight from "./owl/foot-right.png";
import pupilLeft from "./owl/pupil-left.png";
import pupilRight from "./owl/pupil-right.png";
import rig from "./owl/rig.json";
import tuftLeft from "./owl/tuft-left.png";
import tuftRight from "./owl/tuft-right.png";
import wingLeft from "./owl/wing-left.png";
import wingRight from "./owl/wing-right.png";

/** What the owl is doing, one per floating window state. */
export type OwlMood =
  | "sleepy" // dictation armed, mic not live yet
  | "listening" // dictating
  | "reading" // transcribing
  | "thinking" // AI cleanup
  | "studying" // a word offered for the dictionary
  | "puzzled" // no text box to paste into
  | "perked" // "In a meeting?"
  | "drowsy" // "Meeting finished?"
  | "noting" // taking meeting notes
  | "happy"; // a card's action is done

// The owl's parts (scripts/owl-icons/build-icons.swift), drawn as masks so it
// takes the text color, like the menu-bar icon. Pivots and eye boxes are in
// percent of the owl.
const mask = (url: string): React.CSSProperties => ({
  WebkitMaskImage: `url(${url})`,
  maskImage: `url(${url})`,
});
const origin = ([x, y]: number[]) => `${x}% ${y}%`;
/** A lid over one eye, wider than it so a slanted lid still covers the corners. */
const lidBox = ([l, t, r, b]: number[]): React.CSSProperties => ({
  left: `${l - 8}%`,
  top: `${t - 2}%`,
  width: `${r - l + 16}%`,
  height: `${b - t + 4}%`,
});

const rand = (min: number, max: number) => min + Math.random() * (max - min);
// 0 → 1 → 0 over `ms` after `start`, then 0.
const pulse = (now: number, start: number, ms: number) => {
  const t = (now - start) / ms;
  return t > 0 && t < 1 ? Math.sin(Math.PI * t) : 0;
};

// Each animated value is a spring, so moves overshoot and settle like a soft
// toy instead of sliding. [stiffness, damping]
const SPRINGS = {
  lift: [300, 16], // % of the owl, up is negative
  tilt: [140, 11], // degrees
  squash: [420, 15], // + stretches tall, - squashes flat
  wingL: [260, 13], // degrees, + lifts it away from the body
  wingR: [260, 13],
  tuftL: [240, 8], // degrees, + perks up, - droops
  tuftR: [240, 8],
  lidL: [900, 60], // 0 open → 1 shut
  lidR: [900, 60],
  slantL: [300, 30], // degrees; + drops the inner corner (cross), - the outer (sad)
  slantR: [300, 30],
  smileL: [500, 44], // lower lid, 0 → 1
  smileR: [500, 44],
  lookX: [320, 32], // % of the owl
  lookY: [320, 32],
  pupil: [320, 20], // pupil scale
} as const;
type Channel = keyof typeof SPRINGS;
type Pose = Record<Channel, number>;
const REST: Pose = {
  lift: 0,
  tilt: 0,
  squash: 0,
  wingL: 0,
  wingR: 0,
  tuftL: 0,
  tuftR: 0,
  lidL: 0,
  lidR: 0,
  slantL: 0,
  slantR: 0,
  smileL: 0,
  smileR: 0,
  lookX: 0,
  lookY: 0,
  pupil: 1,
};

// Brush-drawn extras around the owl, in the same percent space (the SVG
// spans -30%…130%). Only the cards have room for them: in the dictation pill
// the owl sits in the rounded end with 6 px above and below, so the moods
// shown there (sleepy, listening, reading, thinking) draw none.
const Z = (s: number) =>
  `M${-s / 2} ${-s / 2}H${s / 2}L${-s / 2} ${s / 2}H${s / 2}`;
const SPARK = (r: number) =>
  `M0 ${-r}Q0 0 ${r} 0Q0 0 0 ${r}Q0 0 ${-r} 0Q0 0 0 ${-r}Z`;
const SPARKS: [number, number, number][] = [
  [-14, 14, 15],
  [114, 4, 17],
  [110, 72, 12],
];

/** The menu-bar owl, alive for `mood`. `voice` (0–1, written by the caller's
 * own loop) makes it bob while you talk, `seconds` is how long a drowsy owl
 * takes to nod off, and each change of `beat` makes it nod. */
export const Owl: React.FC<{
  mood: OwlMood;
  voice?: React.RefObject<number>;
  seconds?: number;
  beat?: number;
}> = ({ mood, voice, seconds = 8, beat = 0 }) => {
  const rough = `owl-rough-${useId().replace(/:/g, "")}`;
  const ref = useRef<Record<string, HTMLElement | SVGElement | null>>({});
  const set = (name: string) => (el: HTMLElement | SVGElement | null) => {
    ref.current[name] = el;
  };
  // Read by the loop, so a new mood springs from the current pose instead of
  // restarting the animation.
  const propsRef = useRef({ mood, voice, seconds, beat });
  propsRef.current = { mood, voice, seconds, beat };

  // One loop, once per screen refresh, writing styles directly (no re-render).
  useEffect(() => {
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const el = ref.current;
    const springs = Object.fromEntries(
      Object.keys(SPRINGS).map((k) => [k, { x: REST[k as Channel], v: 0 }]),
    ) as Record<Channel, { x: number; v: number }>;
    let last = performance.now();
    let mood: OwlMood | null = null;
    let since = last;
    let beat = propsRef.current.beat;
    let nodAt = -Infinity;
    let loud = 0;
    let blinkAt = last + rand(1500, 4000);
    let glanceAt = last + rand(2500, 5000);
    let glanceEnd = 0;
    let glance = { x: 0, y: 0 };
    // Repeating motion, held at rest when the user asks for less motion.
    const wave = (now: number, period: number, phase = 0) =>
      still ? 0 : Math.sin(now / period + phase);

    let raf = requestAnimationFrame(function tick(now) {
      const dt = Math.min(now - last, 100) / 1000;
      last = now;
      const p = propsRef.current;
      if (p.mood !== mood) {
        mood = p.mood;
        since = now;
      }
      if (p.beat !== beat) {
        beat = p.beat;
        nodAt = now;
      }
      const t = still ? 1e6 : now - since; // reduced motion: skip entrances
      loud += ((p.voice?.current ?? 0) - loud) * (1 - Math.exp(-10 * dt));

      const g: Pose = { ...REST };
      let blinks = true;
      let breath = 0.02;
      // Extras: opacity (0 = hidden) of each prop this frame.
      let zzz = 0;
      let ask = 0;
      let lines = 0;
      let sparks = 0;

      // One idea per mood, in small moves: the owl is only 22 px. Repeating
      // motion stays gentle; only perked and happy have an entrance, once.
      switch (mood) {
        case "sleepy":
          // Heavy, slanted lids, tufts down, slow breaths.
          g.lidL = g.lidR = 0.6;
          g.slantL = g.slantR = -8;
          g.tuftL = g.tuftR = -8;
          breath = 0.03;
          break;
        case "listening":
          // Bobs with your voice, tufts perking when it's loud; glances
          // around while you're quiet.
          g.lift = -5 * loud;
          g.squash = 0.04 * loud;
          g.tuftL = g.tuftR = 8 * loud;
          g.pupil = 1 + 0.08 * loud;
          if (!still && loud < 0.15 && now >= glanceAt) {
            glance = { x: rand(-4, 4), y: rand(-2, 2) };
            glanceEnd = now + rand(600, 1400);
            glanceAt = now + rand(3000, 7000);
          } else if (now >= glanceEnd || loud >= 0.15) {
            glance = { x: 0, y: 0 };
          }
          g.lookX = glance.x;
          g.lookY = glance.y;
          break;
        case "reading": {
          // Eyes read along a line and back, lids lowered to focus.
          const along = still ? 0.5 : Math.min(((t % 1300) / 1300) * 1.3, 1);
          g.lookX = -4 + 8 * along;
          g.lookY = 2;
          g.tilt = g.lookX * 0.3;
          g.lidL = g.lidR = 0.18;
          break;
        }
        case "thinking":
          // Eyes up and drifting slowly side to side, one squinting, head
          // cocked.
          g.lookX = 3 * wave(now, 1400);
          g.lookY = -4;
          g.lidL = 0.05;
          g.lidR = 0.38;
          g.slantR = 10;
          g.tilt = 5;
          break;
        case "studying":
          // Peering down at the word, focused, a slow nod.
          g.lookY = 4;
          g.lidL = g.lidR = 0.3;
          g.slantL = g.slantR = 5;
          g.lift = 1.5 * (1 + wave(now, 500));
          break;
        case "puzzled":
          // Head cocked, one eye narrowed, one tuft down, "?" bobbing.
          g.tilt = 9;
          g.lookX = 4;
          g.lookY = -2;
          g.lidL = 0.3;
          g.slantL = -10;
          g.tuftL = -12;
          ask = 1;
          break;
        case "perked":
          // Once: a crouch, a spring up with wings out and pupils wide, two
          // blinks. Then it waits, wide-eyed, looking at the button.
          g.lookX = 4;
          g.pupil = 1.25;
          if (t < 130) {
            g.squash = -0.1;
            g.lift = 3;
          } else if (t < 420) {
            g.squash = 0.07;
            g.lift = -10;
            g.wingL = g.wingR = 26;
            g.tuftL = g.tuftR = 12;
          }
          if (t > 560 && t < 980) {
            g.lidL = g.lidR = Math.max(pulse(t, 560, 170), pulse(t, 780, 170));
          }
          blinks = t > 1000;
          lines = still ? 0 : pulse(t, 120, 800);
          break;
        case "drowsy": {
          // Nods off over the countdown: lids sink, tufts droop, head
          // dips, "Zzz" drifts up.
          const sleep = still ? 0.6 : Math.min(t / (p.seconds * 1000), 1);
          g.lidL = g.lidR = 0.2 + 0.55 * sleep;
          g.slantL = g.slantR = -10 * sleep;
          g.lift = 3 * sleep;
          g.tilt = 5 * sleep;
          g.tuftL = g.tuftR = -14 * sleep;
          blinks = sleep < 0.5;
          zzz = Math.max(0, Math.min(1, (sleep - 0.3) * 3));
          break;
        }
        case "noting": {
          // Eyes down on the notes, looking up now and then.
          const up = pulse(now % 4200, 3300, 700);
          g.lookY = 4 - 6 * up;
          g.lidL = g.lidR = 0.2 * (1 - up);
          break;
        }
        case "happy":
          // Smiling eyes; once: a hop with a few wing flaps and sparkles.
          g.smileL = g.smileR = 0.55;
          g.slantL = g.slantR = -6;
          if (t < 110) {
            g.squash = -0.1;
            g.lift = 2;
          } else if (t < 420) {
            g.squash = 0.07;
            g.lift = -11;
          }
          g.wingL = g.wingR = t < 700 ? 28 * Math.abs(Math.sin(t / 70)) : 0;
          g.tuftL = g.tuftR = 8;
          blinks = false;
          sparks = 1;
          break;
      }

      // A nod for each new transcript line.
      g.lift += still ? 0 : 4 * pulse(now, nodAt, 420);
      g.squash -= still ? 0 : 0.03 * pulse(now, nodAt, 420);

      if (blinks && !still && now >= blinkAt) {
        const b = (now - blinkAt) / 190;
        if (b < 1) {
          const shut = 1 - Math.abs(2 * b - 1);
          g.lidL = Math.max(g.lidL, shut);
          g.lidR = Math.max(g.lidR, shut);
        } else blinkAt = now + (Math.random() < 0.2 ? 150 : rand(2500, 6000));
      }

      // Springs, in small steps so stiff ones stay stable. Tufts lag behind
      // the head and wings flare on landing: the follow-through that makes
      // the owl feel soft.
      const s = springs;
      for (let left = dt; left > 0; left -= 1 / 120) {
        const h = Math.min(left, 1 / 120);
        for (const k of Object.keys(SPRINGS) as Channel[]) {
          let target = g[k];
          if (k === "tuftL" || k === "tuftR")
            target +=
              s.lift.v * 0.12 - s.tilt.v * (k === "tuftL" ? 0.05 : -0.05);
          if (k === "wingL" || k === "wingR")
            target += Math.max(0, s.lift.v) * 0.1;
          const c = s[k];
          if (still) {
            c.x = target;
            c.v = 0;
            continue;
          }
          const [stiff, damp] = SPRINGS[k];
          c.v += (stiff * (target - c.x) - damp * c.v) * h;
          c.x += c.v * h;
        }
      }

      const pose = Object.fromEntries(
        Object.entries(s).map(([k, c]) => [k, c.x]),
      ) as Pose;
      const style = (name: string, css: string) => {
        const node = el[name];
        if (node) node.style.transform = css;
      };
      const grow = 1 + breath * wave(now, mood === "sleepy" ? 900 : 600);
      style("owl", `translateY(${pose.lift}%)`);
      style(
        "frame",
        `rotate(${pose.tilt}deg) scale(${grow * (1 - pose.squash * 0.5)}, ${grow * (1 + pose.squash)})`,
      );
      style("wingL", `rotate(${pose.wingL}deg)`);
      style("wingR", `rotate(${-pose.wingR}deg)`);
      style("tuftL", `rotate(${pose.tuftL}deg)`);
      style("tuftR", `rotate(${-pose.tuftR}deg)`);
      const look = `translate(${pose.lookX}%, ${pose.lookY}%) scale(${pose.pupil})`;
      style("pupilL", look);
      style("pupilR", look);
      // A slant only shows once the lid is partway down, so an open eye
      // never has a lid corner poking into it.
      const lid = (shut: number, slant: number, side: number) =>
        `translateY(${(Math.min(shut, 1) - 1) * 100}%) rotate(${side * slant * Math.min(1, shut * 3)}deg)`;
      style("lidL", lid(pose.lidL, pose.slantL, 1));
      style("lidR", lid(pose.lidR, pose.slantR, -1));
      const smile = (up: number) =>
        `translateY(${(1 - Math.min(Math.max(up, 0), 1)) * 100}%)`;
      style("smileL", smile(pose.smileL));
      style("smileR", smile(pose.smileR));

      // Extras.
      const show = (name: string, opacity: number, transform = "") => {
        const node = el[name];
        if (!node) return;
        node.style.opacity = String(Math.max(0, Math.min(1, opacity)));
        if (transform) node.setAttribute("transform", transform);
      };
      for (let i = 0; i < 3; i++) {
        // Each "z" drifts up and right, growing, then fades.
        const q = still ? 0.2 + i * 0.3 : (now / 2600 + i / 3) % 1;
        show(
          `z${i}`,
          zzz * (still ? 1 : Math.sin(Math.PI * q)),
          `translate(${88 + 24 * q} ${14 - 24 * q}) scale(${0.7 + 0.5 * q})`,
        );
        // Sparkles pop once, in turn.
        const [sx, sy] = SPARKS[i];
        const pop = still ? 1 : pulse(t, i * 160, 650);
        show(
          `spark${i}`,
          sparks * pop,
          `translate(${sx} ${sy}) rotate(${45 * pop}) scale(${0.4 + 0.6 * pop})`,
        );
      }
      show(
        "ask",
        ask,
        `translate(108 ${4 + 2.5 * wave(now, 300)}) rotate(${5 * wave(now, 450)}) scale(1.5)`,
      );
      show(
        "lines",
        lines,
        `translate(92 14) scale(${0.6 + 0.6 * lines}) translate(-92 -14)`,
      );

      raf = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(raf);
  }, []);

  const part = (
    name: string,
    url: string,
    pivot?: number[],
  ): React.ReactNode => (
    <span
      className="sowl-layer"
      ref={set(name)}
      style={{ ...mask(url), transformOrigin: pivot && origin(pivot) }}
    />
  );

  return (
    <span className="sowl" ref={set("owl")} aria-hidden="true">
      {/* Feet stay planted while the body squashes and tilts above them. */}
      {part("footL", footLeft)}
      {part("footR", footRight)}
      <span className="sowl-frame" ref={set("frame")}>
        {part("wingL", wingLeft, rig.wingLeft)}
        {part("wingR", wingRight, rig.wingRight)}
        {part("tuftL", tuftLeft, rig.tuftLeft)}
        {part("tuftR", tuftRight, rig.tuftRight)}
        {part("body", body)}
        {/* Pupils and lids, clipped to the eye openings. */}
        <span className="sowl-layer sowl-eyes" style={mask(eyes)}>
          {part("pupilL", pupilLeft, rig.pupilLeft)}
          {part("pupilR", pupilRight, rig.pupilRight)}
          <span
            className="sowl-lid"
            ref={set("lidL")}
            style={{ ...lidBox(rig.eyeLeft), transformOrigin: "50% 100%" }}
          />
          <span
            className="sowl-lid"
            ref={set("lidR")}
            style={{ ...lidBox(rig.eyeRight), transformOrigin: "50% 100%" }}
          />
          <span
            className="sowl-lid smile"
            ref={set("smileL")}
            style={lidBox(rig.eyeLeft)}
          />
          <span
            className="sowl-lid smile"
            ref={set("smileR")}
            style={lidBox(rig.eyeRight)}
          />
        </span>
      </span>
      <svg className="sowl-extras" viewBox="-30 -30 160 160">
        <defs>
          {/* Roughens the strokes so they match the brush drawing. */}
          <filter id={rough} x="-30%" y="-30%" width="160%" height="160%">
            <feTurbulence
              type="fractalNoise"
              baseFrequency="0.7"
              numOctaves="2"
              seed="3"
            />
            <feDisplacementMap in="SourceGraphic" scale="2.4" />
          </filter>
        </defs>
        <g
          filter={`url(#${rough})`}
          fill="none"
          stroke="currentColor"
          strokeWidth="6.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          {[0, 1, 2].map((i) => (
            <path key={`z${i}`} ref={set(`z${i}`)} d={Z(16)} opacity="0" />
          ))}
          <g ref={set("ask")} opacity="0">
            <path
              d="M-6 -10C-6 -18 7 -18 7 -10C7 -4 0 -3 0 4"
              strokeWidth="4.5"
            />
            <circle
              cx="0"
              cy="11.5"
              r="2.6"
              fill="currentColor"
              stroke="none"
            />
          </g>
          <g ref={set("lines")} opacity="0">
            <path d="M93 8L103 -2M98 20L111 18M86 1L89 -11" />
          </g>
          {SPARKS.map(([, , r], i) => (
            <path
              key={`spark${i}`}
              ref={set(`spark${i}`)}
              d={SPARK(r)}
              fill="currentColor"
              stroke="none"
              opacity="0"
            />
          ))}
        </g>
      </svg>
    </span>
  );
};
