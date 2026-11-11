import io
import sys
import json
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from rapidocr_onnxruntime import RapidOCR
from PIL import Image
import numpy as np

PORT = 51055
print(f"[OCR Daemon] Initializing RapidOCR model...", flush=True)
engine = RapidOCR()
for op in engine.text_detector.preprocess_op:
    if hasattr(op, 'limit_side_len'):
        op.limit_side_len = 1200

dummy_arr = np.zeros((100, 100, 3), dtype=np.uint8)
engine(dummy_arr)
print(f"[OCR Daemon] Model ready and warmed up. Starting HTTP server on port {PORT}...", flush=True)

class OCRHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"status":"ok","ready":true}')
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path == "/ocr":
            try:
                content_length = int(self.headers.get("Content-Length", 0))
                if content_length == 0:
                    self.send_response(400)
                    self.end_headers()
                    self.wfile.write(b'{"error":"empty body"}')
                    return

                image_bytes = self.rfile.read(content_length)
                img = Image.open(io.BytesIO(image_bytes)).convert("RGB")
                
                t0 = time.time()
                ocr_results, elapse = engine(image_bytes)
                elapsed = time.time() - t0

                lines = []
                if ocr_results:
                    for item in ocr_results:
                        pts = item[0]
                        text = str(item[1]).strip()
                        score = float(item[2])
                        lines.append({
                            "box": pts,
                            "text": text,
                            "score": score
                        })

                resp_data = {
                    "success": True,
                    "width": img.width,
                    "height": img.height,
                    "elapsed_sec": round(elapsed, 3),
                    "lines": lines
                }
                resp_bytes = json.dumps(resp_data).encode("utf-8")

                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(resp_bytes)))
                self.end_headers()
                self.wfile.write(resp_bytes)
            except Exception as e:
                err_bytes = json.dumps({"success": False, "error": str(e)}).encode("utf-8")
                self.send_response(500)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(err_bytes)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, format, *args):
        sys.stderr.write(f"[OCR Daemon] {args[0]} {args[1]} -> {args[2]}\n")

def run():
    server_address = ("127.0.0.1", PORT)
    httpd = HTTPServer(server_address, OCRHandler)
    print(f"[OCR Daemon] Listening on http://127.0.0.1:{PORT}/ocr", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("[OCR Daemon] Shutting down...", flush=True)

if __name__ == "__main__":
    run()
