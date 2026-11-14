# Connect Local Client

Once the server is installed and running (see [Installation Guide](Installation.md)), follow these steps to configure your translation backend and connect the browser extension to the local server.

## 1. Configure the Server Dashboard

1. Open the Web Dashboard in your browser at:
   ```
   http://127.0.0.1:5000
   ```
2. Complete first-run account setup if prompted, then open **Settings**.
3. Choose your preferred translation mode:
   - **Local Model**: Select **Tencent HY-MT1.5 1.8B** (recommended for comic translation) or **LiquidAI LFM3 1.7B**, or click **Auto-detect** to match your detected GPU/RAM. Click **Download & start** to download the model and `llama.cpp` inference engine automatically.
   - **Cloud API**: Enter your OpenAI-compatible or Claude-compatible endpoint URL, model ID, and API key.
4. Click **Save settings**.

## 2. Connect the Browser Extension

1. For **MyTranslation Extension**, download `chrome-extension.zip` (Chrome, Brave, Edge) or `firefox-extension.zip` (Firefox) from the [Releases](https://github.com/SirAshesh/MyTranslation-rs/releases) page and load the unpacked folder (or use `extension/chrome` / `extension/firefox` from this repository).
2. Open the **ImageTrans Extension** or **MyTranslation Extension** options page and set the **Server URL** to:
   ```
   http://127.0.0.1:5000
   ```
3. If using **ImageTrans**, set **Translation Mode** to `ImageTrans (local or remote server)`.
4. Click **Test Connection** / **Check** to verify connectivity with the server.
5. Save your extension settings. You can now capture and translate comic, manga, or webtoon pages directly in your browser.
