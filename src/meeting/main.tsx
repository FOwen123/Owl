import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { MeetingWindow } from "./MeetingWindow";
import { installMockBackend } from "@/dev/mockBackend";
import {
  applyTheme,
  getStoredTheme,
  syncThemeFromSettings,
} from "@/lib/utils/theme";
import type { Theme } from "@/bindings";
import "@/i18n";
import "./meeting.css";

// Owl: `?mock` runs the UI in a browser without the Rust backend (dev only).
if (import.meta.env.DEV && location.search.includes("mock")) {
  installMockBackend();
}

// Its own webview, so it sets `data-theme` itself (see src/overlay/main.tsx).
applyTheme(getStoredTheme());
syncThemeFromSettings();
listen<Theme>("theme-changed", (event) => applyTheme(event.payload));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MeetingWindow />
  </React.StrictMode>,
);
