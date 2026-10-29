import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { Button } from "../ui/Button";
import { OwlMark } from "../icons/OwlLogo";
import { ModelStep, pendingModel } from "./ModelStep";
import { PermissionsStep } from "./PermissionsStep";
import { TryStep } from "./TryStep";
import {
  AiStep,
  MeetingsStep,
  ShortcutStep,
  WelcomeStep,
  type Uses,
} from "./Steps";

export type Step =
  | "welcome"
  | "model"
  | "permissions"
  | "shortcut"
  | "try"
  | "ai"
  | "meetings";
type Progress = Uses & { step: Step };

// Saved on every step, so quitting midway resumes where the user was.
const KEY = "owl-onboarding";
export const savedProgress = (): Progress | null => {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "null");
  } catch {
    return null;
  }
};

// Shortcut comes after Permissions: recording one needs Accessibility.
const stepsFor = (uses: Uses): Step[] => [
  "welcome",
  "model",
  "permissions",
  ...(uses.dictation ? (["shortcut", "try"] as const) : []),
  "ai",
  ...(uses.meetings ? (["meetings"] as const) : []),
];

/** First-run setup, one step per screen. `only` shows a single step (a
 * returning user missing a permission); `preview` (Debug page) changes
 * nothing. */
const Onboarding: React.FC<{
  onDone: () => void;
  only?: Step;
  start?: Step;
  preview?: boolean;
}> = ({ onDone, only, start, preview = false }) => {
  const { t, i18n } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [progress, setProgress] = useState<Progress>(
    () =>
      (!only && !preview && savedProgress()) || {
        step: only ?? start ?? "welcome",
        dictation: true,
        meetings: true,
      },
  );
  const [name, setName] = useState(getSetting("user_name") ?? "");
  const [chosen, setChosen] = useState<string | null>(
    pendingModel() ?? (getSetting("selected_model") || null),
  );
  const [ready, setReady] = useState(false);

  const steps = only ? [only] : stepsFor(progress);
  const index = steps.indexOf(progress.step);
  const last = index === steps.length - 1;

  const go = (step: Step, next: Progress = progress) => {
    const updated = { ...next, step };
    setProgress(updated);
    setReady(false);
    if (!preview && !only) localStorage.setItem(KEY, JSON.stringify(updated));
  };

  // shortcut: the backend marks onboarding done only when the model is
  // selected, so quitting Owl before the picked model finishes downloading
  // starts setup over; mark it done at Finish from the backend if that bites.
  const finish = () => {
    if (!preview) localStorage.removeItem(KEY);
    onDone();
  };

  const forward = () => {
    if (progress.step === "welcome" && !preview)
      updateSetting("user_name", name.trim());
    if (last) finish();
    else go(steps[index + 1]);
  };

  // Whether Continue is available on this step.
  const canContinue = {
    welcome:
      preview || (!!name.trim() && (progress.dictation || progress.meetings)),
    model: preview || chosen !== null,
    permissions: preview || ready,
    shortcut: true,
    try: ready,
    ai: ready,
    meetings: true,
  }[progress.step];
  // Optional steps offer Skip until they're done, then Continue.
  const skipping =
    (progress.step === "try" || progress.step === "ai") && !canContinue;

  const content = {
    welcome: (
      <WelcomeStep
        name={name}
        onName={setName}
        uses={progress}
        onUses={(uses) => go("welcome", { ...progress, ...uses })}
        onSubmit={() => canContinue && forward()}
      />
    ),
    model: <ModelStep chosen={chosen} onChoose={setChosen} preview={preview} />,
    permissions: (
      <PermissionsStep
        meetings={progress.meetings}
        onReady={setReady}
        preview={preview}
      />
    ),
    shortcut: <ShortcutStep />,
    try: <TryStep onDone={setReady} />,
    ai: <AiStep onReady={setReady} />,
    meetings: <MeetingsStep />,
  }[progress.step];

  const heading = {
    welcome: ["onboarding.welcome.title", "onboarding.welcome.subtitle"],
    model: ["onboarding.model.title", "onboarding.model.subtitle"],
    permissions: [
      "onboarding.permissions.title",
      "onboarding.permissions.description",
    ],
    shortcut: ["onboarding.shortcut.title", "onboarding.shortcut.subtitle"],
    try: ["onboarding.try.title", "onboarding.try.subtitle"],
    ai: ["onboarding.ai.title", "onboarding.ai.subtitle"],
    meetings: ["onboarding.meetings.title", "onboarding.meetings.subtitle"],
  }[progress.step];

  return (
    <div
      dir={getLanguageDirection(i18n.language)}
      className="h-screen flex flex-col select-none cursor-default"
    >
      <div data-tauri-drag-region className="titlebar-spacer shrink-0" />
      <main className="flex-1 min-h-0 overflow-y-auto flex flex-col px-12 pb-4">
        <div
          className={`my-auto w-full flex flex-col items-center gap-6 py-4 ${preview ? "pointer-events-none" : ""}`}
        >
          {progress.step === "welcome" && <OwlMark size={56} />}
          <div className="text-center space-y-1.5 max-w-[460px]">
            <h1 className="text-xl font-semibold">{t(heading[0])}</h1>
            <p className="text-sm text-text-muted">{t(heading[1])}</p>
          </div>
          {content}
        </div>
      </main>
      <footer className="h-[60px] shrink-0 flex items-center justify-between px-5 border-t border-border">
        <div className="flex gap-1.5" aria-hidden>
          {steps.length > 1 &&
            steps.map((step, i) => (
              <span
                key={step}
                className={`w-1.5 h-1.5 rounded-full ${i === index ? "bg-accent" : "bg-text/15"}`}
              />
            ))}
        </div>
        <div className="flex gap-2">
          {index > 0 && (
            <Button variant="secondary" onClick={() => go(steps[index - 1])}>
              {t("onboarding.nav.back")}
            </Button>
          )}
          {skipping ? (
            <Button variant="secondary" onClick={forward}>
              {t("onboarding.nav.skip")}
            </Button>
          ) : (
            <Button disabled={!canContinue} onClick={forward}>
              {last ? t("onboarding.nav.finish") : t("onboarding.nav.continue")}
            </Button>
          )}
        </div>
      </footer>
    </div>
  );
};

export default Onboarding;
