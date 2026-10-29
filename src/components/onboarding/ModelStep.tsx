import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ChevronDown } from "lucide-react";
import type { ModelInfo } from "@/bindings";
import type { ModelCardStatus } from "./ModelCard";
import ModelCard, { isLegacySource } from "./ModelCard";
import { useModelStore } from "../../stores/modelStore";

// The model picked during onboarding, kept until it's downloaded and selected,
// so the download can finish after onboarding (or a restart).
const PENDING = "owl-onboarding-model";
export const pendingModel = () => localStorage.getItem(PENDING);

/** Selects the onboarding's model once its download finishes. Mounted in App,
 * so it works while the user carries on with setup and after it ends. */
export const usePendingModelSelection = () => {
  const { t } = useTranslation();
  const {
    models,
    downloadingModels,
    verifyingModels,
    extractingModels,
    selectModel,
  } = useModelStore();

  useEffect(() => {
    const id = pendingModel();
    const model = models.find((m) => m.id === id);
    if (
      !id ||
      !model?.is_downloaded ||
      id in downloadingModels ||
      id in verifyingModels ||
      id in extractingModels
    )
      return;
    localStorage.removeItem(PENDING);
    selectModel(id).then((ok) => {
      if (!ok) toast.error(t("onboarding.errors.selectModel"));
    });
  }, [
    models,
    downloadingModels,
    verifyingModels,
    extractingModels,
    selectModel,
    t,
  ]);
};

/** Pick a transcription model; its download carries on in the background.
 * `chosen` is the picked model, `onChoose` reports a new pick (null: cancelled). */
export const ModelStep: React.FC<{
  chosen: string | null;
  onChoose: (id: string | null) => void;
  preview?: boolean;
}> = ({ chosen, onChoose, preview = false }) => {
  const { t } = useTranslation();
  const {
    models,
    downloadModel,
    downloadingModels,
    verifyingModels,
    extractingModels,
    downloadProgress,
    downloadStats,
    cancelDownload,
  } = useModelStore();
  const [showAll, setShowAll] = useState(false);

  // Legacy (.bin/ONNX) downloads are deprecated and never offered here. The
  // catalog arrives rank-sorted, so the first two recommended models are the
  // featured picks; everything else hides behind "Show all".
  const { downloaded, downloadable, topPicks, rest } = useMemo(() => {
    const downloadable = models.filter(
      (m: ModelInfo) => !m.is_downloaded && !isLegacySource(m),
    );
    const recommended = downloadable.filter((m) => m.is_recommended);
    return {
      downloaded: models.filter((m) => m.is_downloaded),
      downloadable,
      topPicks: recommended.slice(0, 2),
      rest: downloadable.filter((m) => !recommended.slice(0, 2).includes(m)),
    };
  }, [models]);

  const choose = async (id: string) => {
    if (preview) return;
    localStorage.setItem(PENDING, id);
    onChoose(id);
    if (models.find((m) => m.id === id)?.is_downloaded) return;
    // Error toasts come from modelStore's model-download-failed listener.
    if (!(await downloadModel(id))) {
      localStorage.removeItem(PENDING);
      onChoose(null);
    }
  };

  const cancel = async (id: string) => {
    if (preview || !(await cancelDownload(id))) return;
    localStorage.removeItem(PENDING);
    onChoose(null);
  };

  const status = (id: string): ModelCardStatus => {
    if (id in extractingModels) return "extracting";
    if (id in verifyingModels) return "verifying";
    if (id in downloadingModels) return "downloading";
    return "downloadable";
  };

  const card = (model: ModelInfo, featured = false) => (
    <ModelCard
      key={model.id}
      model={model}
      variant={featured ? "featured" : "default"}
      status={status(model.id)}
      disabled={chosen !== null && chosen !== model.id}
      onSelect={choose}
      onDownload={choose}
      onCancel={cancel}
      downloadProgress={downloadProgress[model.id]?.percentage}
      downloadSpeed={downloadStats[model.id]?.speed}
      showRecommended={false}
    />
  );

  return (
    <div className="w-full max-w-[520px] space-y-3 text-start">
      {downloaded.map((model) => (
        <ModelCard
          key={model.id}
          model={model}
          status={chosen === model.id ? "active" : "available"}
          onSelect={choose}
          showRecommended={false}
        />
      ))}
      {topPicks.map((model) => card(model, true))}
      {rest.length > 0 && topPicks.length > 0 && (
        <button
          type="button"
          onClick={() => setShowAll((v) => !v)}
          className="flex items-center justify-center gap-1.5 mx-auto py-1 text-sm font-medium text-text-muted hover:text-text transition-colors cursor-pointer"
        >
          {showAll
            ? t("onboarding.showFewerModels")
            : t("onboarding.showAllModels", { total: downloadable.length })}
          <ChevronDown
            className={`w-4 h-4 transition-transform duration-200 ${showAll ? "rotate-180" : ""}`}
          />
        </button>
      )}
      {(showAll || topPicks.length === 0) && rest.map((model) => card(model))}
    </div>
  );
};
