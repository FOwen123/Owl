import React from "react";
import { AlertCircle, AlertTriangle, Info, CheckCircle } from "lucide-react";

type AlertVariant = "error" | "warning" | "info" | "success";

interface AlertProps {
  variant?: AlertVariant;
  /** When true, removes rounded corners for use inside containers */
  contained?: boolean;
  children: React.ReactNode;
  className?: string;
}

const variantStyles: Record<
  AlertVariant,
  { container: string; icon: string; text: string }
> = {
  error: {
    container: "bg-error/10 border border-error/20",
    icon: "text-error",
    text: "text-error",
  },
  warning: {
    container: "bg-warning/10 border border-warning/20",
    icon: "text-warning",
    text: "text-warning-foreground",
  },
  info: {
    container: "bg-accent-soft border border-accent/20",
    icon: "text-accent",
    text: "text-text",
  },
  success: {
    container: "bg-success/10 border border-success/20",
    icon: "text-success",
    text: "text-success-foreground",
  },
};

const variantIcons: Record<AlertVariant, React.ElementType> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  success: CheckCircle,
};

export const Alert: React.FC<AlertProps> = ({
  variant = "error",
  contained = false,
  children,
  className = "",
}) => {
  const styles = variantStyles[variant];
  const Icon = variantIcons[variant];

  return (
    <div
      className={`flex items-start gap-3 px-4 py-3 ${styles.container} ${contained ? "" : "rounded-xl"} ${className}`}
    >
      <Icon className={`w-4 h-4 shrink-0 mt-0.5 ${styles.icon}`} />
      <p className={`text-[13px] ${styles.text}`}>{children}</p>
    </div>
  );
};
