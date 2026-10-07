import React from "react";

interface SegmentedControlProps<T extends string> {
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
}

// DESIGN.md segmented control: pill track in surface-raised, the selected
// segment a bordered pill. For 2–4 mutually exclusive options.
export const SegmentedControl = <T extends string>({
  options,
  value,
  onChange,
  disabled = false,
}: SegmentedControlProps<T>) => (
  <div
    role="radiogroup"
    className={`inline-flex p-[3px] gap-0.5 rounded-full border border-border bg-surface-raised ${disabled ? "opacity-50" : ""}`}
  >
    {options.map((option) => {
      const selected = option.value === value;
      return (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={selected}
          disabled={disabled}
          onClick={() => onChange(option.value)}
          className={`px-3 py-0.5 rounded-full text-[13px] font-medium border transition-colors cursor-pointer focus:outline-none focus-visible:ring-[3px] focus-visible:ring-focus-ring ${
            selected
              ? "border-border bg-segment-selected text-text"
              : "border-transparent text-text-muted hover:text-text"
          }`}
        >
          {option.label}
        </button>
      );
    })}
  </div>
);
