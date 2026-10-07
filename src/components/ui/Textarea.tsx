import React from "react";

interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: "default" | "compact";
}

export const Textarea: React.FC<TextareaProps> = ({
  className = "",
  variant = "default",
  ...props
}) => {
  const baseClasses =
    "text-sm bg-surface-raised border border-border rounded-lg text-start placeholder:text-text-muted transition-[background-color,border-color,box-shadow] duration-150 focus:outline-none focus:bg-surface focus:border-accent focus:ring-[3px] focus:ring-focus-ring resize-y";

  const variantClasses = {
    default: "px-2.5 py-1.5 min-h-[100px]",
    compact: "px-2 py-1 min-h-[80px]",
  };

  return (
    <textarea
      className={`${baseClasses} ${variantClasses[variant]} ${className}`}
      {...props}
    />
  );
};
