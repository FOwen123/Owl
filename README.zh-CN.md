<img src="src-tauri/icons/128x128@2x.png" width="96" alt="Owl 图标">

# Owl

[English](README.md) | 简体中文 | [繁體中文](README.zh-TW.md)

**私密的语音输入和会议记录，专为中英文打造。开源的 Wispr Flow 替代品。**

按下 Option+Space 开口说话，Owl 会把你说的内容打进当前的应用里。语音识别在你自己的电脑上运行。Owl 还能去掉口头禅、用语音修改选中的文字，并记录会议。

Owl 最初是 CJ Pais 的 [Handy](https://github.com/cjpais/Handy) 的一个分支，现在独立开发，和 Handy 没有关联，所以 Owl 的问题请在这里提交。它的外观参考了 [Wispr Flow](https://wisprflow.ai)，同样和它没有关联。

![语音输入设置](docs/screenshots/dictation.png)

## 功能

- **语音输入。** 说话时按住 Option+Space，或者按一下开始、再按一下结束。按 Escape 取消。如果结束时没有选中任何文本框，Owl 会把文字留在剪贴板里，并显示一个“复制”按钮。
- **中文和英文。** 有几个语音模型能识别同一句话里夹杂的普通话和英文。Owl 可以用繁体或简体输出中文。
- **整理。** Owl 在你的电脑上去掉口头禅和重复的字。接入 AI 服务后，它还会修正语法、数字和列表。
- **用语音修改。** 选中文字，按 Option+Shift+Space，说出要怎么改。需要 AI 服务。
- **词典和片段。** 把 Owl 常听错的名字教给它，或者保存一段文字，说出“我的邮箱”这样的短语就能粘贴。如果你在语音输入后马上改了某个词，Owl 会提示你把它加进词典。
- **学习。** 默认关闭。Owl 会尝试调整自己的设置，只保留在你最近的语音输入上得分更高的改动。每一项都可以撤销。
- **会议记录。** Owl 同时录下你的麦克风和通话声音，实时生成文字稿，并标出每句话是谁说的。接入 AI 服务后，它还会写会议纪要。
- **命令行和 AI 智能体。** `owl` 命令可以输出和转写会议，`owl mcp` 让 Claude Code 这样的智能体读取会议内容。

![会议记录](docs/screenshots/meeting.png)

截图使用的是 Owl 的演示数据。

## 隐私

语音识别、说话人标注和回声消除都在你的电脑上运行。只有当你开启 AI 步骤并选择云端服务时，音频和文字才会离开你的电脑。除此之外，唯一的联网操作是下载你选择的模型。

AI 步骤可以使用 macOS 26 上的 Apple Intelligence、Ollama 这样的本地服务，或者用你自己的密钥接入 OpenAI、Anthropic、OpenRouter、Groq、Cerebras、Z.AI 或 Amazon Bedrock。你也可以登录 ChatGPT，使用你的订阅。

Owl 会保留最近 5 次语音输入的音频，以及你手动保存的音频。数据存放在 `~/Library/Application Support/com.hao.owl/`。

## 状态

还在早期阶段，为一个人的日常使用而做。目前没有可下载的版本，需要自己构建。我在 macOS 26 上构建和使用它。Windows 和 Linux 的代码还在，但没有测试过。

## 环境要求

- [Rust](https://rustup.rs) 1.99 或更新版本
- [Bun](https://bun.sh) 1.4 或更新版本
- [CMake](https://cmake.org/download/)

会议记录需要 macOS 14.2 或更新版本才能录到通话另一方的声音。Windows 和 Linux 需要什么，请看 [Handy 的构建说明](https://github.com/cjpais/Handy/blob/main/BUILD.md)。

## 安装

### 1. 构建

```bash
git clone https://github.com/FOwen123/Owl.git
cd Owl
bun install
bun run tauri build
```

第一次构建需要一段时间。如果 CMake 报错说策略版本太旧，请运行 `CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build`。

### 2. 安装并打开

```bash
cp -R src-tauri/target/release/bundle/macos/Owl.app /Applications/
open /Applications/Owl.app
```

Owl 会请求麦克风和辅助功能权限。辅助功能权限用来把文字粘贴到其他应用里。然后选择一个语音模型下载。Qwen3-ASR 1.7B 擅长中英混说，Parakeet Unified 速度快，但只支持英文。

第一次开会时，Owl 还会请求系统录音（System Audio Recording）权限，这样才能听到通话的声音。

### 更新

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

每次重新构建，macOS 都会把它当成一个新应用，之前打开的辅助功能开关就会失效。`tccutil` 这一行会清掉旧的授权，Owl 会重新请求。你的设置、历史记录和会议都会保留。

### 卸载

退出 Owl，删除 `/Applications/Owl.app`。如果也要删除数据，再删除 `~/Library/Application Support/com.hao.owl/`。

## 命令行

在 Owl 设置的 **高级 > 其他应用 > 命令行** 里安装 `owl` 命令。

```bash
owl meetings                 # 列出会议
owl meeting 12               # 以 Markdown 输出一场会议
owl transcribe call.m4a      # 把一个文件转写成新会议
owl --toggle-transcription   # 开始或停止语音输入
```

要让 Claude Code 读取你的会议和语音输入，运行 `claude mcp add owl -- owl mcp`。它只能读取，不能修改。

## 常见问题

### 快捷键失效了（安全输入）

很可能是 macOS 的安全输入（Secure Input）开着。密码框和一些应用会打开它，通常会自己关掉。要找出是哪个应用打开的，运行 `ioreg -l -w 0 | grep kCGSSessionSecureInputPID`，再运行 `ps -p <PID>`，然后退出那个应用。

### 重新构建后，辅助功能一直显示“等待中...”

退出 Owl，运行 `tccutil reset Accessibility com.hao.owl`，再打开 Owl 并重新授权。

### 会议里只有我这边的声音

在 **系统设置 > 隐私与安全性 > 系统录音（System Audio Recording）** 里打开 Owl。

更多内容（包括构建错误）请看 [BUILD.md](BUILD.md)（英文）。

## 参与贡献

欢迎提交 issue 和 pull request。[BUILD.md](BUILD.md) 介绍开发流程和需要运行的检查，[AGENTS.md](AGENTS.md) 介绍代码结构，[DESIGN.md](DESIGN.md) 介绍视觉设计。这些文档都是英文的。

## 致谢

- CJ Pais 和贡献者们的 [Handy](https://github.com/cjpais/Handy)。Owl 基于它构建。
- 语音识别：[transcribe.cpp](https://github.com/handy-computer/transcribe.cpp)、[ggml / whisper.cpp](https://github.com/ggml-org/whisper.cpp)、[transcribe-rs](https://github.com/cjpais/transcribe-rs) 和 [Silero VAD](https://github.com/snakers4/silero-vad)。
- 说话人标注：NVIDIA 的 [Streaming Sortformer v2.1](https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf)。
- 回声消除：[LocalVQE](https://github.com/localai-org/LocalVQE)，Apache-2.0 许可。

## 许可证

MIT，和 Handy 一样。Handy 的版权声明保留在 [LICENSE](LICENSE) 里。
