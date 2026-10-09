# Owl cleanup TODO

How Owl tidies a dictation before pasting it: fixed rules (instant, offline) first, then optional AI cleanup. Linked from [TODO.md](TODO.md).

**Today:**

- Rules inherited from Handy, in `src-tauri/src/audio_toolkit/text.rs`:
  - fillers "uh", "hmm", "mmm" (plus "um", "ah", "eh" for English)
  - collapse a word repeated 3+ times
  - tidy spaces
- AI cleanup: the Post Process toggle and provider, in `actions.rs`.

**Missing:** Chinese rules, checks on the AI's reply, and a single "how much cleanup" choice.

**Where code goes:** Owl's rules go in new files (`src-tauri/src/cleanup_rules.rs`, `src-tauri/src/cleanup_guard.rs`) so Handy's `text.rs` stays untouched and upstream merges stay small. They run in `post_process_transcription_text` (`src-tauri/src/managers/transcription.rs`) after Handy's filler and stutter step.

**Rules:**

- Every rule starts with a failing test, using the examples below.
- Logic ported from BetterWispr (Apache-2.0) or Felix (MIT) gets a credit line in the file header.

---

## 1. Cleanup level: None / Light / Medium (as in Wispr Flow)

One choice on the Dictation page, copying Wispr Flow's Auto Cleanup (Style > Auto Cleanup). Default: **Light**.

| Level  | Wispr Flow's wording                                                                  | In Owl                                                                                                                |
| ------ | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| None   | "Pastes the raw transcript, including filler words, false starts, and spoken errors." | No rules, no AI. Only your Dictionary replacements and Traditional/Simplified conversion (your settings, not cleanup) |
| Light  | "Removes filler words and fixes grammar."                                             | All fixed rules below, then the AI fixes grammar, fillers and self-corrections **without rewording**                  |
| Medium | "May reword phrases for clarity and conciseness while preserving meaning."            | Light, plus the AI may reword for clarity                                                                             |

Wispr Flow also had a "High" level and removed it; Owl skips it. Wispr's self-correction feature ("Backtrack": "coffee at 2 actually 3" → "coffee at 3") is part of Light here.

- [x] **Setting:** a new `cleanup_level` (none / light / medium) in `settings.rs`.
  - It replaces Handy's "Remove Filler Words" toggle. Whether the AI takes part is a separate "AI cleanup" switch (Handy's Post Process toggle, relabelled) on the Post Process page, so its state is always visible.
  - None also turns off Handy's filler removal.
  - Migration: filler removal off → None; otherwise Light. Post Process keeps its old value, so upgrading never starts sending dictations to an AI that was off.
- [x] **AI step:**
  - Runs for Light and Medium when "AI cleanup" is on and an AI provider is set up; otherwise rules only.
  - Light uses the prompt selected on the Post Process page.
  - Medium adds one line to it: "You may reword for clarity and conciseness, keeping the meaning."
- [x] **UI:**
  - A `SegmentedControl` (`src/components/ui/SegmentedControl.tsx`) on the Dictation page.
  - Remove Handy's separate "Remove Filler Words" toggle; relabel the Post Process toggle "AI cleanup".
  - Keep the Post Process page for choosing the AI provider and prompt.
- [x] **Strings:** one short sentence each, in `en`, `zh`, `zh-TW`.
- [x] **Tests:** migration of old settings; which steps run at each level.

## 2. Checks on the AI's reply (`cleanup_guard.rs`)

Owl pastes whatever the AI returns. Throw the reply away and keep the rule-cleaned text when the reply:

- [x] is empty or only whitespace (this caused the empty pastes)
- [x] starts with a preamble: "Here is the cleaned text:", "Sure,", 「以下是」… (Felix: `META_PREFIXES` in `cleanup.rs`)
- [x] answers instead of editing: the dictation had "?" and the reply has none, or the reply is much longer
- [x] changes length too much: fewer than 40% or more than 120% of the words (+2), per BetterWispr. Count Chinese by characters.
- [x] adds too many new words: more than 25% of its meaningful words were never said
- [x] Also: skip the AI for dictations under 4 words, and give it a timeout (BetterWispr: 10 s + 1 s per 10 words)
- [x] Log which check fired (`debug!`) so bad prompts are easy to spot

## 3. Chinese rules (`cleanup_rules.rs`)

