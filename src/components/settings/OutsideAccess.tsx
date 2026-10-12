import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { TextDisplay } from "../ui/TextDisplay";
import { ToggleSwitch } from "../ui/ToggleSwitch";

const API_URL = "http://127.0.0.1:8723/v1/audio/transcriptions";
const MCP_COMMAND = "claude mcp add owl -- owl mcp";

/** Owl: the `owl` command, the local transcription API and AI agents (MCP). */
export const OutsideAccess: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings } = useSettings();
  const [installedAt, setInstalledAt] = useState<string | null>(null);
  const [switching, setSwitching] = useState(false);
  const apiOn = getSetting("local_api_enabled") ?? false;

  const install = async () => {
    const result = await commands.installCommandLine();
    if (result.status === "ok") setInstalledAt(result.data);
    else toast.error(t("settings.advanced.outside.cli.failed"));
  };

  const switchApi = async (enabled: boolean) => {
    setSwitching(true);
    const result = await commands.changeLocalApiSetting(enabled);
    if (result.status === "error")
      toast.error(t("settings.advanced.outside.api.busy"));
    // Picks up the new token, or the switch staying off.
    await refreshSettings();
    setSwitching(false);
  };

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.outside.cli.title")}
        description={t("settings.advanced.outside.cli.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        {installedAt ? (
          <span className="text-xs text-text-muted">
            {t("settings.advanced.outside.cli.installed", {
              path: installedAt,
            })}
          </span>
        ) : (
          <Button variant="secondary" size="sm" onClick={install}>
            {t("settings.advanced.outside.cli.install")}
          </Button>
        )}
      </SettingContainer>
      <ToggleSwitch
        checked={apiOn}
        onChange={switchApi}
        isUpdating={switching}
        label={t("settings.advanced.outside.api.title")}
        description={t("settings.advanced.outside.api.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      {apiOn && (
        <>
          <TextDisplay
            label={t("settings.advanced.outside.api.address")}
            description={t("settings.advanced.outside.api.addressDescription")}
            value={API_URL}
            grouped={true}
            copyable={true}
            monospace={true}
          />
          <TextDisplay
            label={t("settings.advanced.outside.api.token")}
            description={t("settings.advanced.outside.api.tokenDescription")}
            value={getSetting("local_api_token") ?? ""}
            grouped={true}
            copyable={true}
            monospace={true}
          />
        </>
      )}
      <TextDisplay
        label={t("settings.advanced.outside.mcp.title")}
        description={t("settings.advanced.outside.mcp.description")}
        value={MCP_COMMAND}
        grouped={true}
        copyable={true}
        monospace={true}
      />
    </>
  );
};
