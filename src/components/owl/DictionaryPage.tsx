import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { CustomWords } from "../settings/CustomWords";
import { PairListEditor } from "./PairListEditor";

export const DictionaryPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const replacements = getSetting("replacements") ?? [];

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
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
      </SettingsGroup>
    </div>
  );
};
