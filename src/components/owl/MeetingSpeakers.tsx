import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { commands, type Meeting } from "@/bindings";
import { Button } from "../ui/Button";
import Badge from "../ui/Badge";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

type Line = Meeting["transcript"][number];
type Names = Meeting["speaker_names"];

/** The user's name from onboarding, else "You". */
const useYou = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  return getSetting("user_name")?.trim() || t("owl.notetaker.speaker.you");
};

/** The user, a speaker's name, "Speaker 2", or "Others" before speaker labels. */
export const useSpeakerLabel = (names: Names) => {
  const { t } = useTranslation();
  const you = useYou();
  return (line: Pick<Line, "speaker" | "voice">) => {
    if (line.speaker === "you") return you;
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
  const you = useYou();
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
      <Badge>{you}</Badge>
      {voices.map((voice) =>
        editing === voice ? (
          <Input
            key={voice}
            variant="compact"
            autoFocus
            defaultValue={meeting.speaker_names[voice] ?? ""}
            placeholder={t("owl.notetaker.speakerName")}
            className="w-32 text-xs! py-0.5!"
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
            className="rounded-full cursor-pointer focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-focus-ring"
          >
            <Badge variant="secondary" className="hover:text-text">
              {label({ speaker: "others", voice })}
            </Badge>
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
    <SettingContainer
      title={t("owl.notetaker.speakerModel.title")}
      description={t("owl.notetaker.speakerModel.description")}
      descriptionMode="inline"
      grouped
    >
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
    </SettingContainer>
  );
};
