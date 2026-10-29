import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Mic, NotebookPen, X } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { SegmentedControl } from "../ui/SegmentedControl";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ShortcutInput } from "../settings/ShortcutInput";
import { ShortcutActivationSetting } from "../settings/ShortcutActivation";
import { PostProcessingSettingsApi } from "../settings/PostProcessingSettingsApi";
import { ChatGptAccount } from "../settings/PostProcessingSettingsApi/ChatGptAccount";
import { usePostProcessProviderState } from "../settings/PostProcessingSettingsApi/usePostProcessProviderState";
import { MeetingDetectionToggle } from "../owl/NotetakerPage";
import { SpeakerModelRow } from "../owl/MeetingSpeakers";
import { DefaultTemplateRow } from "../owl/MeetingTemplates";
import { Owl } from "@/overlay/Owl";

export type Uses = { dictation: boolean; meetings: boolean };

/** A use Owl can be set up for, picked like a checkbox. */
const UseCard: React.FC<{
  icon: LucideIcon;
  title: string;
  hint: string;
  selected: boolean;
  onToggle: () => void;
}> = ({ icon: Icon, title, hint, selected, onToggle }) => (
  <button
    type="button"
    aria-pressed={selected}
    onClick={onToggle}
    className={`flex-1 flex flex-col gap-2.5 p-3 rounded-xl border text-start cursor-pointer transition-colors focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-focus-ring ${
      selected
        ? "bg-accent-soft border-accent"
        : "bg-surface border-border hover:bg-surface-raised"
    }`}
  >
    <span className="flex items-start justify-between">
      <span
        className={`p-1.5 rounded-lg ${selected ? "bg-accent-soft text-accent" : "bg-surface-raised text-text"}`}
      >
        <Icon className="w-4 h-4" />
      </span>
      {selected ? (
        <span className="w-[18px] h-[18px] rounded-full bg-accent-fill flex items-center justify-center">
          <Check className="w-3 h-3 text-accent-foreground" strokeWidth={3} />
        </span>
      ) : (
        <span className="w-4 h-4 rounded-full border border-text/20" />
      )}
    </span>
    <span>
      <span className="block text-sm font-medium">{title}</span>
      <span className="block text-xs text-text-muted">{hint}</span>
    </span>
  </button>
);

/** The logo, the user's name (meeting notes use it) and what Owl is for. */
export const WelcomeStep: React.FC<{
  name: string;
  onName: (name: string) => void;
  uses: Uses;
  onUses: (uses: Uses) => void;
  onSubmit: () => void;
}> = ({ name, onName, uses, onUses, onSubmit }) => {
  const { t } = useTranslation();
  return (
    <>
      <div className="w-[320px] space-y-1.5">
        <label
          htmlFor="onboarding-name"
          className="block text-xs font-medium text-text-muted"
        >
          {t("onboarding.welcome.name")}
        </label>
        <Input
          id="onboarding-name"
          autoFocus
          value={name}
          onChange={(e) => onName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && onSubmit()}
          placeholder={t("onboarding.welcome.namePlaceholder")}
          className="w-full"
        />
        <p className="text-xs text-text-muted">
          {t("onboarding.welcome.nameHint")}
        </p>
      </div>
      <div className="w-[320px] space-y-1.5">
        <p className="text-xs font-medium text-text-muted">
          {t("onboarding.welcome.useQuestion")}
        </p>
        <div className="flex gap-3">
          <UseCard
            icon={Mic}
            title={t("onboarding.welcome.dictation")}
            hint={t("onboarding.welcome.dictationHint")}
            selected={uses.dictation}
            onToggle={() => onUses({ ...uses, dictation: !uses.dictation })}
          />
          <UseCard
            icon={NotebookPen}
            title={t("onboarding.welcome.meetings")}
            hint={t("onboarding.welcome.meetingsHint")}
            selected={uses.meetings}
            onToggle={() => onUses({ ...uses, meetings: !uses.meetings })}
          />
        </div>
      </div>
    </>
  );
};

