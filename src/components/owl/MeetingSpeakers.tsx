import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { commands, type Meeting } from "@/bindings";
import { Button } from "../ui/Button";

type Line = Meeting["transcript"][number];
type Names = Meeting["speaker_names"];

/** "You", a speaker's name, "Speaker 2", or "Others" before speaker labels. */
export const useSpeakerLabel = (names: Names) => {
  const { t } = useTranslation();
  return (line: Pick<Line, "speaker" | "voice">) => {
    if (line.speaker === "you") return t("owl.notetaker.speaker.you");
    if (line.voice == null) return t("owl.notetaker.speaker.others");
    return (
      names[line.voice] ?? t("owl.notetaker.speakerNumbered", { n: line.voice })
    );
  };
};

/** Notes written with `<@speaker:N>` placeholders, with the current names. */
export const useFillSpeakers = (names: Names) => {
  const label = useSpeakerLabel(names);
  return (text: string) =>
    text.replace(/<@speaker:(\d+)>/g, (_, n: string) =>
      label({ speaker: "others", voice: Number(n) }),
    );
};

/** The people in a meeting; click one to name them for the whole meeting. */
export const SpeakerChips: React.FC<{ meeting: Meeting }> = ({ meeting }) => {
  const { t } = useTranslation();
  const label = useSpeakerLabel(meeting.speaker_names);
  const [editing, setEditing] = useState<number | null>(null);
  const voices = [
    ...new Set(meeting.transcript.flatMap((l) => (l.voice ? [l.voice] : []))),
  ].sort((a, b) => a - b);
  if (voices.length === 0) return null;

  const save = async (voice: number, name: string) => {
    setEditing(null);
    const result = await commands.renameSpeaker(meeting.id, voice, name);
    if (result.status === "error") toast.error(result.error);
  };

  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      <span className="px-2 py-0.5 rounded-full bg-logo-primary/10 text-logo-primary">
        {t("owl.notetaker.speaker.you")}
      </span>
      {voices.map((voice) =>
        editing === voice ? (
          <input
            key={voice}
            autoFocus
            defaultValue={meeting.speaker_names[voice] ?? ""}
            placeholder={t("owl.notetaker.speakerName")}
            className="w-28 px-2 py-0.5 rounded-full border border-mid-gray/30 bg-transparent text-xs"
            onBlur={(e) => save(voice, e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") setEditing(null);
            }}
          />
        ) : (
          <button
            key={voice}
            type="button"
            onClick={() => setEditing(voice)}
            className="px-2 py-0.5 rounded-full bg-mid-gray/10 hover:bg-mid-gray/20 cursor-pointer"
          >
            {label({ speaker: "others", voice })}
          </button>
        ),
      )}
    </span>
  );
};

/** Offers the speaker model once; hidden when it's installed. */
export const SpeakerModelRow: React.FC = () => {
  const { t } = useTranslation();
  const [installed, setInstalled] = useState(true);
  const [percent, setPercent] = useState<number | null>(null);

  useEffect(() => {
    commands.speakerModelInstalled().then(setInstalled);
    const unlisten = listen<{ downloaded: number; total: number }>(
      "speaker-model-progress",
      (e) =>
        setPercent(Math.floor((e.payload.downloaded / e.payload.total) * 100)),
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
  if (installed) return null;

  const download = async () => {
    setPercent(0);
    const result = await commands.downloadSpeakerModel();
    setPercent(null);
    if (result.status === "error") toast.error(result.error);
    else setInstalled(true);
  };

  return (
    <div className="flex items-center justify-between gap-4 p-4">
      <div className="min-w-0">
        <p className="text-sm font-medium">
          {t("owl.notetaker.speakerModel.title")}
        </p>
        <p className="text-xs text-mid-gray">
          {t("owl.notetaker.speakerModel.description")}
        </p>
      </div>
      <Button
        variant="secondary"
        size="sm"
        disabled={percent !== null}
        onClick={download}
        className="shrink-0"
      >
        {percent === null
          ? t("owl.notetaker.speakerModel.download")
          : t("owl.notetaker.speakerModel.downloading", { percent })}
      </Button>
    </div>
  );
};
