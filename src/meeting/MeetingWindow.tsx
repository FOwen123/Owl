import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Square } from "lucide-react";
import { toast, Toaster } from "sonner";
import { commands, type Meeting, type Result } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { clock } from "@/components/owl/NotetakerPage";
import { useSpeakerLabel } from "@/components/owl/MeetingSpeakers";
import { useMeetings } from "@/components/owl/useMeetings";
import { ring } from "@/overlay/RecordingOverlay";
import { Owl, type OwlMood } from "@/overlay/Owl";
import { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";

type Mode = "prompt" | "live" | "ended";

/** The cards' width (`.mw-ask` in meeting.css). */
const CARD_WIDTH = 320;
const RING = ring(CARD_WIDTH);
/** How long "In a meeting?" waits before going away by itself. */
const ASK_SECONDS = 15;
/** How long "Meeting finished" waits for Resume (GRACE in meeting_detect.rs). */
const ENDED_SECONDS = 8;

/** The floating notetaker window (meeting_window.rs). */
export const MeetingWindow: React.FC = () => {
  const [mode, setMode] = useState<Mode | null>(null);
  // Counts each time Rust shows the window, so a card's countdown starts over.
  const [shows, setShows] = useState(0);
  const meetings = useMeetings();

  useEffect(() => {
    const unlisten = [
      // A hidden window keeps running, so read the language on every show.
      listen<Mode>("meeting-window-mode", async (e) => {
        await syncLanguageFromSettings();
        setMode(e.payload);
        setShows((n) => n + 1);
      }),
      listen("app-language-changed", syncLanguageFromSettings),
    ];
    return () => unlisten.forEach((p) => p.then((fn) => fn()));
  }, []);

  const recording = meetings.find((m) => m.status === "recording");
  // Until Rust says otherwise, the window opened for what's happening now.
  const shown = mode ?? (recording ? "live" : "prompt");

  return (
    <>
      {shown === "live" ? (
        recording ? (
          <Live meeting={recording} />
        ) : null
      ) : shown === "ended" ? (
        <Ended key={shows} />
      ) : (
        <Prompt key={shows} />
      )}
      <Toaster theme="system" position="bottom-center" />
    </>
  );
};

/** "In a meeting?", styled as the overlay's "Adding to dictionary" card. */
const Prompt: React.FC = () => {
  const { t } = useTranslation();
  const hide = () => getCurrentWindow().hide();
  return (
    <Card
      text={t("owl.meetingWindow.ask")}
      action={t("owl.meetingWindow.takeNotes")}
      onAction={commands.startMeeting}
      close={t("owl.meetingWindow.notNow")}
      onClose={hide}
      seconds={ASK_SECONDS}
      onDone={hide}
      mood="perked"
    />
  );
};

/** The call ended: the notes stop soon, unless this was a false alarm. */
const Ended: React.FC = () => {
  const { t } = useTranslation();
  return (
    <Card
      text={t("owl.meetingWindow.ended")}
      action={t("owl.meetingWindow.resume")}
      onAction={commands.resumeMeeting}
      close={t("owl.meetingWindow.finishNow")}
      onClose={commands.stopMeeting}
      seconds={ENDED_SECONDS}
      mood="drowsy"
    />
  );
};

/** Owl, question, a button and X, the edge counting down `seconds` like
 * the dictionary card's. */
const Card: React.FC<{
  text: string;
  action: string;
  onAction: () => Promise<Result<unknown, string> | void>;
  close: string;
  onClose: () => unknown;
  seconds: number;
  onDone?: () => void;
  mood: OwlMood;
}> = ({ text, action, onAction, close, onClose, seconds, onDone, mood }) => {
  const { i18n } = useTranslation();
  const [busy, setBusy] = useState(false);
  const act = async () => {
    setBusy(true);
    const result = await onAction();
    if (result?.status === "error") toast.error(result.error);
    setBusy(false);
  };
  return (
    <div dir={getLanguageDirection(i18n.language)} className="ov-stage">
      <div className="scard ssug mw-ask">
        <span className="ssug-book" aria-hidden="true">
          {/* Wide awake once you act, so Resume wakes a drowsy owl. */}
          <Owl mood={busy ? "perked" : mood} seconds={seconds} />
        </span>
        <span className="mw-ask-text">{text}</span>
        <Button size="sm" disabled={busy} onClick={act} className="mw-ask-btn">
          {action}
        </Button>
        <button className="sx" aria-label={close} onClick={onClose}>
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path
              d="M4 4 L12 12 M12 4 L4 12"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
            />
          </svg>
        </button>
        <svg
          className="ssug-ring"
          viewBox={`0 0 ${CARD_WIDTH} 52`}
          aria-hidden="true"
        >
          <path className="ssug-track" d={RING} pathLength={100} />
          <path
            className="ssug-countdown"
            d={RING}
            pathLength={100}
            style={{ animationDuration: `${seconds}s` }}
            onAnimationEnd={onDone}
          />
        </svg>
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
        <Owl mood="noting" beat={lines} />
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
