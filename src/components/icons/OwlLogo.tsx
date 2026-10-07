import React from "react";

// Source artwork: scripts/owl-icons/appicon.svg (same 64×64 geometry).
export const OwlMark = ({
  width = 24,
  height,
  className,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
}) => (
  <svg
    width={width}
    height={height ?? width}
    viewBox="0 0 64 64"
    className={className}
    aria-hidden="true"
  >
    <g
      className="stroke-logo-stroke"
      strokeWidth={3}
      strokeLinejoin="round"
      strokeLinecap="round"
    >
      <path
        className="logo-primary"
        d="M16 21 L13 7 L26 15 Q32 13.5 38 15 L51 7 L48 21 Q56 31 54 44 Q51 60 32 60 Q13 60 10 44 Q8 31 16 21 Z"
      />
      <path fill="none" d="M14 25 Q23 17 32 25 Q41 17 50 25" />
      <circle cx="23" cy="33" r="8.5" fill="#fff" />
      <circle cx="41" cy="33" r="8.5" fill="#fff" />
    </g>
    <g className="logo-stroke" stroke="none">
      <circle cx="23" cy="33.5" r="3.4" />
      <circle cx="41" cy="33.5" r="3.4" />
      <path d="M28.5 42 L35.5 42 L32 48.5 Z" />
    </g>
  </svg>
);

export const OwlLogo = ({
  size = 32,
  className,
}: {
  size?: number;
  className?: string;
}) => (
  <div className={`flex items-center gap-2 ${className ?? ""}`}>
    <OwlMark width={size} />
    {/* eslint-disable-next-line i18next/no-literal-string */}
    <span className="font-semibold text-text" style={{ fontSize: size * 0.6 }}>
      Owl
    </span>
  </div>
);
