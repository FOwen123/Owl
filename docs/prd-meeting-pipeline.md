# Owl: meeting pipeline and dictation extras (PRD)

Status: steps 1 to 8 built on branch `feat/meeting-pipeline` (2026-10-12); step 9 not started. The task list is `TODO.md` Phase 4 and 5; this file says what each item does and when it is done.

**Rules for every step**

- Mac first, but use parts that also run on Windows (transcribe.cpp, ONNX, plain Rust). Mac-only code goes in small files of its own.
- Local by default. Nothing goes to an AI unless Post Process is set up; meeting notes are written automatically only while Post Process is on.
- UI stays clean: one short sentence per description, following `DESIGN.md`.
- Each step is done when `bun run lint`, `bunx tsc --noEmit`, `bun run check:translations`, `cargo test` and `cargo clippy` pass and the step's own checks below were run on a real recording or in the built app.

**Delivery**

Steps 1 to 8 are built in order on one branch in the `../Owl-speakers` worktree, one or more commits per step, and opened as a single pull request after step 8. Step 9 gets its own worktree, is tested locally and reported, with no pull request.

| Step  | What                                                                               |
| ----- | ---------------------------------------------------------------------------------- |
| 1     | Merge Handy's latest code                                                          |
| 2     | Dictionary: learning bug, Words guard, vocabulary hints                            |
| 3     | Speaker labels (Sortformer v2.1)                                                   |
| 4     | Notes: titles, templates, Summarize again, Re-transcribe, new layout, Paper export |
| 5     | Live transcript and echo removal                                                   |
| 6     | Import and export                                                                  |
| 7     | Command line, local API, AI-agent access                                           |
| 8     | Edit by voice and auto dictionary, then open the pull request                      |
| 9     | Nemotron 3 port (own worktree, local test, report)                                 |
| Later | Calendar "Join & record", then open-sourcing (`TODO.md` Phase 6)                   |

---

## 1. Merge Handy's latest code

Handy is 10 commits ahead of Owl (since 2026-10-05). They bring:

