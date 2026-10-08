import React from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight } from "lucide-react";
import { commands, type Replacement } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Button } from "../ui/Button";
import { CustomWords } from "../settings/CustomWords";
import { PairListEditor } from "./PairListEditor";

export const DictionaryPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const replacements = getSetting("replacements") ?? [];
  const suggestions = getSetting("edit_suggestions") ?? [];

  // Suggestions come from correction_watch.rs; Add moves one into replacements.
  const resolve = async (
    action: (suggestion: Replacement) => Promise<void>,
    suggestion: Replacement,
  ) => {
    await action(suggestion);
    await refreshSettings();
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      {suggestions.length > 0 && (
        <SettingsGroup
          title={t("owl.dictionary.suggestionsTitle")}
          description={t("owl.dictionary.suggestionsDescription")}
        >
          <ul className="divide-y divide-mid-gray/20">
            {suggestions.map((s) => (
              <li
                key={`${s.from}→${s.to}`}
                className="flex items-center gap-3 px-4 py-2 text-sm"
              >
                <span className="font-medium break-words">{s.from}</span>
                <ArrowRight className="w-4 h-4 shrink-0 text-mid-gray" />
                <span className="flex-1 min-w-0 break-words">{s.to}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => resolve(commands.dismissEditSuggestion, s)}
                >
                  {t("owl.dictionary.dismissSuggestion")}
                </Button>
                <Button
                  size="sm"
                  variant="primary-soft"
                  onClick={() => resolve(commands.acceptEditSuggestion, s)}
                >
                  {t("owl.dictionary.addSuggestion")}
                </Button>
              </li>
            ))}
          </ul>
        </SettingsGroup>
      )}

      <SettingsGroup title={t("owl.dictionary.wordsTitle")}>
        <CustomWords descriptionMode="inline" grouped />
      </SettingsGroup>

      <SettingsGroup
        title={t("owl.dictionary.replacementsTitle")}
        description={t("owl.dictionary.replacementsDescription")}
      >
        <PairListEditor
          pairs={replacements.map((r) => [r.from, r.to])}
          onChange={(pairs) =>
            updateSetting(
              "replacements",
              pairs.map(([from, to]) => ({ from, to })),
            )
          }
          firstPlaceholder={t("owl.dictionary.from")}
          secondPlaceholder={t("owl.dictionary.to")}
          emptyText={t("owl.dictionary.empty")}
          disabled={isUpdating("replacements")}
        />
        <ToggleSwitch
          checked={getSetting("learn_from_edits") ?? true}
          onChange={(enabled) => updateSetting("learn_from_edits", enabled)}
          isUpdating={isUpdating("learn_from_edits")}
          label={t("owl.dictionary.learnLabel")}
          description={t("owl.dictionary.learnDescription")}
          descriptionMode="inline"
          grouped
        />
      </SettingsGroup>
    </div>
  );
};
