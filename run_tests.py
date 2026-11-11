import os
import sys
import time
import base64
import requests
from PIL import Image

SERVER_URL = "http://127.0.0.1:5000"

print("="*60)
print("TEST 1: Server Status and Discovery (/list)")
print("="*60)

r = requests.get(f"{SERVER_URL}/list")
print(f"GET /list: HTTP {r.status_code}")
print("Response:", r.json())
assert r.status_code == 200, "Failed /list"

print("\n" + "="*60)
print("TEST 2: Single Image POST /translate (ImageTrans format)")
print("="*60)

test_img_path = "Server-rs/dummy/The_Witch_Asked_to_Make_a_Love_Potion_for_Her_Crus (1).webp"
with open(test_img_path, "rb") as f:
    img_b64 = base64.b64encode(f.read()).decode("utf-8")

payload = {
    "src": f"data:image/webp;base64,{img_b64}",
    "sourceLang": "en",
    "targetLang": "id",
    "withoutImage": "false",
    "displayName": "default"
}

t0 = time.time()
resp = requests.post(f"{SERVER_URL}/translate", data=payload, timeout=30)
t_total = time.time() - t0

print(f"POST /translate: HTTP {resp.status_code} in {t_total:.2f}s")
assert resp.status_code == 200, "Failed /translate"

data = resp.json()
print("Success:", data.get("success"))
img_map = data.get("imgMap", {})
boxes = img_map.get("boxes", [])
print(f"Total translated speech bubbles: {len(boxes)}")

print("\n--- SAMPLE TRANSLATIONS ---")
for i, b in enumerate(boxes[:6]):
    print(f"Bubble #{i+1}:")
    print(f"  EN: {b.get('text')}")
    print(f"  ID: {b.get('target')}")

if data.get("img"):
    out_bytes = base64.b64decode(data["img"])
    out_file = "Server-rs/dummy/translated_test1.webp"
    with open(out_file, "wb") as f:
        f.write(out_bytes)
    print(f"\n[OK] Translated image saved to {out_file} ({len(out_bytes)} bytes)")

print("\n" + "="*60)
print("TEST 3: Bulk / Batch Translation (/batch)")
print("="*60)

batch_files = [
    "Server-rs/dummy/The_Witch_Asked_to_Make_a_Love_Potion_for_Her_Crus (1).webp",
    "Server-rs/dummy/The_Witch_Asked_to_Make_a_Love_Potion_for_Her_Crus (2).webp",
    "Server-rs/dummy/The_Witch_Asked_to_Make_a_Love_Potion_for_Her_Crus (3).webp",
]

batch_payload = {
    "images": [],
    "sourceLang": "en",
    "targetLang": "id"
}

for bf in batch_files:
    with open(bf, "rb") as f:
        batch_payload["images"].append("data:image/webp;base64," + base64.b64encode(f.read()).decode("utf-8"))

t0 = time.time()
batch_resp = requests.post(f"{SERVER_URL}/batch", json=batch_payload, timeout=60)
t_batch = time.time() - t0

print(f"POST /batch (3 full comic pages): HTTP {batch_resp.status_code} in {t_batch:.2f}s")
batch_data = batch_resp.json()
print("Batch success:", batch_data.get("success"))
print(f"Processed: {batch_data.get('total')} images in {batch_data.get('elapsed_sec')}s")

for i, res in enumerate(batch_data.get("results", [])):
    num_boxes = len(res.get("imgMap", {}).get("boxes", []))
    print(f"  Page {i+1}: {num_boxes} bubbles translated")
    if res.get("img"):
        out_f = f"Server-rs/dummy/translated_batch_{i+1}.webp"
        with open(out_f, "wb") as f:
            f.write(base64.b64decode(res["img"]))
        print(f"    Saved to {out_f}")

print("\n" + "="*60)
print("ALL TESTS PASSED SUCCESSFULLY!")
print("="*60)
