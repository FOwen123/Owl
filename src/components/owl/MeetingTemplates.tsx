import React from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2 } from "lucide-react";
import type { MeetingTemplate } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Dropdown } from "../ui/Dropdown";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";

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
  const edit = (id: string, change: Partial<MeetingTemplate>) =>
    save(own.map((tpl) => (tpl.id === id ? { ...tpl, ...change } : tpl)));

  return (
    <SettingsGroup
      title={t("owl.notetaker.templates.title")}
      description={t("owl.notetaker.templates.description")}
    >
      <div className="flex items-center justify-between gap-4 px-4 py-3">
        <span className="text-sm">{t("owl.notetaker.templates.default")}</span>
        <Dropdown
          options={options}
          selectedValue={getSetting("meeting_template") ?? "general"}
          onSelect={(value) => updateSetting("meeting_template", value)}
          className="w-48"
        />
      </div>
      {own.map((tpl) => (
        <div key={tpl.id} className="px-4 py-3 space-y-2">
          <div className="flex items-center gap-2">
            <Input
              variant="compact"
              value={tpl.name}
              placeholder={t("owl.notetaker.templates.name")}
              onChange={(e) => edit(tpl.id, { name: e.target.value })}
              className="flex-1"
            />
            <Button
              variant="ghost"
              size="sm"
              aria-label={t("owl.notetaker.templates.remove")}
              onClick={() => save(own.filter((x) => x.id !== tpl.id))}
            >
              <Trash2 className="w-4 h-4" />
            </Button>
          </div>
          <Textarea
            value={tpl.prompt}
            placeholder={t("owl.notetaker.templates.prompt")}
            onChange={(e) => edit(tpl.id, { prompt: e.target.value })}
            rows={4}
            className="w-full"
          />
        </div>
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
