import React from "react";
import { SettingContainer } from "./SettingContainer";

interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  isUpdating?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  tooltipPosition?: "top" | "bottom";
}

export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({
  checked,
  onChange,
  disabled = false,
  isUpdating = false,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  tooltipPosition = "top",
}) => {
  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      tooltipPosition={tooltipPosition}
    >
      <label
        className={`flex items-center ${disabled || isUpdating ? "cursor-not-allowed" : "cursor-pointer"}`}
      >
        <input
          type="checkbox"
          value=""
          className="sr-only peer"
          checked={checked}
          disabled={disabled || isUpdating}
          onChange={(e) => onChange(e.target.checked)}
        />
        {/* DESIGN.md toggle: 40×22 pill; on = accent fill with a white knob,
            off = raised surface with a border and a muted knob in dark mode. */}
        <div className="relative w-10 h-[22px] rounded-full border border-border bg-surface-raised transition-colors peer-focus-visible:ring-[3px] peer-focus-visible:ring-focus-ring peer-checked:bg-accent-fill peer-checked:border-accent-fill peer-disabled:opacity-50 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:h-4 after:w-4 after:rounded-full after:bg-toggle-knob peer-checked:after:bg-white after:transition-transform peer-checked:after:translate-x-[18px] rtl:peer-checked:after:-translate-x-[18px]"></div>
      </label>
      {isUpdating && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin"></div>
        </div>
      )}
    </SettingContainer>
  );
};
