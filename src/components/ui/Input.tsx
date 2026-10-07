import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

export const Input: React.FC<InputProps> = ({
  className = "",
  variant = "default",
  disabled,
  ...props
}) => {
  const baseClasses =
    "text-sm bg-surface-raised border border-border rounded-lg text-start placeholder:text-text-muted transition-[background-color,border-color,box-shadow] duration-150";

  // DESIGN.md input: focused = surface background, accent border, 3px ring.
  const interactiveClasses = disabled
    ? "opacity-60 cursor-not-allowed"
    : "focus:outline-none focus:bg-surface focus:border-accent focus:ring-[3px] focus:ring-focus-ring";

  const variantClasses = {
    default: "px-2.5 py-1.5",
    compact: "px-2 py-1",
  } as const;

  return (
    <input
      className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
      disabled={disabled}
      {...props}
    />
  );
};
