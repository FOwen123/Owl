import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "warning"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  const baseClasses =
    "font-medium rounded-lg border transition-colors focus:outline-none focus-visible:ring-[3px] focus-visible:ring-focus-ring disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer";

  const variantClasses = {
    // DESIGN.md: one primary per view; deep fill with an inset top highlight.
    primary:
      "text-accent-foreground bg-accent-fill border-accent-fill shadow-[inset_0_1px_0_rgb(255_255_255/16%)] hover:brightness-110",
    "primary-soft":
      "text-accent bg-accent-soft border-transparent hover:brightness-110",
    secondary:
      "text-text bg-surface-raised border-border hover:bg-surface-raised/70",
    // For buttons sitting on warning surfaces like SecureInputWarning.
    warning:
      "text-text bg-surface-raised border-border hover:bg-warning/15 hover:border-warning",
    // Destructive actions are ghost buttons with error text; no red fills.
    danger: "text-error border-transparent hover:bg-surface-raised",
    "danger-ghost": "text-error border-transparent hover:bg-surface-raised",
    ghost: "text-current border-transparent hover:bg-surface-raised",
  };

  const sizeClasses = {
    sm: "px-2.5 py-1 text-xs",
    md: "px-3.5 py-1.5 text-sm",
    lg: "px-4 py-2 text-base",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
