import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { commands, type ChatGptStatus } from "@/bindings";
import { Button } from "../../ui/Button";

/** Owl: Sign in with ChatGPT, shown instead of an API key for that provider. */
export const ChatGptAccount: React.FC<{ onSignedIn: () => void }> = ({
  onSignedIn,
}) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<ChatGptStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const load = () => commands.chatgptStatus().then(setStatus);
    load();
    const unlisten = listen("chatgpt-changed", load);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const signIn = async () => {
    setBusy(true);
    const result = await commands.chatgptSignIn();
    setBusy(false);
    if (result.status === "ok") {
      setStatus(result.data);
      onSignedIn();
    } else {
      toast.error(t("settings.postProcessing.api.chatgpt.failed"), {
        description: result.error,
      });
    }
  };

  const signOut = async () => {
    setBusy(true);
    await commands.chatgptSignOut();
    setBusy(false);
    setStatus({ signed_in: false, email: null });
  };

  if (!status) return null;
  return status.signed_in ? (
    <div className="flex items-center gap-3 text-sm">
      <span className="text-text-muted">
        {status.email
          ? t("settings.postProcessing.api.chatgpt.signedInAs", {
              email: status.email,
            })
          : t("settings.postProcessing.api.chatgpt.signedIn")}
      </span>
      <Button variant="secondary" size="sm" onClick={signOut} disabled={busy}>
        {t("settings.postProcessing.api.chatgpt.signOut")}
      </Button>
    </div>
  ) : (
    <Button variant="primary" size="sm" onClick={signIn} disabled={busy}>
      {busy
        ? t("settings.postProcessing.api.chatgpt.waiting")
        : t("settings.postProcessing.api.chatgpt.signIn")}
    </Button>
  );
};
