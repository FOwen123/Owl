import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  ArrowLeft,
  Copy,
  Sparkles,
  Square,
  Circle,
  MoreHorizontal,
  Download,
} from "lucide-react";
import {
  commands,
  type ExportFormat,
  type Meeting,
  type SummaryOptions,
} from "@/bindings";
import { formatDateTime } from "@/utils/dateFormat";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Button } from "../ui/Button";
import { Alert } from "../ui/Alert";
import { Dropdown } from "../ui/Dropdown";
import { MarkdownContent } from "../whats-new/MarkdownContent";
import {
  SpeakerChips,
  SpeakerModelRow,
  useFillSpeakers,
  useSpeakerLabel,
} from "./MeetingSpeakers";
import { MeetingTemplates, useTemplateOptions } from "./MeetingTemplates";
import { useSettings } from "../../hooks/useSettings";

const clock = (ms: number) => {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
};

const when = (m: Meeting, locale: string) =>
  formatDateTime(String(Math.floor(m.started_at / 1000)), locale);

/** What Import accepts; Owl can't decode Opus (WebM, .opus) yet. */
const AUDIO_AND_VIDEO = [
  "mp3",
  "m4a",
  "aac",
  "wav",
  "flac",
  "ogg",
  "mp4",
  "mov",
  "m4v",
];

const EXPORT_FORMATS: { format: ExportFormat; extension: string }[] = [
  { format: "markdown", extension: "md" },
  { format: "text", extension: "txt" },
  { format: "srt", extension: "srt" },
  { format: "vtt", extension: "vtt" },
];

/** The notes' first sentence, without Markdown, for the meeting list. */
const firstLine = (markdown: string) =>
  markdown
    .split("\n")
    .map((l) => l.replace(/^[#>*\-\s]+/, "").replace(/[*_`]/g, ""))
    .find((l) => l.trim()) ?? "";

export const NotetakerPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [openId, setOpenId] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const [busy, setBusy] = useState(false);
  const [options, setOptions] = useState<SummaryOptions>({
    built_in_templates: ["general"],
    providers: [],
  });

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
    commands.getSummaryOptions().then(setOptions);
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

  if (recording) {
    return (
      <LiveMeeting
        meeting={recording}
        elapsed={now - recording.started_at}
        busy={busy}
        onStop={() => run(commands.stopMeeting)}
      />
    );
  }

  const importFile = async () => {
    const path = await open({
      multiple: false,
      filters: [
        {
          name: t("owl.notetaker.audioOrVideo"),
          extensions: AUDIO_AND_VIDEO,
        },
      ],
    });
    if (typeof path !== "string") return;
    const result = await commands.importMeeting(path);
    if (result.status === "error") {
      toast.error(
        result.error === "unsupported_format"
          ? t("owl.notetaker.unsupportedFormat")
          : result.error,
      );
    }
  };

  const openMeeting = meetings.find((m) => m.id === openId);
  if (openMeeting) {
    return (
      <MeetingDetail
        meeting={openMeeting}
        options={options}
        locale={i18n.language}
        onBack={() => setOpenId(null)}
      />
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("owl.notetaker.title")}>
        <div className="flex items-center justify-between gap-4 p-4">
          <p className="text-sm text-mid-gray">
            {t("owl.notetaker.description")}
          </p>
          <div className="flex items-center gap-2 shrink-0">
            <Button variant="secondary" disabled={busy} onClick={importFile}>
              {t("owl.notetaker.import")}
            </Button>
            <Button disabled={busy} onClick={() => run(commands.startMeeting)}>
              {t("owl.notetaker.start")}
            </Button>
          </div>
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
            <MeetingRow
              key={m.id}
              meeting={m}
              locale={i18n.language}
              onOpen={() => setOpenId(m.id)}
            />
          ))
        )}
      </SettingsGroup>

      <MeetingTemplates builtIn={options.built_in_templates} />
    </div>
  );
};