- [x] **Fillers:** 嗯, 呃, 欸, 誒 (also with ， after them) at the start of a clause or standing alone
  - 「嗯，我覺得可以」 → 「我覺得可以」
  - Only remove 啊 when it stands alone between commas; 好啊 is a word
  - Never by rule: 那個, 就是, 然後 (real words; leave them to the AI)
- [x] **Stutters:** Handy's rule splits on spaces, so it never fires on Chinese
  - Collapse one character repeated **3+** times: 「我我我覺得」 → 「我覺得」
  - Collapse a word repeated across a comma: 「我們，我們去」 → 「我們去」
  - Never collapse doubles (謝謝, 看看, 天天, 媽媽); keep laughter (哈哈哈)
- [x] **Always a space between Chinese and English words**:
  - 「用Claude Code寫」 → 「用 Claude Code 寫」
  - Not next to punctuation: 「我用 Claude，很好」
  - Numbers stay as they are
- [x] **Full-width punctuation** next to Chinese characters:
  - `, ? ! : ;` → `，？！：；`
  - 「好的,我們走?」 → 「好的，我們走？」
  - Leave English and number contexts alone ("3.5", "Node.js", "a, b")

## 4. English rules (`cleanup_rules.rs`)

- [x] **Double words:** "I I think" → "I think", "the the" → "the"
  - Keep emphasis: very very, really really, no no, bye bye, ha ha, had had, that that (BetterWispr `keptDoubles`)
- [x] **More fillers:** "er", "erm"
  - "mm", except after a number ("5 mm")
  - "you know" only when set off by commas: "It was, you know, fine" → "It was fine"
  - Never "like" (leave to the AI)
- [x] **Simple self-corrections** (BetterWispr `dropRepairs`): only when the correction restarts with a word said in the last 4 words
  - "fly to Pune, sorry, no, to Delhi" → "fly to Delhi"
  - Cues: sorry, no, wait, oops, actually, I mean
  - Not covered (needs the AI): "Tuesday, sorry, I mean Wednesday"

## 5. Silence "hallucinations"

Whisper-family models (Whisper, Breeze) invent phrases on near-silent audio.

- [x] Drop the result when the **whole** transcript is exactly one known phrase (ignoring case and punctuation)
  - English: "Thanks for watching!", "Thank you for watching." ("Thank you." is **not** dropped: people really dictate it.)
  - Chinese: 「字幕由 Amara.org 社群提供」, 「請不吝點讚 訂閱 轉發 打賞支持明鏡與點點欄目」, 「謝謝觀看」
- [x] Never edit inside longer text, because you might really say "thank you"

## 6. Voice commands (opt-in, off by default)

You might say these words literally, so they need their own switch.

- [x] Line breaks: "new line", "new paragraph"; 「換行」, 「新段落」
- [x] Punctuation: "comma", "question mark", "exclamation mark", "full stop"; 「逗號」, 「問號」, 「句號」
- [x] "scratch that" removes the sentence before it; 「刪掉上一句」
- [ ] Later: "press enter" at the end of a dictation sends Return after pasting (Felix `apply_voice_triggers` in `scratchpad.rs`)

## 7. Left to the AI (Light and Medium), no rules

- Most self-corrections ("Tuesday, sorry, I mean Wednesday"; 「明天，不是，後天」)
- Numbers, dates, money ("twenty five percent" → 25%)
- Fillers that are also real words: like, 那個, 就是, 然後

BetterWispr's research notes back this up (`docs/accuracy.md`): fillers suit a fixed list, self-repairs need a trained model, and running an AI over every dictation made accuracy worse.

## Order

1. Reply checks (section 2)
2. Chinese rules (section 3)
3. Cleanup levels (section 1)
4. English rules (section 4)
5. Silence phrases (section 5)
6. Voice commands (section 6)

**Built:** `cleanup_rules.rs`, `cleanup_guard.rs`, `commands/cleanup.rs`, `src/components/settings/CleanupSettings.tsx` (Dictation page). Still to check by hand: a real dictation at each level in a built Owl.app.

**Done means:** tests pass, then `bun run lint`, `bunx tsc --noEmit`, `bun run check:translations`, `cargo test`, `cargo clippy`, then a real dictation in a built Owl.app with Qwen3-ASR.
