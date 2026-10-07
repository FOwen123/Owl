import React from "react";
import owlMark from "@/assets/owl-mark.png";

// The hand-drawn owl (source and build script: scripts/owl-icons/). It is black
// ink, so it always sits on a white "sticker" to stay visible in dark mode.
export const OwlMark = ({
  size = 24,
  className = "",
}: {
  size?: number;
  className?: string;
}) => (
  <span
    className={`inline-flex items-center justify-center shrink-0 rounded-full bg-logo-paper ${className}`}
    style={{ width: size, height: size }}
  >
    <img
      src={owlMark}
      alt=""
      width={size * 0.82}
      height={size * 0.82}
      draggable={false}
    />
  </span>
);

export const OwlLogo = ({
  size = 32,
  className = "",
}: {
  size?: number;
  className?: string;
}) => (
  <div className={`flex items-center gap-2 ${className}`}>
    <OwlMark size={size} />
    {/* eslint-disable-next-line i18next/no-literal-string */}
    <span className="font-semibold text-text" style={{ fontSize: size * 0.6 }}>
      Owl
    </span>
  </div>
);
