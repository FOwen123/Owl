import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { ArrowLeft, Copy, Sparkles, Square, Circle } from "lucide-react";
import { commands, type Meeting } from "@/bindings";
import { formatDateTime } from "@/utils/dateFormat";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Button } from "../ui/Button";
import { Alert } from "../ui/Alert";
import { MarkdownContent } from "../whats-new/MarkdownContent";
import {
  SpeakerChips,
  SpeakerModelRow,
  useFillSpeakers,
  useSpeakerLabel,
} from "./MeetingSpeakers";

const clock = (ms: number) => {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
};

const when = (m: Meeting, locale: string) =>
  formatDateTime(String(Math.floor(m.started_at / 1000)), locale);

export const NotetakerPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [openId, setOpenId] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const result = await commands.getMeetings();
    if (result.status === "ok") setMeetings(result.data);
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen("meetings-updated", refresh);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const recording = meetings.find((m) => m.status === "recording");
  useEffect(() => {
    if (!recording) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [recording]);

  const run = async (
    action: () => Promise<{ status: string; error?: string }>,
  ) => {
    setBusy(true);
    const result = await action();
    if (result.status === "error") toast.error(result.error);
    setBusy(false);
  };

  const open = meetings.find((m) => m.id === openId);
  if (open) {
    return (
      <MeetingDetail
        meeting={open}
        locale={i18n.language}
        onBack={() => setOpenId(null)}
      />
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("owl.notetaker.title")}>
        <div className="flex items-center justify-between gap-4 p-4">
          {recording ? (
            <div className="flex items-center gap-2 text-sm font-medium">
              <Circle className="w-3 h-3 fill-logo-primary text-logo-primary animate-pulse" />
              {t("owl.notetaker.recording")}
              <span className="font-mono text-mid-gray">
                {clock(now - recording.started_at)}
              </span>
            </div>
          ) : (
            <p className="text-sm text-mid-gray">
              {t("owl.notetaker.description")}
            </p>
          )}
          {recording ? (
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => run(commands.stopMeeting)}
              className="inline-flex items-center gap-2 shrink-0"
            >
              <Square className="w-3.5 h-3.5" />
              {t("owl.notetaker.stop")}
            </Button>
          ) : (
            <Button
              disabled={busy}
              onClick={() => run(commands.startMeeting)}
              className="shrink-0"
            >
              {t("owl.notetaker.start")}
            </Button>
          )}
        </div>
        <SpeakerModelRow />
      </SettingsGroup>

      <SettingsGroup title={t("owl.notetaker.meetings")}>
        {meetings.length === 0 ? (
          <p className="p-4 text-sm text-mid-gray">
            {t("owl.notetaker.empty")}
          </p>
        ) : (
          meetings.map((m) => (
            <button
              key={m.id}
              type="button"
              disabled={m.status === "recording"}
              onClick={() => setOpenId(m.id)}
              className="w-full flex items-center justify-between gap-4 px-4 py-3 text-start hover:bg-mid-gray/10 disabled:cursor-default cursor-pointer"
            >
              <div className="min-w-0">
                <p className="text-sm font-medium">{when(m, i18n.language)}</p>
                <p className="text-xs text-mid-gray truncate">
                  {m.transcript[0]?.text ??
                    t(`owl.notetaker.status.${m.status}`)}
                </p>
              </div>
              <span className="text-xs text-mid-gray shrink-0">
                {m.status === "ready"
                  ? clock(m.duration_ms)
                  : t(`owl.notetaker.status.${m.status}`)}
              </span>
            </button>
          ))
        )}
      </SettingsGroup>
    </div>
  );
};

