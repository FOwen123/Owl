import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Square } from "lucide-react";
import { toast, Toaster } from "sonner";
import { commands, type Meeting } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { OwlMark } from "@/components/icons/OwlLogo";
import { clock } from "@/components/owl/NotetakerPage";
import { useSpeakerLabel } from "@/components/owl/MeetingSpeakers";

type Mode = "prompt" | "live";

/** The floating notetaker window (meeting_window.rs). */
export const MeetingWindow: React.FC = () => {
  const [mode, setMode] = useState<Mode | null>(null);
  const [meetings, setMeetings] = useState<Meeting[]>([]);

  // Replies can arrive out of order; only the newest request's counts.
  const latest = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++latest.current;
    const result = await commands.getMeetings();
    if (request === latest.current && result.status === "ok")
      setMeetings(result.data);
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = [
      listen("meetings-updated", refresh),
      listen<Mode>("meeting-window-mode", (e) => setMode(e.payload)),
    ];
    return () => unlisten.forEach((p) => p.then((fn) => fn()));
  }, [refresh]);

  const recording = meetings.find((m) => m.status === "recording");
  // Until Rust says otherwise, the window opened for what's happening now.
  const shown = mode ?? (recording ? "live" : "prompt");

  return (
    <>
      {shown === "live" ? (
        recording ? (
          <Live meeting={recording} />
        ) : null
      ) : (
        <Prompt />
      )}
      <Toaster theme="system" position="bottom-center" />
    </>
  );
};

const Prompt: React.FC = () => {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const start = async () => {
    setBusy(true);
    const result = await commands.startMeeting();
    if (result.status === "error") toast.error(result.error);
    setBusy(false);
  };
  return (
    <div data-tauri-drag-region className="mw-card mw-pop p-4 gap-3">
      <div data-tauri-drag-region className="flex items-center gap-3">
        <span className="mw-halo">
          <OwlMark size={32} />
        </span>
        <div data-tauri-drag-region className="min-w-0">
          <p className="text-sm font-medium">{t("owl.meetingWindow.ask")}</p>
          <p className="text-xs text-text-muted">
            {t("owl.meetingWindow.askDescription")}
          </p>
        </div>
      </div>
      <div className="flex justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => getCurrentWindow().hide()}
        >
          {t("owl.meetingWindow.notNow")}
        </Button>
        <Button size="sm" disabled={busy} onClick={start}>
          {t("owl.meetingWindow.takeNotes")}
        </Button>
      </div>
    </div>
  );
};

/** The transcript as it arrives, under a header that keeps Stop in reach. */
const Live: React.FC<{ meeting: Meeting }> = ({ meeting }) => {
  const { t } = useTranslation();
  const speaker = useSpeakerLabel(meeting.speaker_names);
  const [now, setNow] = useState(Date.now());
  const [busy, setBusy] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  const lines = meeting.transcript.length;

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [lines]);

  const stop = async () => {
    setBusy(true);
    const result = await commands.stopMeeting();
    if (result.status === "error") toast.error(result.error);
    setBusy(false);
  };

  return (
    <div className="mw-card mw-pop">
      <header
        data-tauri-drag-region
        className="flex items-center gap-3 px-4 py-3 border-b border-border"
      >
        <span className="mw-bars" aria-hidden>
          <i />
          <i />
          <i />
          <i />
          <i />
        </span>
        <span data-tauri-drag-region className="text-sm font-medium">
          {t("owl.notetaker.recording")}
        </span>
        <span
          data-tauri-drag-region
          className="font-mono text-xs text-text-muted"
        >
          {clock(now - meeting.started_at)}
        </span>
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          onClick={stop}
          className="ms-auto inline-flex items-center gap-1.5"
        >
          <Square className="w-3 h-3 fill-current" />
          {t("owl.notetaker.stop")}
        </Button>
      </header>

      <div className="flex-1 overflow-y-auto px-4 py-3">
        <ul className="space-y-3">
          {meeting.transcript.map((line, i) => (
            <li key={i} className="mw-line text-sm">
              <p className="text-xs text-text-muted flex gap-2">
                <span
                  className={`font-medium ${line.speaker === "you" ? "text-accent" : ""}`}
                >
                  {speaker(line)}
                </span>
                <span className="font-mono">{clock(line.start_ms)}</span>
              </p>
              <p>{line.text}</p>
            </li>
          ))}
        </ul>
        <p className="mw-listening text-xs mt-3">
          {t("owl.notetaker.listening")}
        </p>
        <div ref={end} />
      </div>
    </div>
  );
};