- transcribe.cpp 0.4.0 (Handy #2265). Owl needs it for any newer transcribe.cpp, including Nemotron 3.
- Transcription in its own worker (#2208): a crash in the speech engine no longer takes the app down.
- Model-loading status instead of a stuck "Transcribing…" (#2227 and a follow-up).
- Microphone capture through the new `handy-recorder` crate (#2188).
- transcribe.cpp's own speech detection, Silero (#2267 and a fix).

**Work**

- Merge, keeping Owl's side for docs Owl deleted or rewrote, and both sides for the 26 translation files.
- Owl's meeting recorder used `cpal`, `rtrb` and `FrameResampler`, which Handy removed. Record the meeting microphone with `handy-recorder` instead (see below). Owl's own system-audio capture (`system_capture.rs`) keeps `rtrb`.

**Why handy-recorder for the meeting mic**

Both sit on `cpal`; the difference is what happens when things go wrong.

|                                         | Owl's current meeting mic code                           | handy-recorder                                                     |
| --------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------ |
| Pick the mic by name                    | Yes                                                      | Yes (device id from its device list)                               |
| Convert to 16 kHz mono                  | Yes, through `FrameResampler`                            | Built in                                                           |
| Sample formats                          | f32 and i16 only; other mics fail                        | Whatever the OS uses                                               |
| Audio arriving faster than it's saved   | A 10-second buffer, then audio is dropped without a word | Counts dropped audio and reports it                                |
| Mic unplugged, stalled, Mac asleep      | Logs an error and records silence                        | Detects it and reports why the recording ended                     |
| Fixed 30 ms pieces for speech detection | No                                                       | Yes, which step 5's live transcript needs                          |
| AirPods                                 | —                                                        | Can move AirPods to this Mac while recording                       |
| Maintained                              | By us, about 90 lines                                    | By the transcribe.cpp/Handy maintainer; Handy itself depends on it |

**Done when**

- All checks pass and a dictation works in English and Chinese.
- A short meeting records both tracks and transcribes.
- Unplugging the mic mid-meeting shows an error instead of a silent recording.

---

## 2. Dictionary: learning bug, Words guard, vocabulary hints

**The learning bug.** You said "just keep it simple", Owl wrote "skip", you fixed it, and Owl offered "skip → keep". A replacement rule from an ordinary word would rewrite every future "skip".

**New rule** for "Learn from my edits" (`correction.rs`), using the OS spell checker (macOS `NSSpellChecker`, built in with 43 languages, part of AppKit, which Owl already links; Windows Spell Checking API later). Nothing to download.

- The misheard word is a real word ("skip", "depict"): no replacement rule.
- The misheard word isn't a word ("Owel"): offer the replacement, as today.
- The fixed word is a name or term (capital letters, an acronym, or unknown to the spell checker, like "Depick"): also add it to **Words**.
- Chinese keeps its same-sound (pinyin) rule.

**Words guard.** Owl's Words fix-up (Handy's `apply_custom_words`) matches by spelling and sound, so with "Claude" in Words every "cloud" becomes "Claude", and with "Depick" every "depict" becomes "Depick". It never rewrites a word the spell checker knows. Your own replacement rules (like your "cloud → Claude") still always apply, because you chose them.

**Vocabulary hints for every model that takes them.** Today Owl passes Words only to Whisper, as its starting prompt. Instead, pass Words through transcribe.cpp's general `vocabulary` option to any model that says it supports it (`Feature::Vocabulary`): Whisper, Qwen3-ASR, FunASR-Nano, Granite and MOSS today, and future ones without code changes. Nemotron 3.5 doesn't take hints, so for it Words work through the guarded fix-up and the meeting summary's spelling list.

**Done when**, as tests in `correction.rs` and `text.rs`:

- skip → keep, send → sent and ship → shop offer nothing.
- Owel → Owen offers a replacement.
- depict → depick adds "depick" to Words only.
- With "Claude" in Words, "cloud computing" stays as it is.
- A Whisper or Qwen3-ASR dictation receives Words as hints. Checked by logging the run options once.

---

## 3. Speaker labels

**What you see.** In a meeting, the other side is split into Speaker 1, 2, 3… instead of one "Others"; your mic stays "You". Click a speaker anywhere to rename them; the name changes on every line and in the notes. No voices are remembered between meetings (privacy).

**Model.** NVIDIA Sortformer v2.1 (`handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf`, Q8, 139 MB). It runs on transcribe.cpp 0.4.0 with no fork and handles up to 4 other people. On the 12 Oct meeting it found the one other person on the call track, and processed 88 minutes in 18 seconds. Nemotron 3 replaces it in step 9; screens and storage stay the same.

**Download.** One "Download speaker labels" button on the Notetaker page, no model picker. It isn't in Owl's model catalog, so it gets its own entry: full URL, size and SHA-256, the same way Glimpse lists its models. Without it, meetings work as today.

**How it works.** After the meeting, run the speaker model on `system.wav`. Assign each "Others" line to the speaker talking during it; when the speaker changes inside a line, split it at that word, using the word times Nemotron 3.5 provides. This works the same whether lines were made after the meeting (now) or live (step 5).

**Storage** (`meetings.db` migration):

- Each line gets a speaker number (absent on old meetings).
- Each meeting gets a name per speaker.

**Notes.** The AI writes `<@speaker:2>` placeholders, as Wispr Flow does, which Owl fills with the current name. A rename updates the notes without a new AI call.

**Done when**

- The 12 Oct meeting shows "You" and one other speaker.
- A call with 3 or more people shows each one separately.
- Renaming updates the transcript and notes.
- Old meetings still open.

---

## 4. Notes

- **Auto title.** Written by the AI in the same request as the notes, saved in a new `title` column, shown in the meeting list. Without an AI it stays the date. Editable by clicking it.
- **Templates.** Built-ins: General (today's Wispr Flow-style notes), 1:1, Interview, Stand-up, Lecture, plus your own, edited on the Notetaker page. Pick one from a dropdown next to "Summarize again"; set a default in Notetaker settings.
- **Summarize again** with another template or AI model.
- **Re-transcribe** from the kept audio, so new Words, rules or a better model apply. Lives in the meeting's ⋯ menu.
- **Layout.**
  - The list: rows show title, date, duration and the notes' first line, with "Import audio" and "Start meeting" at the top.
  - A meeting page, top to bottom:
    - a back link, the Export menu and the ⋯ menu (Re-transcribe, Delete)
    - the title
    - date, duration and speaker chips (click to rename)
    - Notes, with the template dropdown and Summarize again
    - the transcript, with Copy
  - While recording, the live transcript fills the page, with Stop.
- **Paper.** After building, export the meeting pages to Paper. Later changes start in Paper and are then implemented.

**Done when**

- A new meeting gets a title and notes automatically when Post Process is on.
- Switching template rewrites the notes.
- Re-transcribe picks up a newly added Word.
- The Paper file has the list, meeting page and recording view.

---

## 5. Live transcript and echo removal

**Live transcript.** While recording, each track goes through speech detection (transcribe.cpp's Silero) in 30 ms pieces. When a stretch of speech ends (or after 25 seconds), it's transcribed with the local model and appears on screen. That text is final. After the meeting Owl only adds speaker labels (step 3) and joins neighbouring lines from the same speaker.

Wispr Flow differs: it shows Soniox's live lines, then replaces them with a second cloud pass. Owl would run the same model twice, so it doesn't.

**Echo removal.** Without earphones, the other side's voices leak into your mic and appear as "You".

- **Model:** LocalVQE v1.4-AEC (echo only, 3 MB, Apache-2.0, `LocalAI-io/LocalVQE`, the one Muesli uses), shipped inside the app.
- **When:** it runs live on the mic before transcription, with the call audio as the reference.
- **Raw audio:** the raw `mic.wav` is kept, so Re-transcribe can redo it.
- **Build:** LocalVQE brings its own copy of ggml, which clashes with transcribe.cpp's, so it's built as its own dynamic library inside the app bundle.
- **Comparison:** test v1.2 (echo + noise, Muesli's choice) on the same recording.

**Done when**

- A call on laptop speakers: count "You" lines that repeat the other side, with and without LocalVQE (target: none left). Compare v1.4-AEC with v1.2.
- Live lines appear within a few seconds of each pause.
- The finished transcript matches what was shown live.

---

## 6. Import and export

- **Import** audio or video into the meeting list.
  - **Decoder:** `symphonia` (pure Rust, Mac and Windows) for mp3, m4a/AAC, wav, flac, ogg/Vorbis, and the audio inside mp4 and mov, resampled to 16 kHz.
  - **Opus:** symphonia can't decode Opus (WebM, `.opus`, WhatsApp and Telegram voice notes). Those files get a plain "this format isn't supported yet" message.
  - **Speakers:** an imported file is one track, so everyone in it goes through speaker labels; there's no "You".
- **Export:** Markdown, plain text, SRT and VTT subtitles, written as plain text with no library, saved through Owl's existing save dialog. Lines store their end time (`end_ms`) for subtitles. PDF later.

**Done when**

- An m4a, an mp3 and an mp4 import and get speaker labels; an `.opus` file shows the message.
- Each export opens correctly: the SRT in a video player, the Markdown in a preview.

---

## 7. Command line, local API, AI-agent access

- **Command line:** `owl meetings`, `owl meeting <id> --notes|--transcript`, `owl transcribe <file>`. The read commands read `meetings.db` directly, so the app needn't be open. Installed from Settings, which adds a link to Owl's binary.
- **Local API:** an OpenAI-compatible `POST /v1/audio/transcriptions`, off by default, only reachable from this Mac, needs a token shown in Settings.
- **AI-agent access:** `owl mcp`, a read-only MCP server over standard input and output. Tools: list, read and search meetings and dictations. No starting, editing or deleting.

**Done when**

- Each command works from Terminal.
- `curl` transcribes a file with the token and is refused without it.
- Claude Code (`claude mcp add owl -- owl mcp`) can find a meeting by a word in it.

---

## 8. Edit by voice and auto dictionary

**Edit by voice**, like Wispr Flow's Command Mode:

- **Shortcut:** Option+Shift+Space. That key was Handy's "Transcribe with Post-Processing", which stays in Settings with no key assigned.
- **With text selected:** hold the shortcut, say what to change ("make this shorter", 「翻成英文」), release. The Post Process AI's answer replaces the selection.
- **With nothing selected:** the answer is written at the cursor.
- **Cancel and undo:** Esc cancels. Undo is the app's own Cmd+Z.
- **No AI set up:** the same "Set up AI" prompt as Summarize.

**Auto dictionary**, like Glimpse and Wispr Flow: when you fix a pasted word into a name or term (step 2's rule), Owl offers to add it to **Words**, using the existing card with the 5-second countdown. So the model spells it right however it mishears it next time, and the meeting summary uses the same spelling.

**Then:** open the pull request for steps 1 to 8.

**Done when**

- Selecting a sentence in Notes and saying "make it formal" replaces it, and Cmd+Z restores it.
- With nothing selected, a question gets an answer at the cursor.
- Fixing "depick" to "Depick" in a dictation shows the card and adds the Word.

---

## 9. Nemotron 3 port (own worktree, `../Owl-nemotron3`)

Upstream status on 2026-10-12:

- **The maintainer's port:** open PR `handy-computer/transcribe.cpp#175`. Model type `nemotron3_diar`, "stage 1-4" of his seven-stage process, untouched since 2026-09-26, and now conflicting with the 2026-10-05 rework of the speaker code.
- **Model file:** none published. His converter needs NVIDIA's NeMo toolkit.
- **Glimpse's port:** it works, but its model file doesn't load in #175.

**Work**

1. Ask on #175 whether he's still on it and offer help. This is a public comment from your GitHub, so it waits for your yes.
2. Rebase #175 onto transcribe.cpp 0.4.0 and finish the remaining stages.
3. Make the model file once with his converter, then host it on `owl-dictation` on Hugging Face.
4. Point Owl at it, then test locally and report. No pull request to Owl.
5. Offer the fixes back to #175. Switch to the official version once it merges.

**Done when**

- On the 12 Oct meeting: 2 speakers on the mixed recording, at least as good as Glimpse's build (88% agreement with Wispr Flow).
- A call with 5 or more people shows each one.
- Dictation and the rest of the pipeline still pass.

---

## Not doing

- Remembering voices across meetings (privacy).
- A model picker for speaker labels.
- Bundling FFmpeg.
- PDF export (later).
- Calendar (later).
