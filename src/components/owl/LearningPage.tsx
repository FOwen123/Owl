import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { listen } from "@tauri-apps/api/event";
import { Sparkles } from "lucide-react";
import { toast } from "sonner";
import {
  commands,
  type Change,
  type LearnReport,
  type LearningOverview,
} from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Textarea } from "../ui/Textarea";
import { Button } from "../ui/Button";

const percent = (n: number | null | undefined) =>
  n == null ? "–" : (n * 100).toFixed(1);

/** One learned change as a sentence. */
const describe = (change: Change, t: TFunction) => {
  switch (change.kind) {
    case "add_word":
      return t("owl.learning.change.add_word", { word: change.word });
    case "add_swap":
    case "remove_swap":
      return t(`owl.learning.change.${change.kind}`, {
        from: change.from,
        to: change.to,
      });
    case "priority":
      return t("owl.learning.change.priority", {
        words: change.after.slice(0, 5).join(", "),
      });
    case "context":
      return t("owl.learning.change.context");
    case "word_limit":
      return t("owl.learning.change.word_limit", { count: change.after ?? 0 });
    case "examples":
      return t("owl.learning.change.examples");
  }
};

const Outcome: React.FC<{ report: LearnReport; min: number }> = ({
  report,
  min,
}) => {
  const { t } = useTranslation();
  return (
    <span>
      {t(`owl.learning.outcome.${report.outcome}`, {
        count: report.kept.length,
        tried: report.tried,
        before: percent(report.before),
        after: percent(report.after),
        min,
      })}
    </span>
  );
};

/** Owl: the learning log, the learner's changes and what it hears with. */
export const LearningPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("learning_enabled") ?? false;
  const [overview, setOverview] = useState<LearningOverview | null>(null);
  const [running, setRunning] = useState(false);
  const [context, setContext] = useState(getSetting("hearing_context") ?? "");
  const savedContext = getSetting("hearing_context") ?? "";

  const load = useCallback(async () => {
    const result = await commands.getLearningOverview();
    if (result.status === "ok") {
      setOverview(result.data);
      setRunning(result.data.running);
    }
  }, []);

  useEffect(() => {
    load();
    const unlisten = listen("learning-changed", load);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [load]);

  useEffect(() => setContext(savedContext), [savedContext]);

  const learnNow = async () => {
    setRunning(true);
    const result = await commands.runLearningNow();
    setRunning(false);
    if (result.status === "error") toast.error(result.error);
    load();
  };

  const undo = async (id: number) => {
    const result = await commands.undoLearnedChange(id);
    if (result.status === "error") toast.error(result.error);
    load();
  };

  const when = (secs: number) =>
    new Date(secs * 1000).toLocaleString(i18n.language, {
      dateStyle: "medium",
      timeStyle: "short",
    });

  const stats = overview?.stats;
  const min = overview?.min_keyed ?? 10;
  const ready = (stats?.keyed ?? 0) >= min;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("owl.learning.title")}>
        <ToggleSwitch
          checked={enabled}
          onChange={(on) => updateSetting("learning_enabled", on)}
          isUpdating={isUpdating("learning_enabled")}
          label={t("owl.learning.enableLabel")}
          description={t("owl.learning.enableDescription")}
          descriptionMode="inline"
          grouped
        />
      </SettingsGroup>

      {enabled && stats && (
        <SettingsGroup title={t("owl.learning.progressTitle")}>
          <div className="px-4 py-3 space-y-3 text-sm">
            <p>
              {t("owl.learning.stats", {
                keyed: stats.keyed,
                dictations: stats.dictations,
              })}
            </p>
            {!ready && (
              <p className="text-text-muted">
                {t("owl.learning.needMore", { min })}
              </p>
            )}
            {overview?.last_run_at != null && overview.last_report && (
              <p className="text-text-muted">
                {t("owl.learning.lastRun", {
                  when: when(overview.last_run_at),
                })}
                {" · "}
                <Outcome report={overview.last_report} min={min} />
              </p>
            )}
            <Button
              variant="primary-soft"
              size="sm"
              onClick={learnNow}
              disabled={!ready || running}
              className="inline-flex items-center gap-1.5"
            >
              <Sparkles className="w-3.5 h-3.5" />
              {running
                ? t("owl.learning.learning")
                : t("owl.learning.learnNow")}
            </Button>
          </div>
        </SettingsGroup>
      )}

      {enabled && (
        <SettingsGroup title={t("owl.learning.changesTitle")}>
          {overview && overview.changes.length > 0 ? (
            <ul className="divide-y divide-border">
              {overview.changes.map((c) => (
                <li
                  key={c.id}
                  className="flex items-center gap-3 px-4 py-2 text-sm"
                >
                  <span
                    className={`flex-1 min-w-0 break-words ${c.undone ? "text-text-muted line-through" : ""}`}
                  >
                    {describe(c.change, t)}
                  </span>
                  <span className="text-xs text-text-muted shrink-0">
                    {when(c.at)}
                  </span>
                  {c.undone ? (
                    <span className="text-xs text-text-muted shrink-0">
                      {t("owl.learning.undone")}
                    </span>
                  ) : (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => undo(c.id)}
                    >
                      {t("owl.learning.undo")}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className="px-4 py-3 text-sm text-text-muted">
              {t("owl.learning.changesEmpty")}
            </p>
          )}
        </SettingsGroup>
      )}

      <SettingsGroup
        title={t("owl.learning.contextTitle")}
        description={t("owl.learning.contextDescription")}
      >
        <div className="p-4">
          <Textarea
            value={context}
            onChange={(e) => setContext(e.target.value)}
            onBlur={() =>
              context !== savedContext &&
              updateSetting("hearing_context", context)
            }
            placeholder={t("owl.learning.contextPlaceholder")}
            aria-label={t("owl.learning.contextTitle")}
            rows={3}
            className="w-full"
          />
        </div>
        <ToggleSwitch
          checked={getSetting("cleanup_examples") ?? false}
          onChange={(on) => updateSetting("cleanup_examples", on)}
          isUpdating={isUpdating("cleanup_examples")}
          label={t("owl.learning.examplesLabel")}
          description={t("owl.learning.examplesDescription")}
          descriptionMode="inline"
          grouped
        />
      </SettingsGroup>
    </div>
  );
};
