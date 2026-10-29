import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { platform } from "@tauri-apps/plugin-os";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  checkAccessibilityPermission,
  requestAccessibilityPermission,
  checkMicrophonePermission,
  requestMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import { toast } from "sonner";
import { Check, Keyboard, Loader2, Mic, Volume2 } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { commands } from "@/bindings";
import { useSettingsStore } from "@/stores/settingsStore";
import { Button } from "../ui/Button";
import { SettingsGroup } from "../ui/SettingsGroup";

type Status = "checking" | "needed" | "waiting" | "granted";
type Platform = "macos" | "windows" | "other";
const MAX_POLLING_ERRORS = 3;

// System Settings panes for "Not seeing a prompt?".
const PANE = {
  microphone: "Privacy_Microphone",
  accessibility: "Privacy_Accessibility",
  systemAudio: "Privacy_ScreenCapture",
};
const openPane = (pane: string) =>
  openUrl(`x-apple.systempreferences:com.apple.preference.security?${pane}`);

/** One permission: what it's for, and its button or state. */
const PermissionRow: React.FC<{
  icon: LucideIcon;
  title: string;
  description: string;
  children: React.ReactNode;
}> = ({ icon: Icon, title, description, children }) => (
  <div className="flex items-center gap-3 min-h-[52px] px-4 py-2.5">
    <div className="p-2 rounded-full bg-accent-soft shrink-0">
      <Icon className="w-4 h-4 text-accent" />
    </div>
    <div className="flex-1 min-w-0">
      <h3 className="text-sm font-medium">{title}</h3>
      <p className="text-xs text-text-muted">{description}</p>
    </div>
    <div className="shrink-0">{children}</div>
  </div>
);

/** Microphone and Accessibility (Owl needs both), plus System Audio when the
 * user takes meeting notes. `onReady` reports whether the required ones are
 * granted. */
