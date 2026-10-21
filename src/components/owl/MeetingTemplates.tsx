import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2 } from "lucide-react";
import type { MeetingTemplate } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Dropdown } from "../ui/Dropdown";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import { SettingContainer } from "../ui/SettingContainer";
import { IconButton } from "../settings/history/HistorySettings";

/** Every template, built-in and the user's, for the template dropdowns. */
export const useTemplateOptions = (builtIn: string[]) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const own = getSetting("meeting_templates") ?? [];
  return [
    ...builtIn.map((id) => ({
      value: id,
      label: t(`owl.notetaker.templates.${id}`),
    })),
    ...own.map((tpl) => ({ value: tpl.id, label: tpl.name })),
  ];
};

/** The default template, and the user's own templates to write and edit. */
export const MeetingTemplates: React.FC<{ builtIn: string[] }> = ({
  builtIn,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const own = getSetting("meeting_templates") ?? [];
  const options = useTemplateOptions(builtIn);

  const save = (templates: MeetingTemplate[]) =>
    updateSetting("meeting_templates", templates);
  const edit = (id: string, change: MeetingTemplate) =>
    save(own.map((tpl) => (tpl.id === id ? change : tpl)));

  return (
    <SettingsGroup
      title={t("owl.notetaker.templates.title")}
      description={t("owl.notetaker.templates.description")}
    >
      <SettingContainer
        title={t("owl.notetaker.templates.default")}
        description={t("owl.notetaker.templates.defaultDescription")}
        descriptionMode="inline"
        grouped
      >
        <Dropdown
          options={options}
          selectedValue={getSetting("meeting_template") ?? "general"}
          onSelect={(value) => updateSetting("meeting_template", value)}
        />
      </SettingContainer>
      {own.map((tpl) => (
        <TemplateEditor
          key={tpl.id}
          template={tpl}
          onSave={(next) => edit(next.id, next)}
          onRemove={() => save(own.filter((x) => x.id !== tpl.id))}
        />
      ))}
      <div className="px-4 py-3">
        <Button
          variant="secondary"
          size="sm"
          className="inline-flex items-center gap-1.5"
          onClick={() =>
            save([
              ...own,
              {
                id: `own-${Date.now()}`,
                name: t("owl.notetaker.templates.untitled"),
                prompt: "",
              },
            ])
          }
        >
          <Plus className="w-3.5 h-3.5" />
          {t("owl.notetaker.templates.add")}
        </Button>
      </div>
    </SettingsGroup>
  );
};

/** One template's name and prompt. Saved when a field loses focus, not on
 * every keystroke: each save writes the settings file and rebuilds the menu
 * bar. */
const TemplateEditor: React.FC<{
  template: MeetingTemplate;
  onSave: (template: MeetingTemplate) => void;
  onRemove: () => void;
}> = ({ template, onSave, onRemove }) => {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(template);
  // A change from elsewhere replaces the draft; our own save matches it.
  useEffect(() => setDraft(template), [template.name, template.prompt]);
  const save = () => {
    if (draft.name !== template.name || draft.prompt !== template.prompt)
      onSave(draft);
  };
  return (
    <div className="px-4 py-3 space-y-2">
      <div className="flex items-center gap-2">
        <Input
          variant="compact"
          value={draft.name}
          placeholder={t("owl.notetaker.templates.name")}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          onBlur={save}
          className="flex-1"
        />
        <IconButton
          title={t("owl.notetaker.templates.remove")}
          onClick={onRemove}
        >
          <Trash2 width={16} height={16} />
        </IconButton>
      </div>
      <Textarea
        value={draft.prompt}
        placeholder={t("owl.notetaker.templates.prompt")}
        onChange={(e) => setDraft({ ...draft, prompt: e.target.value })}
        onBlur={save}
        rows={4}
        className="w-full"
      />
    </div>
  );
};
