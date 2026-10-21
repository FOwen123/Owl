import React from "react";
import { useTranslation } from "react-i18next";
import {
  Cog,
  FlaskConical,
  GraduationCap,
  History,
  BookOpen,
  Info,
  Mic,
  NotebookPen,
  Scissors,
  Sparkles,
  Cpu,
} from "lucide-react";
import { OwlMark } from "./icons/OwlLogo";
import { DictionaryPage } from "./owl/DictionaryPage";
import { LearningPage } from "./owl/LearningPage";
import { NotetakerPage } from "./owl/NotetakerPage";
import { SnippetsPage } from "./owl/SnippetsPage";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  HistorySettings,
  DebugSettings,
  AboutSettings,
  PostProcessingSettings,
  ModelsSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component: React.ComponentType;
  enabled: (settings: any) => boolean;
  /** Owl: settings pages sit at the bottom of the sidebar. */
  bottom?: boolean;
}

export const SECTIONS_CONFIG = {
  general: {
    labelKey: "sidebar.general",
    icon: Mic,
    component: GeneralSettings,
    enabled: () => true,
  },
  notetaker: {
    labelKey: "sidebar.notetaker",
    icon: NotebookPen,
    component: NotetakerPage,
    enabled: () => true,
  },
  dictionary: {
    labelKey: "sidebar.dictionary",
    icon: BookOpen,
    component: DictionaryPage,
    enabled: () => true,
  },
  learning: {
    labelKey: "sidebar.learning",
    icon: GraduationCap,
    component: LearningPage,
    enabled: () => true,
  },
  snippets: {
    labelKey: "sidebar.snippets",
    icon: Scissors,
    component: SnippetsPage,
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  models: {
    bottom: true,
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  postprocessing: {
    bottom: true,
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: () => true,
  },
  advanced: {
    bottom: true,
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    enabled: () => true,
  },
  debug: {
    bottom: true,
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    bottom: true,
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({
      id: id as SidebarSection,
      ...(config as SectionConfig),
    }));

  const item = (section: (typeof availableSections)[number]) => {
    const Icon = section.icon;
    const isActive = activeSection === section.id;
    return (
      <button
        key={section.id}
        type="button"
        className={`flex gap-2.5 items-center px-2.5 py-1.5 w-full rounded-lg cursor-pointer transition-colors text-start focus:outline-none focus-visible:ring-[3px] focus-visible:ring-focus-ring ${
          isActive
            ? "bg-accent-soft text-accent"
            : "text-text hover:bg-surface-raised"
        }`}
        onClick={() => onSectionChange(section.id)}
      >
        <Icon
          width={18}
          height={18}
          className={`shrink-0 ${isActive ? "" : "text-text-muted"}`}
        />
        <span
          className="text-sm font-medium truncate"
          title={t(section.labelKey)}
        >
          {t(section.labelKey)}
        </span>
      </button>
    );
  };

  return (
    <nav className="flex flex-col w-48 h-full shrink-0 border-e border-border bg-background p-2 gap-1">
      {/* DESIGN.md: the signature gradient's one appearance on this screen. */}
      <div
        className="flex items-center gap-2 rounded-xl px-3 py-3 mb-2"
        style={{ background: "var(--gradient-signature)" }}
      >
        <OwlMark size={40} />
        {/* eslint-disable-next-line i18next/no-literal-string */}
        <span className="text-base font-semibold text-accent-foreground">
          Owl
        </span>
      </div>
      {availableSections.filter((s) => !s.bottom).map(item)}
      <div className="flex-1" />
      <div className="pt-2 border-t border-border flex flex-col gap-1">
        {availableSections.filter((s) => s.bottom).map(item)}
      </div>
    </nav>
  );
};