const MeetingDetail: React.FC<{
  meeting: Meeting;
  locale: string;
  onBack: () => void;
}> = ({ meeting, locale, onBack }) => {
  const { t } = useTranslation();
  const speaker = useSpeakerLabel(meeting.speaker_names);
  const fillSpeakers = useFillSpeakers(meeting.speaker_names);
  const transcriptText = meeting.transcript
    .map((l) => `[${clock(l.start_ms)}] ${speaker(l)}: ${l.text}`)
    .join("\n");

  const summarize = async () => {
    const result = await commands.summarizeMeeting(meeting.id);
    if (result.status === "error" && result.error === "no_provider") {
      toast(t("owl.notetaker.summaryNeedsProvider"), {
        action: {
          label: t("owl.notetaker.setUpAi"),
          onClick: () => emit("show-section", "postprocessing"),
        },
      });
    } else if (result.status === "error") {
      toast.error(result.error);
    }
  };

  const remove = async () => {
    const result = await commands.deleteMeeting(meeting.id);
    if (result.status === "error") toast.error(result.error);
    else onBack();
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="flex items-center justify-between gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={onBack}
          className="inline-flex items-center gap-1"
        >
          <ArrowLeft className="w-4 h-4" />
          {t("owl.notetaker.back")}
        </Button>
        <Button variant="danger-ghost" size="sm" onClick={remove}>
          {t("owl.notetaker.delete")}
        </Button>
      </div>

      <div className="px-4">
        <h1 className="text-lg font-semibold">{when(meeting, locale)}</h1>
        <p className="text-xs text-mid-gray flex flex-wrap items-center gap-2">
          {clock(meeting.duration_ms)}
          <SpeakerChips meeting={meeting} />
        </p>
      </div>

      {meeting.status === "failed" && (
        <Alert variant="error">{meeting.error}</Alert>
      )}
      {meeting.error === "system_audio_missing" && (
        <Alert variant="warning">{t("owl.notetaker.systemAudioMissing")}</Alert>
      )}
      {meeting.error === "mic_failed" && (
        <Alert variant="warning">{t("owl.notetaker.micFailed")}</Alert>
      )}

      {meeting.status === "ready" && (
        <SettingsGroup title={t("owl.notetaker.summary")}>
          <div className="p-4 space-y-3">
            {meeting.summary && (
              <MarkdownContent markdown={fillSpeakers(meeting.summary)} />
            )}
            <Button
              variant={meeting.summary ? "secondary" : "primary"}
              size="sm"
              disabled={meeting.summarizing || meeting.transcript.length === 0}
              onClick={summarize}
              className="inline-flex items-center gap-1.5"
            >
              <Sparkles className="w-3.5 h-3.5" />
              {meeting.summarizing
                ? t("owl.notetaker.summarizing")
                : meeting.summary
                  ? t("owl.notetaker.resummarize")
                  : t("owl.notetaker.summarize")}
            </Button>
          </div>
        </SettingsGroup>
      )}

      <SettingsGroup title={t("owl.notetaker.transcript")}>
        {meeting.status === "transcribing" ? (
          <p className="p-4 text-sm text-mid-gray">
            {t("owl.notetaker.status.transcribing")}
          </p>
        ) : meeting.transcript.length === 0 ? (
          <p className="p-4 text-sm text-mid-gray">
            {t("owl.notetaker.noSpeech")}
          </p>
        ) : (
          <div className="p-4 space-y-3">
            <Button
              variant="secondary"
              size="sm"
              className="inline-flex items-center gap-1.5"
              onClick={async () => {
                await navigator.clipboard.writeText(transcriptText);
                toast.success(t("owl.notetaker.copied"));
              }}
            >
              <Copy className="w-3.5 h-3.5" />
              {t("owl.notetaker.copy")}
            </Button>
            <ul className="space-y-2">
              {meeting.transcript.map((line, i) => (
                <li key={i} className="flex gap-3 text-sm">
                  <span className="font-mono text-xs text-mid-gray w-12 shrink-0 pt-0.5">
                    {clock(line.start_ms)}
                  </span>
                  <span
                    className={`w-20 shrink-0 font-medium truncate ${line.speaker === "you" ? "text-logo-primary" : ""}`}
                  >
                    {speaker(line)}
                  </span>
                  <span className="flex-1 min-w-0">{line.text}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </SettingsGroup>
    </div>
  );
};
