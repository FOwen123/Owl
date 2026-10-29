import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Check, Keyboard } from "lucide-react";
import { events } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { useModelStore } from "../../stores/modelStore";
import { formatKeyCombination } from "../../lib/utils/keyboard";
import { pendingModel } from "./ModelStep";
import { Waveform } from "@/overlay/Waveform";

const isWordChar = (c: string) => /[\p{L}\p{N}]/u.test(c);

/** How much of `phrase` the heard text covers, in order: the letters (or
 * Chinese characters) matched before the first one that wasn't heard. */
export const heardLength = (phrase: string, heard: string) => {
  const said = [...heard.toLowerCase()].filter(isWordChar).join("");
  let from = 0;
  let cut = 0;
  for (const [i, c] of [...phrase].entries()) {
    if (!isWordChar(c)) continue;
    const at = said.indexOf(c.toLowerCase(), from);
    if (at < 0) break;
    from = at + 1;
    cut = i + 1;
  }
  return cut;
};

/** Dictate a sentence into a box: the shortcut lights up while held, the
 * waveform follows the mic, and the sentence fills in as Owl hears it.
 * `onDone` reports once a dictation has landed in the box. */
export const TryStep: React.FC<{ onDone: (done: boolean) => void }> = ({
  onDone,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const osType = useOsType();
  const { models, downloadProgress } = useModelStore();
  const [text, setText] = useState("");
  const [live, setLive] = useState("");
  // Latest mic levels for the waveform; a ref, so ~24 events a second don't
  // re-render the step.
  const levels = useRef<number[]>([]);
  const [listening, setListening] = useState(false);
  const lastLevel = useRef(0);
  const box = useRef<HTMLTextAreaElement>(null);

  const phrase = t("onboarding.try.phrase");
  const shortcut = formatKeyCombination(
    getSetting("bindings")?.transcribe?.current_binding ?? "",
    osType,
  );
  // Ready once the model picked earlier is downloaded and selected.
  const waitingFor = pendingModel();
  const selected = getSetting("selected_model");
  const ready = !waitingFor && !!selected;
  const percent = waitingFor ? downloadProgress[waitingFor]?.percentage : null;
  const modelName = models.find((m) => m.id === waitingFor)?.name;

  // Mic levels arrive only while Owl records: that's "listening".
  useEffect(() => {
    const unlisten = listen<number[]>("mic-level", (event) => {
      lastLevel.current = Date.now();
      levels.current = event.payload;
    });
    const timer = setInterval(
      () => setListening(Date.now() - lastLevel.current < 400),
      150,
    );
    return () => {
      unlisten.then((fn) => fn());
      clearInterval(timer);
    };
  }, []);

  // Words as they're heard, from models that stream.
  useEffect(() => {
    const unlisten = events.streamTextEvent.listen((event) =>
      setLive(`${event.payload.committed} ${event.payload.tentative}`),
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (listening) setLive("");
  }, [listening]);

  const done = !listening && text.trim().length > 0;
  useEffect(() => onDone(done), [done, onDone]);

  const heard = heardLength(phrase, listening ? live : text);
  const tryAgain = () => {
    setText("");
    box.current?.focus();
  };

  const status = !ready ? (
    <div className="w-full max-w-[460px] space-y-2 text-center">
      <div className="h-1 rounded-full bg-surface-raised overflow-hidden">
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-300"
          style={{ width: `${percent ?? 0}%` }}
        />
      </div>
      <p className="text-xs text-text-muted">
        {percent != null
          ? t("onboarding.try.downloading", {
              model: modelName,
              percent: Math.round(percent),
            })
          : t("onboarding.try.preparing")}
      </p>
    </div>
  ) : done ? (
    <div className="flex items-center gap-2 text-sm font-medium">
      <Check className="w-4 h-4 text-success-foreground" />
      <span className="text-success-foreground">
        {t("onboarding.try.works")}
      </span>
      <span className="text-text-muted">·</span>
      <button
        type="button"
        onClick={tryAgain}
        className="text-accent hover:underline cursor-pointer"
      >
        {t("onboarding.try.again")}
      </button>
    </div>
  ) : (
    <div className="flex items-center gap-2.5 text-sm">
      <span
        className={`flex items-center gap-1.5 px-3 py-1 rounded-lg border font-medium transition-colors ${
          listening
            ? "bg-accent-soft border-accent text-accent"
            : "bg-surface-raised border-border text-text"
        }`}
      >
        <Keyboard className="w-4 h-4" />
        {shortcut}
      </span>
      {listening ? (
        <>
          <Waveform levels={levels} ready />
          <span className="text-text-muted">{t("onboarding.try.letGo")}</span>
        </>
      ) : (
        <span className="text-text-muted">{t("onboarding.try.hold")}</span>
      )}
    </div>
  );

  return (
    <div className="w-full flex flex-col items-center gap-5">
      <div className="text-center space-y-1">
        <p className="text-xs font-medium text-text-muted">
          {t("onboarding.try.say")}
        </p>
        <p className="text-lg font-semibold">
          <span className={done ? "text-success-foreground" : "text-accent"}>
            {phrase.slice(0, done ? phrase.length : heard)}
          </span>
          <span className={heard || done ? "text-text/25" : "text-text"}>
            {done ? "" : phrase.slice(heard)}
          </span>
        </p>
      </div>
      <textarea
        ref={box}
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
        disabled={!ready}
        placeholder={t("onboarding.try.placeholder")}
        aria-label={t("onboarding.try.say")}
        className={`w-full max-w-[460px] h-[76px] resize-none px-3 py-2.5 text-sm rounded-lg bg-surface border focus:outline-none focus:ring-[3px] focus:ring-focus-ring disabled:opacity-50 ${
          done ? "border-success-foreground/60" : "border-accent"
        }`}
      />
      {status}
    </div>
  );
};
