# ImageTrans-rs: High-Performance ImageTrans Server in Rust

A standalone ImageTrans server implementation written in pure Rust. Built specifically for high-speed, natural comic, manga, and manhwa scanlation.

---

## Key Features

1. **Sub-3-Second Performance**:
   - The entire local processing pipeline (decoding, OCR, speech bubble clustering, inpainting, and font typesetting) finishes in approximately 0.8 to 1.1 seconds on CPU.
   - Total turnaround time per high-resolution comic page (including DeepSeek V3 API response) is typically 2.2 to 2.9 seconds.
2. **Stroke-Masked Speech Bubble Inpainting**:
   - Zero ugly white boxes and zero cut-off speech bubble borders.
   - Two-tier inpainting scans the entire bubble envelope and individual line bounds, sampling local background colors and dilating text masks by 2 to 3 pixels.
   - Erases 100% of raw source text while keeping round and spiky speech bubble borders crisp and intact.
3. **Natural Localization with DeepSeek V3**:
   - Groups text lines into speech bubbles using horizontal center alignment and vertical proximity rules.
   - Comic dialogue is sent to DeepSeek V3 with dedicated scanlation guidelines (natural spoken dialogue, concise phrasing, and zero repetitive phrasing).
4. **Professional Comic Typography**:
   - Uses comic lettering (`fonts/comic_bold.ttf`) with dynamic font size fitting, automatic word wrapping, vertical and horizontal centering, and subtle 1px white stroke outlines for high legibility on any background.
5. **Bulk and Batch Processing**:
   - Built-in `POST /batch` endpoint with parallel execution via the Tokio async runtime, capable of translating multi-page chapters in seconds.
6. **Full ImageTrans Chrome Extension Compatibility**:
   - Supports `src`, `sourceLang`, `targetLang`, `withoutImage`, `type=html`, and `callback` (JSONP) form data parameters.

---

## Getting Started

### 1. Configure `.env`
Create or edit `.env`:
```env
DEEPSEEK_API=your_deepseek_api_key_here
PORT=5000
```

### 2. Run the Server
From the repository root:

```bash
# Release mode (recommended for maximum performance)
cargo run --release
```

Or run the compiled release binary directly:
```bash
./target/release/server-rs.exe
```

The server automatically launches the internal RapidOCR daemon (`ocr_daemon.py`) and listens on port 5000.

---

## Connecting with the ImageTrans Chrome Extension

1. In Chrome, right-click the **ImageTrans** extension icon and select **Options**.
2. Under **Translation Mode**, choose:
   `ImageTrans (local or remote server)`
3. In the **Server URL** input, enter:
   `http://localhost:5000` (or `http://127.0.0.1:5000`)
4. Click **Save**.
5. Click **Check** to verify connectivity (it opens `http://localhost:5000/list` showing `[{"name":"Server-rs", ...}]`).

---

## API Endpoints

| Endpoint | Method | Input Format | Description |
| :--- | :--- | :--- | :--- |
| `GET /list` | GET | None | Server discovery endpoint for ImageTrans extensions |
| `POST /translate` | POST | Form data or JSON | Full-page image translation with inpainting and rendering |
| `POST /translateRegion` | POST | Form data | Single bubble or rectangular region translation |
| `POST /batch` | POST | JSON `{"images": ["data:image/..."]}` | High-speed parallel batch translation for multiple pages |
| `GET /stats` | GET | None | Processed image statistics and uptime |
