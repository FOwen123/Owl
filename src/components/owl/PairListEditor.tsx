import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, X } from "lucide-react";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import { Button } from "../ui/Button";
import { IconButton } from "../settings/history/HistorySettings";

export type Pair = [string, string];

interface PairListEditorProps {
  pairs: Pair[];
  onChange: (pairs: Pair[]) => void;
  firstPlaceholder: string;
  secondPlaceholder: string;
  emptyText: string;
  /** Use a textarea for the second value (snippets can be several lines). */
  multiline?: boolean;
  disabled?: boolean;
}

// Shared editor for Owl's two "this → that" lists: snippets and dictionary
// replacements. Adding a pair whose first value already exists replaces it.
export const PairListEditor: React.FC<PairListEditorProps> = ({
  pairs,
  onChange,
  firstPlaceholder,
  secondPlaceholder,
  emptyText,
  multiline = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const [first, setFirst] = useState("");
  const [second, setSecond] = useState("");
  const canAdd = first.trim() !== "" && second.trim() !== "" && !disabled;

  const add = () => {
    if (!canAdd) return;
    const key = first.trim();
    const rest = pairs.filter(([a]) => a !== key);
    onChange([...rest, [key, multiline ? second : second.trim()]]);
    setFirst("");
    setSecond("");
  };

  const SecondField = multiline ? Textarea : Input;

  return (
    <>
      <div className="flex items-start gap-2 px-4 py-3">
        <Input
          variant="compact"
          className="w-40 shrink-0"
          value={first}
          placeholder={firstPlaceholder}
          onChange={(e) => setFirst(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          disabled={disabled}
        />
        <ArrowRight className="w-4 h-4 mt-1.5 shrink-0 text-mid-gray" />
        <SecondField
          variant="compact"
          className="flex-1 min-w-0"
          value={second}
          placeholder={secondPlaceholder}
          onChange={(
            e: React.ChangeEvent<HTMLInputElement & HTMLTextAreaElement>,
          ) => setSecond(e.target.value)}
          onKeyDown={(e: React.KeyboardEvent) =>
            e.key === "Enter" && !multiline && add()
          }
          disabled={disabled}
        />
        <Button onClick={add} disabled={!canAdd} size="md">
          {t("owl.pairs.add")}
        </Button>
      </div>

      {pairs.length === 0 ? (
        <p className="px-4 py-3 text-center text-text/60">{emptyText}</p>
      ) : (
        <ul className="divide-y divide-border">
          {pairs.map(([a, b]) => (
            <li key={a} className="flex items-start gap-3 px-4 py-2 text-sm">
              <span className="w-40 shrink-0 font-medium break-words">{a}</span>
              <ArrowRight className="w-4 h-4 mt-0.5 shrink-0 text-mid-gray" />
              <span className="flex-1 min-w-0 whitespace-pre-wrap break-words">
                {b}
              </span>
              {/* Pulled up so the button lines up with the text. */}
              <div className="-my-1 shrink-0">
                <IconButton
                  title={t("owl.pairs.remove", { item: a })}
                  onClick={() => onChange(pairs.filter(([x]) => x !== a))}
                  disabled={disabled}
                >
                  <X width={16} height={16} />
                </IconButton>
              </div>
            </li>
          ))}
        </ul>
      )}
    </>
  );
};
