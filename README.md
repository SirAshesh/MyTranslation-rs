# MyTranslation-rs

[![Release](https://img.shields.io/github/v/release/SirAshesh/MyTranslation-rs?display_name=tag)](https://github.com/SirAshesh/MyTranslation-rs/releases)
[![Last Commit](https://img.shields.io/github/last-commit/SirAshesh/MyTranslation-rs)](https://github.com/SirAshesh/MyTranslation-rs/commits/main)
![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?logo=rust)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-blue)

[![MyTranslation-rs Preview](sample/preview.gif)](https://id.pinterest.com/pin/1005569423056533419/)

A fast, lightweight, and native Rust translation and typesetting server for manga, manhwa, and comics. It features built-in RapidOCR (DirectML on Windows, high-performance ONNX Runtime on Linux), stroke-level bubble cleaning that preserves screentones, automatic typesetting with `comic_bold.ttf`, and translation powered by cloud APIs (DeepSeek, OpenAI, Claude) or fully local on-device LLMs (LiquidAI LFM3 1.7B via llama.cpp with CUDA, Vulkan, or CPU acceleration).

## Key Features
- **Cross-Platform**: Runs natively on both **Windows 10/11 (x64)** and **Linux / Ubuntu (x64)**.
- **Single Standalone Binary**: No Python environment, PyTorch, or heavy runtime dependencies required.
- **Multilingual Support**: Supports horizontal comics (LTR) and vertical Japanese manga (tategaki, RTL) with automatic line joining and junk/SFX filtering.
- **Flexible Translation Backends**:
  - **Local Model**: 1-click built-in download and execution of LiquidAI LFM3 1.7B with automatic GPU detection (NVIDIA CUDA, AMD/Intel Vulkan, or CPU).
  - **Cloud APIs**: OpenAI-compatible (DeepSeek, OpenAI, OpenRouter) and Claude-compatible endpoints.
- **ImageTrans Extension Ready**: Direct compatibility with the ImageTrans Chrome/Firefox browser extension.

## Sample

| RAW | Translated |
| :--- | :--- |
| ![RAW Sample](sample/raw.png) | ![Translated Sample](sample/translated.png) |

Source: https://id.pinterest.com/pin/614600680437863284/

## Quick Start (Prebuilt Release)
### Windows (x64)

1. Download `ImageTrans-rs-windows-x64.zip` from the [Releases](https://github.com/SirAshesh/MyTranslation-rs/releases) page.
2. Extract the archive into a folder.
3. Run `server-rs.exe`.
4. Open your browser at `http://127.0.0.1:5000`.

### Linux / Ubuntu (x64)

You can install, update, and run the server using the single-line automated runner:

```bash
curl -sSL https://raw.githubusercontent.com/SirAshesh/MyTranslation-rs/main/run.sh -o run.sh
chmod +x run.sh
./run.sh
```

The script automatically detects updates from GitHub Releases, downloads missing binaries/models, and launches the server.

Alternatively, for manual installation:
1. Download `ImageTrans-rs-ubuntu-x64.tar.gz` from the [Releases](https://github.com/SirAshesh/MyTranslation-rs/releases) page.
2. Extract the archive:
   ```bash
   tar -xzf ImageTrans-rs-ubuntu-x64.tar.gz
   cd ImageTrans-rs
   chmod +x server-rs run.sh
   ./server-rs
   ```
3. Open `http://127.0.0.1:5000` in your browser.


## Configuration

Settings are saved in `config.json` in the application directory:

```json
{
  "port": 5000,
  "translation_mode": "local",
  "openai_base_url": "https://api.deepseek.com",
  "openai_model": "deepseek-chat",
  "deepseek_api_key": "sk-your-key-here",
  "claude_base_url": "https://api.anthropic.com",
  "claude_model": "claude-3-5-sonnet-latest",
  "claude_api_key": "",
  "local_model": "Q6_K"
}
```

- **Web Dashboard**: Navigate to `http://127.0.0.1:5000` to adjust settings, check detected GPU hardware, and download local models directly from the UI.
- **Port**: Customize the listening port by modifying `"port"` in `config.json` or passing `PORT=5000 ./server-rs`.

## Connecting the ImageTrans Browser Extension

1. In Chrome/Brave/Edge, open the **ImageTrans** extension options.
2. Under **Translation Mode**, select `ImageTrans (local or remote server)`.
3. Set the **Server URL** to:
   ```
   http://127.0.0.1:5000
   ```
4. Click **Check** to verify connectivity (returns `[{"name":"MyTranslation","displayName":"default","running":false}]`).
5. Save your settings. Now you can capture and translate manga pages directly in your browser.


## Building from Source

### Prerequisites

- [Rust Toolchain](https://rustup.rs/) (1.75 or newer)
- **Windows**: Windows 10/11 x64, DirectX 12 compatible GPU for DirectML acceleration.
- **Ubuntu / Debian**:
  ```bash
  sudo apt-get update
  sudo apt-get install -y build-essential libssl-dev pkg-config
  ```

### Build Commands

```bash
git clone https://github.com/SirAshesh/MyTranslation-rs.git
cd MyTranslation-rs
cargo build --release
```

- **Windows binary**: `target/release/server-rs.exe` (requires `DirectML.dll` from the target folder).
- **Linux binary**: `target/release/server-rs`.

## Credit
- xulihang / [ImageTrans Chrome Extension](https://github.com/xulihang/ImageTrans_chrome_extension)
- koharu-rs / [Koharu](https://github.com/koharu-rs/koharu)

## License
MIT License.