/** The Dictation page's shortcut rows. */
export const ShortcutStep: React.FC = () => (
  <div className="w-full max-w-[520px]">
    <SettingsGroup>
      <ShortcutInput shortcutId="transcribe" descriptionMode="inline" grouped />
      <ShortcutActivationSetting descriptionMode="inline" grouped />
    </SettingsGroup>
  </div>
);

/** ChatGPT, or the Post Process page's provider rows for the user's own key.
 * `onReady` reports whether one is set up; setting one up turns AI cleanup on. */
export const AiStep: React.FC<{ onReady: (ready: boolean) => void }> = ({
  onReady,
}) => {
  const { t } = useTranslation();
  const { updateSetting } = useSettings();
  const state = usePostProcessProviderState();
  const mode = state.selectedProviderId === "chatgpt" ? "chatgpt" : "key";
  const [signedIn, setSignedIn] = useState(false);

  const checkSignIn = () =>
    commands.chatgptStatus().then((s) => setSignedIn(s.signed_in));
  useEffect(() => {
    checkSignIn();
  }, []);

  const ready = mode === "chatgpt" ? signedIn : !!state.apiKey.trim();
  useEffect(() => {
    onReady(ready);
    if (ready) updateSetting("post_process_enabled", true);
  }, [ready, onReady, updateSetting]);

  return (
    <div className="w-full max-w-[520px] flex flex-col items-center gap-3">
      <SegmentedControl
        options={[
          { value: "chatgpt", label: t("onboarding.ai.chatgpt") },
          { value: "key", label: t("onboarding.ai.ownKey") },
        ]}
        value={mode}
        onChange={(next) =>
          state.handleProviderSelect(next === "chatgpt" ? "chatgpt" : "openai")
        }
      />
      <div className="w-full">
        <SettingsGroup>
          {mode === "chatgpt" ? (
            <SettingContainer
              title={t("settings.postProcessing.api.chatgpt.title")}
              description={t("onboarding.ai.chatgptDescription")}
              descriptionMode="inline"
              grouped
            >
              <ChatGptAccount
                onSignedIn={() => {
                  state.handleRefreshModels();
                  checkSignIn();
                }}
              />
            </SettingContainer>
          ) : (
            <PostProcessingSettingsApi />
          )}
        </SettingsGroup>
      </div>
    </div>
  );
};

/** The Notetaker page's rows, and the card Owl shows when a call starts. */
export const MeetingsStep: React.FC = () => {
  const { t } = useTranslation();
  const [builtIn, setBuiltIn] = useState(["general"]);
  useEffect(() => {
    commands.getSummaryOptions().then((o) => setBuiltIn(o.built_in_templates));
  }, []);

  return (
    <div className="w-full max-w-[520px] flex flex-col items-center gap-5">
      <div className="w-full">
        <SettingsGroup>
          <MeetingDetectionToggle />
          <SpeakerModelRow />
          <DefaultTemplateRow builtIn={builtIn} />
        </SettingsGroup>
      </div>
      <div
        className="flex flex-col items-center gap-2 pointer-events-none"
        aria-hidden
      >
        <p className="text-xs font-medium text-text-muted">
          {t("onboarding.meetings.preview")}
        </p>
        <div className="flex items-center gap-2.5 w-[320px] h-[52px] ps-2.5 pe-4 rounded-full bg-surface border-2 border-accent">
          <span className="w-8 h-8 shrink-0 rounded-full bg-accent-soft flex items-center justify-center">
            <Owl mood="perked" />
          </span>
          <span className="flex-1 text-sm font-medium">
            {t("owl.meetingWindow.ask")}
          </span>
          <Button size="sm" tabIndex={-1}>
            {t("owl.meetingWindow.takeNotes")}
          </Button>
          <span className="w-5 h-5 shrink-0 rounded-full bg-text/10 flex items-center justify-center">
            <X className="w-2.5 h-2.5 text-text-muted" strokeWidth={3} />
          </span>
        </div>
      </div>
    </div>
  );
};
