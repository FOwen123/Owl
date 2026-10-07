import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { PairListEditor } from "./PairListEditor";

export const SnippetsPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const snippets = getSetting("snippets") ?? [];

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup
        title={t("owl.snippets.title")}
        description={t("owl.snippets.description")}
      >
        <PairListEditor
          pairs={snippets.map((s) => [s.trigger, s.text])}
          onChange={(pairs) =>
            updateSetting(
              "snippets",
              pairs.map(([trigger, text]) => ({ trigger, text })),
            )
          }
          firstPlaceholder={t("owl.snippets.trigger")}
          secondPlaceholder={t("owl.snippets.text")}
          emptyText={t("owl.snippets.empty")}
          multiline
          disabled={isUpdating("snippets")}
        />
      </SettingsGroup>
    </div>
  );
};