/** While recording: the transcript as it arrives, with Stop. */
const LiveMeeting: React.FC<{
  meeting: Meeting;
  elapsed: number;
  busy: boolean;
  onStop: () => void;
}> = ({ meeting, elapsed, busy, onStop }) => {
  const { t } = useTranslation();
  const speaker = useSpeakerLabel(meeting.speaker_names);
  const end = useRef<HTMLDivElement>(null);
  const lines = meeting.transcript.length;
  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [lines]);
  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="flex items-center justify-between gap-4 px-4">
        <div className="flex items-center gap-2 text-sm font-medium">
          <Circle className="w-3 h-3 fill-logo-primary text-logo-primary animate-pulse" />
          {t("owl.notetaker.recording")}
          <span className="font-mono text-mid-gray">{clock(elapsed)}</span>
        </div>
        <Button
          variant="secondary"
          disabled={busy}
          onClick={onStop}
          className="inline-flex items-center gap-2 shrink-0"
        >
          <Square className="w-3.5 h-3.5" />
          {t("owl.notetaker.stop")}
        </Button>
      </div>
      <SettingsGroup title={t("owl.notetaker.transcript")}>
        {lines === 0 ? (
          <p className="p-4 text-sm text-mid-gray">
            {t("owl.notetaker.listening")}
          </p>
        ) : (
          <ul className="p-4 space-y-2">
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
        )}
        <div ref={end} />
      </SettingsGroup>
    </div>
  );
};

const MeetingRow: React.FC<{
  meeting: Meeting;
  locale: string;
  onOpen: () => void;
}> = ({ meeting: m, locale, onOpen }) => {
  const { t } = useTranslation();
  const fillSpeakers = useFillSpeakers(m.speaker_names);
  const preview = m.summary
    ? firstLine(fillSpeakers(m.summary))
    : (m.transcript[0]?.text ?? t(`owl.notetaker.status.${m.status}`));
  return (
    <button
      type="button"
      disabled={m.status === "recording"}
      onClick={onOpen}
      className="w-full flex items-center justify-between gap-4 px-4 py-3 text-start hover:bg-mid-gray/10 disabled:cursor-default cursor-pointer"
    >
      <div className="min-w-0">
        <p className="text-sm font-medium truncate">
          {m.title ?? when(m, locale)}
        </p>
        <p className="text-xs text-mid-gray truncate">{preview}</p>
      </div>
      <span className="text-xs text-mid-gray shrink-0 text-end">
        {m.title && <span className="block">{when(m, locale)}</span>}
        {m.status === "ready"
          ? clock(m.duration_ms)
          : t(`owl.notetaker.status.${m.status}`)}
      </span>
    </button>
  );
};

