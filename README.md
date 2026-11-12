# MyTranslation-rs

A native Rust translation and typesetting server for manga, manhwa, and comic pages. It runs RapidOCR on Windows DirectML for text detection and recognition, cleans speech bubbles at the stroke level while keeping screentones intact, typesets translated lines with `comic_bold.ttf`, and translates dialogue through OpenAI-compatible endpoints (DeepSeek, OpenAI, OpenRouter) or Claude-compatible endpoints.

## Architecture

- Runs as a single compiled Rust binary (`server-rs.exe`) with no Python runtime, PyTorch installation, or background helper daemons.
- Executes PP-OCRv4 mobile detection and recognition models on GPU through ONNX Runtime and `DirectML.dll`.
- Samples local bubble backgrounds and dilates text stroke masks before rendering wrapped text with a 1px outline.

## Quick Start (Prebuilt Release)

1. Download `ImageTrans-rs-windows-x64.zip` from the [Releases](https://github.com/SirAshesh/ImageTrans-rs/releases) page.
2. Extract the archive to a local folder.
3. Run `server-rs.exe`.
4. Open `http://127.0.0.1:5000` in your browser.
5. In **Settings**, choose your preferred translation mode, enter your API key or endpoint, and click **Save settings**.
6. Point the ImageTrans browser extension to `http://127.0.0.1:5000`.

## Sample

<video src="sample/Sample.mp4" controls="controls" muted="muted" width="100%"></video> or [Click Here](./sample/Sample.mp4)

## Configuration

Settings live in `config.json` in the working directory:

```json
{
  "port": 5000,
  "translation_mode": "openai",
  "openai_base_url": "https://api.deepseek.com",
  "openai_model": "deepseek-chat",
  "deepseek_api_key": "sk-your-key-here",
  "claude_base_url": "https://api.anthropic.com",
  "claude_model": "claude-3-5-sonnet-latest",
  "claude_api_key": ""
}
```

The listening port is managed in `config.json`. Translation backend settings can be updated either through the web interface or by directly editing `config.json`.

## Connecting the ImageTrans Browser Extension

1. In Chrome, right-click the **ImageTrans** extension icon and open **Options**.
2. Set **Translation Mode** to `ImageTrans (local or remote server)`.
3. Enter `http://127.0.0.1:5000` in the **Server URL** field and click **Save**.
4. Click **Check** to confirm `/list` returns `[{"name":"MyTranslation","displayName":"default","running":false}]`.

## Building from Source

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (1.75 or newer)
- Windows 10/11 x64 with DirectX 12 support

### Build Commands
```bash
git clone https://github.com/SirAshesh/ImageTrans-rs.git
cd ImageTrans-rs
cargo build --release
```

The release build places `server-rs.exe` and `DirectML.dll` in `target/release/`.

## License
MIT License.