export const PermissionsStep: React.FC<{
  meetings: boolean;
  onReady: (ready: boolean) => void;
  preview?: boolean;
}> = ({ meetings, onReady, preview = false }) => {
  const { t } = useTranslation();
  const refreshAudioDevices = useSettingsStore((s) => s.refreshAudioDevices);
  const refreshOutputDevices = useSettingsStore((s) => s.refreshOutputDevices);
  const [os, setOs] = useState<Platform | null>(null);
  const [mic, setMic] = useState<Status>("checking");
  const [ax, setAx] = useState<Status>("checking");
  const [systemAudio, setSystemAudio] = useState<"needed" | "asked">("needed");
  const polling = useRef<ReturnType<typeof setInterval> | null>(null);
  const errors = useRef(0);

  const ready = mic === "granted" && ax === "granted";
  useEffect(() => {
    onReady(ready);
    if (ready && !preview) {
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [ready, preview, onReady, refreshAudioDevices, refreshOutputDevices]);

  const windowsMicAllowed = useCallback(async () => {
    const status = await commands.getWindowsMicrophonePermissionStatus();
    return !status.supported || status.overall_access !== "denied";
  }, []);

  // Reads both permissions; once Accessibility is on, Owl can type and its
  // shortcut can start, which the next steps need.
  const check = useCallback(
    async (current: Platform) => {
      if (current === "windows") {
        setAx("granted");
        if (await windowsMicAllowed()) setMic("granted");
        return;
      }
      const [axOk, micOk] = await Promise.all([
        checkAccessibilityPermission(),
        checkMicrophonePermission(),
      ]);
      if (axOk) {
        setAx("granted");
        Promise.all([
          commands.initializeEnigo(),
          commands.initializeShortcuts(),
        ]).catch((e) => console.warn("Failed to initialize:", e));
      }
      if (micOk) setMic("granted");
      if (axOk && micOk && polling.current) {
        clearInterval(polling.current);
        polling.current = null;
      }
    },
    [windowsMicAllowed],
  );

  useEffect(() => {
    const p = platform();
    const current: Platform =
      p === "macos" ? "macos" : p === "windows" ? "windows" : "other";
    setOs(current);
    // Previews show the request UI without touching OS permissions.
    if (preview || current === "other") {
      setMic(preview ? "needed" : "granted");
      setAx(preview && current === "macos" ? "needed" : "granted");
      return;
    }
    check(current)
      .catch((error) => {
        console.error("Failed to check permissions:", error);
        toast.error(t("onboarding.permissions.errors.checkFailed"));
      })
      .finally(() => {
        setMic((s) => (s === "checking" ? "needed" : s));
        setAx((s) => (s === "checking" ? "needed" : s));
      });
    return () => {
      if (polling.current) clearInterval(polling.current);
    };
  }, [check, preview, t]);

  const startPolling = () => {
    if (polling.current || !os) return;
    polling.current = setInterval(() => {
      check(os)
        .then(() => (errors.current = 0))
        .catch((error) => {
          console.error("Error checking permissions:", error);
          if (++errors.current >= MAX_POLLING_ERRORS && polling.current) {
            clearInterval(polling.current);
            polling.current = null;
            toast.error(t("onboarding.permissions.errors.checkFailed"));
          }
        });
    }, 1000);
  };

  const ask = async (
    request: () => Promise<unknown>,
    set: (s: Status) => void,
  ) => {
    if (preview) return;
    try {
      await request();
      set("waiting");
      startPolling();
    } catch (error) {
      console.error("Failed to request permission:", error);
      toast.error(t("onboarding.permissions.errors.requestFailed"));
    }
  };

  const askSystemAudio = async () => {
    if (preview) return;
    const result = await commands.requestSystemAudioPermission();
    if (result.status === "error") toast.error(result.error);
    setSystemAudio("asked");
  };

  // The first permission still needed gets the primary button.
  const next =
    mic !== "granted"
      ? "microphone"
      : ax !== "granted"
        ? "accessibility"
        : null;
  const control = (status: Status, onGrant: () => void, isNext: boolean) => {
    if (status === "granted")
      return (
        <span className="flex items-center gap-1.5 text-sm font-medium text-success-foreground">
          <Check className="w-4 h-4" />
          {t("onboarding.permissions.granted")}
        </span>
      );
    if (status === "waiting" || status === "checking")
      return (
        <span className="flex items-center gap-1.5 text-sm text-text-muted">
          <Loader2 className="w-4 h-4 animate-spin" />
          {t("onboarding.permissions.waiting")}
        </span>
      );
    return (
      <Button
        size="sm"
        variant={isNext ? "primary" : "secondary"}
        onClick={onGrant}
      >
        {os === "windows"
          ? t("accessibility.openSettings")
          : t("onboarding.permissions.grant")}
      </Button>
    );
  };

  const missingPane =
    next ?? (meetings && systemAudio === "asked" ? "systemAudio" : null);

  return (
    <div className="w-full max-w-[520px] flex flex-col items-center gap-4">
      <div className="w-full">
        <SettingsGroup>
          <PermissionRow
            icon={Mic}
            title={t("onboarding.permissions.microphone.title")}
            description={t("onboarding.permissions.microphone.description")}
          >
            {control(
              mic,
              () =>
                ask(
                  os === "windows"
                    ? commands.openMicrophonePrivacySettings
                    : requestMicrophonePermission,
                  setMic,
                ),
              next === "microphone",
            )}
          </PermissionRow>
          {os === "macos" && (
            <PermissionRow
              icon={Keyboard}
              title={t("onboarding.permissions.accessibility.title")}
              description={t(
                "onboarding.permissions.accessibility.description",
              )}
            >
              {control(
                ax,
                () => ask(requestAccessibilityPermission, setAx),
                next === "accessibility",
              )}
            </PermissionRow>
          )}
          {os === "macos" && meetings && (
            <PermissionRow
              icon={Volume2}
              title={t("onboarding.permissions.systemAudio.title")}
              description={t("onboarding.permissions.systemAudio.description")}
            >
              {systemAudio === "asked" ? (
                <span className="flex items-center gap-1.5 text-sm text-text-muted">
                  <Check className="w-4 h-4" />
                  {t("onboarding.permissions.systemAudio.asked")}
                </span>
              ) : (
                <Button
                  size="sm"
                  variant={next === null ? "primary" : "secondary"}
                  onClick={askSystemAudio}
                >
                  {t("onboarding.permissions.grant")}
                </Button>
              )}
            </PermissionRow>
          )}
        </SettingsGroup>
      </div>
      {os === "macos" && missingPane && (
        <button
          type="button"
          onClick={() => openPane(PANE[missingPane])}
          className="text-xs font-medium text-accent hover:underline cursor-pointer"
        >
          {t("onboarding.permissions.openSettings")}
        </button>
      )}
    </div>
  );
};