/** A button that opens a short list of actions; "⋯" unless given a label. */
const MoreMenu: React.FC<{
  label?: React.ReactNode;
  items: { label: string; danger?: boolean; onSelect: () => void }[];
}> = ({ label, items }) => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  return (
    <div ref={ref} className="relative">
      <Button
        variant="ghost"
        size="sm"
        aria-label={label ? undefined : t("owl.notetaker.more")}
        onClick={() => setOpen(!open)}
        className="inline-flex items-center gap-1.5"
      >
        {label ?? <MoreHorizontal className="w-4 h-4" />}
      </Button>
      {open && (
        <div className="absolute end-0 mt-1 min-w-40 z-20 py-1 rounded-lg border border-border bg-surface shadow-lg">
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className={`w-full px-3 py-1.5 text-start text-sm hover:bg-mid-gray/10 cursor-pointer ${item.danger ? "text-error" : ""}`}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

/** The meeting's title; click to rename. An empty title goes back to the date. */
const MeetingTitle: React.FC<{ meeting: Meeting; fallback: string }> = ({
  meeting,
  fallback,
}) => {
  const { t } = useTranslation();
  const [editing, setEditing] = useState(false);
  const save = async (title: string) => {
    setEditing(false);
    if (title.trim() === (meeting.title ?? "")) return;
    const result = await commands.renameMeeting(meeting.id, title);
    if (result.status === "error") toast.error(result.error);
  };
  if (editing) {
    return (
      <input
        autoFocus
        defaultValue={meeting.title ?? ""}
        placeholder={fallback}
        aria-label={t("owl.notetaker.rename")}
        className="w-full text-lg font-semibold bg-transparent border-b border-mid-gray/30 outline-none"
        onBlur={(e) => save(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setEditing(false);
        }}
      />
    );
  }
  return (
    <h1
      className="text-lg font-semibold cursor-text"
      title={t("owl.notetaker.rename")}
      onClick={() => setEditing(true)}
    >
      {meeting.title ?? fallback}
    </h1>
  );
};

const MeetingDetail: React.FC<{
  meeting: Meeting;
  options: SummaryOptions;
  locale: string;
  onBack: () => void;
}> = ({ meeting, options, locale, onBack }) => {
  const { t } = useTranslation();
  const speaker = useSpeakerLabel(meeting.speaker_names);
  const fillSpeakers = useFillSpeakers(meeting.speaker_names);
  const templates = useTemplateOptions(options.built_in_templates);
  const [template, setTemplate] = useState<string | null>(meeting.template);
  const [provider, setProvider] = useState<string | null>(null);
  const { getSetting } = useSettings();
  // Show what Summarize will use, and send exactly that.
  const shownTemplate =
    [template, getSetting("meeting_template")].find((id) =>
      templates.some((o) => o.value === id),
    ) ??
    templates[0]?.value ??
    null;
  const activeProvider = getSetting("post_process_provider_id");
  const shownProvider =
    provider ??
    options.providers.find((p) => p.id === activeProvider)?.id ??
    options.providers[0]?.id ??
    null;
  const transcriptText = meeting.transcript
    .map((l) => `[${clock(l.start_ms)}] ${speaker(l)}: ${l.text}`)
    .join("\n");

  const summarize = async () => {
    const result = await commands.summarizeMeeting(
      meeting.id,
      shownTemplate,
      shownProvider,
    );
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

  const exportAs = async (format: ExportFormat, extension: string) => {
    const name = (meeting.title ?? when(meeting, locale)).replace(
      /[/\\:*?"<>|]/g,
      "-",
    );
    const path = await save({
      defaultPath: `${name}.${extension}`,
      filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
    });
    if (!path) return;
    act(commands.exportMeeting(meeting.id, format, path), () =>
      toast.success(t("owl.notetaker.exported")),
    );
  };

  const act = async (
    action: Promise<{ status: string; error?: string }>,
    done?: () => void,
  ) => {
    const result = await action;
    if (result.status === "error") toast.error(result.error);
    else done?.();
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
        <div className="flex items-center gap-1">
          <MoreMenu
            label={
              <>
                <Download className="w-4 h-4" />
                {t("owl.notetaker.export")}
              </>
            }
            items={EXPORT_FORMATS.map(({ format, extension }) => ({
              label: t(`owl.notetaker.exportAs.${format}`),
              onSelect: () => exportAs(format, extension),
            }))}
          />
          <MoreMenu
            items={[
              {
                label: t("owl.notetaker.retranscribe"),
                onSelect: () => act(commands.retranscribeMeeting(meeting.id)),
              },
              {
                label: t("owl.notetaker.delete"),
                danger: true,
                onSelect: () => act(commands.deleteMeeting(meeting.id), onBack),
              },
            ]}
          />
        </div>
      </div>

      <div className="px-4 space-y-1">
        <MeetingTitle meeting={meeting} fallback={when(meeting, locale)} />
        <p className="text-xs text-mid-gray flex flex-wrap items-center gap-2">
          <span>
            {meeting.title && `${when(meeting, locale)} · `}
            {clock(meeting.duration_ms)}
          </span>
          <SpeakerChips meeting={meeting} />
        </p>
      </div>

      {meeting.status === "failed" && (
        <Alert variant="error">
          {meeting.error === "too_long"
            ? t("owl.notetaker.importTooLong")
            : meeting.error === "unsupported_format"
              ? t("owl.notetaker.unsupportedFormat")
              : meeting.error}
        </Alert>
      )}
      {meeting.status === "ready" &&
        meeting.error &&
        !["system_audio_missing", "mic_failed"].includes(meeting.error) && (
          <Alert variant="warning">{meeting.error}</Alert>
        )}
      {meeting.error === "system_audio_missing" && (
        <Alert variant="warning">{t("owl.notetaker.systemAudioMissing")}</Alert>
      )}
      {meeting.error === "mic_failed" && (
        <Alert variant="warning">{t("owl.notetaker.micFailed")}</Alert>
      )}

      {meeting.status === "ready" && (
        <SettingsGroup title={t("owl.notetaker.notes")}>
          <div className="p-4 space-y-3">
            {meeting.summary && (
              <MarkdownContent markdown={fillSpeakers(meeting.summary)} />
            )}
            <div className="flex flex-wrap items-center gap-2">
              <Dropdown
                options={templates}
                selectedValue={shownTemplate}
                onSelect={setTemplate}
              />
              {options.providers.length > 1 && (
                <Dropdown
                  options={options.providers.map((p) => ({
                    value: p.id,
                    label: p.label,
                    description: p.model,
                  }))}
                  selectedValue={shownProvider}
                  onSelect={setProvider}
                />
              )}
              <Button
                variant={meeting.summary ? "secondary" : "primary"}
                disabled={
                  meeting.summarizing || meeting.transcript.length === 0
                }
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
