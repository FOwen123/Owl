// Dev-only fake backend so the UI can be designed and checked in a browser:
// `bun run dev`, then open http://localhost:1420/?mock. Never runs in a real
// build (guarded by import.meta.env.DEV and the ?mock query).
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

const binding = (id: string, name: string, keys: string) => ({
  id,
  name,
  description: "",
  default_binding: keys,
  current_binding: keys,
});

const settings: Record<string, unknown> = {
  onboarding_completed: true,
  selected_model: "turbo",
  selected_language: "zh",
  chinese_script: "traditional",
  theme: "dark",
  app_language: "en",
  audio_feedback: true,
  push_to_talk: true,
  bindings: {
    transcribe: binding("transcribe", "Transcribe", "option+space"),
    cancel: binding("cancel", "Cancel", "escape"),
  },
  custom_words: ["Owl", "Tauri", "Breeze"],
  snippets: [
    { trigger: "my email", text: "owen@example.com" },
    { trigger: "我的地址", text: "台北市信義區市府路 1 號" },
  ],
  replacements: [{ from: "cloud code", to: "Claude Code" }],
  meeting_template: "general",
  meeting_templates: [
    {
      id: "own-1",
      name: "Sales call",
      prompt: "### Customer needs, ### Objections, ### Next Steps",
    },
  ],
  learn_from_edits: true,
  local_api_enabled: false,
  local_api_token: "",
  cleanup_level: "light",
  post_process_enabled: true,
  voice_commands_enabled: false,
  edit_suggestions: [
    { from: "Owel", to: "Owen" },
    { from: "公事", to: "公式" },
  ],
  post_process_providers: [],
  post_process_prompts: [],
};

const model = {
  id: "turbo",
  name: "Whisper Turbo",
  description: "Balanced accuracy and speed.",
  filename: "turbo.bin",
  source: { type: "Url", url: "", sha256: null },
  size_mb: 1600,
  is_downloaded: true,
  is_downloading: false,
  partial_size: 0,
  is_directory: false,
  engine_type: "Whisper",
  accuracy_score: 0.8,
  speed_score: 0.6,
  supports_translation: true,
  is_recommended: true,
  supported_languages: ["en", "zh"],
  supports_language_selection: true,
  is_custom: false,
  supports_streaming: false,
  supports_language_detection: true,
};

const responses: Record<string, unknown> = {
  get_app_settings: settings,
  get_default_settings: settings,
  get_available_models: [model],
  get_current_model: "turbo",
  get_transcription_model_status: "turbo",
  get_model_load_status: { is_loaded: true, current_model: "turbo" },
  get_available_microphones: [
    { index: "0", name: "MacBook Pro Microphone", is_default: true },
  ],
  get_available_output_devices: [
    { index: "0", name: "MacBook Pro Speakers", is_default: true },
  ],
  get_history_entries: [],
  get_meetings: [
    {
      id: 1,
      started_at: Date.now() - 86_400_000,
      duration_ms: 1_520_000,
      status: "ready",
      summary:
        "<@speaker:1> and you agreed to ship the beta on Friday.\n\n### Launch\n- <@speaker:2> says QA has 12 open bugs.",
      title: "Beta launch plan",
      template: "general",
      speaker_names: { 1: "Harry" },
      summarizing: false,
      error: null,
      transcript: [
        { start_ms: 2_000, speaker: "you", text: "好，我們開始今天的週會。" },
        {
          start_ms: 6_500,
          speaker: "others",
          voice: 1,
          text: "Sure. The beta build is ready for testing.",
        },
        {
          start_ms: 9_000,
          speaker: "others",
          voice: 2,
          text: "QA still has twelve open bugs.",
        },
        {
          start_ms: 12_000,
          speaker: "you",
          text: "Great, let's ship it to the team on Friday.",
        },
      ],
    },
  ],
  speaker_model_installed: false,
  get_summary_options: {
    built_in_templates: [
      "general",
      "one_on_one",
      "interview",
      "standup",
      "lecture",
    ],
    providers: [
      { id: "openrouter", label: "OpenRouter", model: "deepseek-v4-flash" },
    ],
  },
  get_secure_input_status: { active: false },
  is_update_checks_locked: true,
  is_portable: false,
  install_command_line: "/Users/you/.local/bin/owl",
  is_laptop: true,
  is_recording: false,
  get_app_dir_path: "~/Library/Application Support/com.hao.owl",
  get_log_dir_path: "~/Library/Logs/com.hao.owl",
  "plugin:app|version": "0.1.0",
};

export function installMockBackend() {
  (window as unknown as Record<string, unknown>).__TAURI_OS_PLUGIN_INTERNALS__ =
    { platform: "macos", os_type: "macos", version: "26.0", eol: "\n" };
  mockWindows("main");
  mockIPC(
    (cmd, payload) => {
      if (cmd in responses) return responses[cmd];
      if (cmd === "change_local_api_setting") {
        settings.local_api_enabled = (payload as { enabled: boolean }).enabled;
        settings.local_api_token ||=
          "owl_3f9c1a7e5b2d4c6f8a0e1b3d5f7a9c2e4b6d8f0a1c3e5a7b";
        return null;
      }
      if (cmd === "change_cleanup_level_setting") {
        settings.cleanup_level = (payload as { level: string }).level;
        return null;
      }
      if (
        cmd === "accept_edit_suggestion" ||
        cmd === "dismiss_edit_suggestion"
      ) {
        type Pair = { from: string; to: string };
        const { suggestion } = payload as { suggestion: Pair };
        const same = (r: Pair) =>
          r.from === suggestion.from && r.to === suggestion.to;
        settings.edit_suggestions = (
          settings.edit_suggestions as Pair[]
        ).filter((r) => !same(r));
        if (cmd === "accept_edit_suggestion")
          (settings.replacements as Pair[]).push(suggestion);
        return null;
      }
      if (cmd.startsWith("plugin:macos-permissions|check")) return true;
      if (cmd.startsWith("plugin:macos-permissions|")) return null;
      return null;
    },
    { shouldMockEvents: true },
  );
}
