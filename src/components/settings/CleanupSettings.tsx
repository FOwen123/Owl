import React from "react";
import { useTranslation } from "react-i18next";
import { SegmentedControl } from "../ui/SegmentedControl";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { CleanupLevel } from "@/bindings";

const LEVELS: CleanupLevel[] = ["none", "light", "medium"];

// Owl: None / Light / Medium, as in Wispr Flow's Auto Cleanup (TODO-CLEANUP.md).
export const CleanupLevelSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, refreshSettings } = useSettings();
  const level = getSetting("cleanup_level") ?? "light";

  return (
    <SettingContainer
      title={t("settings.cleanup.level.title")}
      description={t(`settings.cleanup.level.descriptions.${level}`)}
      descriptionMode="inline"
      grouped
    >
      <SegmentedControl
        options={LEVELS.map((value) => ({
          value,
          label: t(`settings.cleanup.level.options.${value}`),
        }))}
        value={level}
        onChange={async (value) => {
          await updateSetting("cleanup_level", value);
          // The backend also flips the filler-removal and AI switches.
          await refreshSettings();
        }}
      />
    </SettingContainer>
  );
};

export const VoiceCommandsSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("voice_commands_enabled") ?? false}
      onChange={(enabled) => updateSetting("voice_commands_enabled", enabled)}
      isUpdating={isUpdating("voice_commands_enabled")}
      label={t("settings.cleanup.voiceCommands.label")}
      description={t("settings.cleanup.voiceCommands.description")}
      descriptionMode="inline"
      grouped
    />
  );
};
