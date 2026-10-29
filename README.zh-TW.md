<img src="src-tauri/icons/128x128@2x.png" width="96" alt="Owl 圖示">

# Owl

[English](README.md) | [简体中文](README.zh-CN.md) | 繁體中文

**私密的語音輸入和會議記錄，專為中英文打造。開源的 Wispr Flow 替代方案。**

按下 Option+Space 開口說話，Owl 會把你說的內容打進目前的 App 裡。語音辨識在你自己的電腦上執行。Owl 還能去掉贅字、用語音修改選取的文字，並記錄會議。

Owl 最初是 CJ Pais 的 [Handy](https://github.com/cjpais/Handy) 的一個分支，現在獨立開發，和 Handy 沒有關聯，所以 Owl 的問題請在這裡回報。它的外觀參考了 [Wispr Flow](https://wisprflow.ai)，同樣和它沒有關聯。

![語音輸入設定](docs/screenshots/dictation.png)

## 功能

- **語音輸入。** 說話時按住 Option+Space，或者按一下開始、再按一下結束。按 Escape 取消。如果結束時沒有選取任何文字框，Owl 會把文字留在剪貼簿裡，並顯示一個「複製」按鈕。
- **中文和英文。** 有幾個語音模型能辨識同一句話裡夾雜的國語和英文。Owl 可以用繁體或簡體輸出中文。
- **整理。** Owl 在你的電腦上去掉贅字和重複的字。接上 AI 服務後，它還會修正文法、數字和清單。
- **用語音修改。** 選取文字，按 Option+Shift+Space，說出要怎麼改。需要 AI 服務。
- **字典和片語。** 把 Owl 常聽錯的名字教給它，或者儲存一段文字，說出「我的 email」這樣的短語就能貼上。如果你在語音輸入後馬上改了某個詞，Owl 會提示你把它加進字典。
- **學習。** 預設關閉。Owl 會嘗試調整自己的設定，只保留在你最近的語音輸入上得分更高的改動。每一項都可以復原。
- **會議記錄。** Owl 同時錄下你的麥克風和通話聲音，即時產生逐字稿，並標出每句話是誰說的。接上 AI 服務後，它還會寫會議摘要。
- **命令列和 AI 代理。** `owl` 指令可以輸出和轉錄會議，`owl mcp` 讓 Claude Code 這類代理讀取會議內容。

![會議記錄](docs/screenshots/meeting.png)

截圖使用的是 Owl 的示範資料。

## 隱私

語音辨識、說話者標註和回音消除都在你的電腦上執行。只有當你開啟 AI 步驟並選擇雲端服務時，音訊和文字才會離開你的電腦。除此之外，唯一的連網動作是下載你選擇的模型。

AI 步驟可以使用 macOS 26 上的 Apple Intelligence、Ollama 這類本機伺服器，或者用你自己的金鑰接上 OpenAI、Anthropic、OpenRouter、Groq、Cerebras、Z.AI 或 Amazon Bedrock。你也可以登入 ChatGPT，使用你的訂閱方案。

Owl 會保留最近 5 次語音輸入的音訊，以及你手動儲存的音訊。資料存放在 `~/Library/Application Support/com.hao.owl/`。

## 狀態

還在早期階段，為一個人的日常使用而做。可以下載 Mac 版，也可以自己建置。我在 macOS 26 上建置和使用它。Windows 和 Linux 的程式碼還在，但沒有測試過。

## 系統需求

下載版需要搭載 Apple Silicon 的 Mac。自己建置則需要：

- [Rust](https://rustup.rs) 1.99 或更新版本
- [Bun](https://bun.sh) 1.4 或更新版本
- [CMake](https://cmake.org/download/)

會議記錄需要 macOS 14.2 或更新版本才能錄到通話另一方的聲音。Windows 和 Linux 需要什麼，請看 [Handy 的建置說明](https://github.com/cjpais/Handy/blob/main/BUILD.md)。

## 安裝

### 下載

從[最新版本](https://github.com/FOwen123/Owl/releases/latest)下載 `Owl_<版本>_aarch64.dmg`，打開後把 Owl 拖進「應用程式」，再開啟它。

### 自己建置

```bash
git clone https://github.com/FOwen123/Owl.git
cd Owl
bun install
bun run tauri build
```

第一次建置需要一段時間。如果 CMake 報錯說政策版本太舊，請執行 `CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build`。

接著安裝並開啟：

```bash
cp -R src-tauri/target/release/bundle/macos/Owl.app /Applications/
open /Applications/Owl.app
```

### 設定

Owl 會一步步帶你完成設定：你的名字（會議筆記會用它）、你想用 Owl 做什麼、下載一個語音模型、麥克風和輔助使用權限、你的快捷鍵和一次簡短的聽寫測試，最後可以選擇接上 AI 來整理文字和寫筆記。輔助使用權限是用來把文字貼到其他 App 裡。Qwen3-ASR 1.7B 擅長中英夾雜，Parakeet Unified 速度快，但只支援英文。

如果要做會議筆記，設定時還會要求系統錄音（System Audio Recording）權限，這樣 Owl 才能聽到通話的聲音。

### 更新

Owl 會檢查新版本並自動更新，你也可以在視窗底部手動檢查。如果是自己建置的，請這樣更新：

```bash
cd Owl
git pull
bun install
bun run tauri build
osascript -e 'quit app "Owl"'
rm -rf /Applications/Owl.app
cp -R src-tauri/target/release/bundle/macos/Owl.app /Applications/
tccutil reset Accessibility com.hao.owl
open /Applications/Owl.app
```

每次重新建置，macOS 都會把它當成一個新 App，之前打開的輔助使用開關就會失效。`tccutil` 這一行會清掉舊的授權，Owl 會重新要求。你的設定、歷史紀錄和會議都會保留。

### 解除安裝

結束 Owl，刪除 `/Applications/Owl.app`。如果也要刪除資料，再刪除 `~/Library/Application Support/com.hao.owl/`。

## 命令列

在 Owl 設定的 **進階 > 其他 App > 命令列** 裡安裝 `owl` 指令。

```bash
owl meetings                 # 列出會議
owl meeting 12               # 以 Markdown 輸出一場會議
owl transcribe call.m4a      # 把一個檔案轉錄成新會議
owl --toggle-transcription   # 開始或停止語音輸入
```

要讓 Claude Code 讀取你的會議和語音輸入，執行 `claude mcp add owl -- owl mcp`。它只能讀取，不能修改。

## 常見問題

### 快捷鍵失效了（安全輸入）

很可能是 macOS 的安全輸入（Secure Input）開著。密碼欄位和一些 App 會打開它，通常會自己關掉。要找出是哪個 App 打開的，執行 `ioreg -l -w 0 | grep kCGSSessionSecureInputPID`，再執行 `ps -p <PID>`，然後結束那個 App。

### 重新建置後，輔助使用一直顯示「等待中...」

結束 Owl，執行 `tccutil reset Accessibility com.hao.owl`，再開啟 Owl 並重新授權。

### 會議裡只有我這邊的聲音

在 **系統設定 > 隱私權與安全性 > 系統錄音（System Audio Recording）** 裡打開 Owl。

更多內容（包括建置錯誤）請看 [BUILD.md](BUILD.md)（英文）。

## 參與貢獻

歡迎提交 issue 和 pull request。[BUILD.md](BUILD.md) 介紹開發流程和需要執行的檢查，[AGENTS.md](AGENTS.md) 介紹程式碼結構，[DESIGN.md](DESIGN.md) 介紹視覺設計。這些文件都是英文的。

## 致謝

- CJ Pais 和貢獻者們的 [Handy](https://github.com/cjpais/Handy)。Owl 以它為基礎。
- 語音辨識：[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp)、[ggml / whisper.cpp](https://github.com/ggml-org/whisper.cpp)、[transcribe-rs](https://github.com/cjpais/transcribe-rs) 和 [Silero VAD](https://github.com/snakers4/silero-vad)。
- 說話者標註：NVIDIA 的 [Streaming Sortformer v2.1](https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf)。
- 回音消除：[LocalVQE](https://github.com/localai-org/LocalVQE)，Apache-2.0 授權。

## 授權

MIT，和 Handy 一樣。Handy 的版權聲明保留在 [LICENSE](LICENSE) 裡。
