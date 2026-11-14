const pgForm = document.getElementById("playground-form");
const pgFileInput = document.getElementById("pg-file");
const pgSourceLang = document.getElementById("pg-source-lang");
const pgTargetLang = document.getElementById("pg-target-lang");
const pgRunBtn = document.getElementById("pg-run-btn");
const pgClearBtn = document.getElementById("pg-clear-btn");
const pgDownloadBtn = document.getElementById("pg-download-btn");
const pgStatusBox = document.getElementById("pg-status-box");
const pgSrcFrame = document.getElementById("pg-src-frame");
const pgOutFrame = document.getElementById("pg-out-frame");
const pgBoxesTbody = document.getElementById("pg-boxes-tbody");

const chatForm = document.getElementById("chat-form");
const chatInput = document.getElementById("chat-input");
const chatSendBtn = document.getElementById("chat-send-btn");
const chatStream = document.getElementById("chat-stream");
const chatSrcLang = document.getElementById("chat-src-lang");
const chatTgtLang = document.getElementById("chat-tgt-lang");
const chatClearBtn = document.getElementById("chat-clear-btn");

function escapeHtml(str) {
    return String(str).replace(/[&<>"']/g, (m) => ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;"
    })[m]);
}

pgFileInput.addEventListener("change", () => {
    const file = pgFileInput.files && pgFileInput.files[0];
    if (!file) {
        selectedImageDataUrl = "";
        pgSrcFrame.innerHTML = '<span class="empty-text">[EMPTY] No source image selected.</span>';
        return;
    }
    const reader = new FileReader();
    reader.onload = () => {
        selectedImageDataUrl = String(reader.result || "");
        pgSrcFrame.innerHTML = "";
        const img = document.createElement("img");
        img.src = selectedImageDataUrl;
        img.alt = "Selected source comic page";
        pgSrcFrame.appendChild(img);
        setBanner(
            pgStatusBox,
            "loading",
            "READY",
            "Loaded " + file.name + " (" + Math.round(file.size / 1024) + " KB). Click Run translation."
        );
    };
    reader.readAsDataURL(file);
});

pgClearBtn.addEventListener("click", () => {
    pgFileInput.value = "";
    selectedImageDataUrl = "";
    pgDownloadBtn.style.display = "none";
    pgSrcFrame.innerHTML = '<span class="empty-text">[EMPTY] No source image selected.</span>';
    pgOutFrame.innerHTML = '<span class="empty-text">[EMPTY] Run translation to render output image.</span>';
    pgBoxesTbody.innerHTML = '<tr><td colspan="4" class="mono" style="color:var(--muted);">[EMPTY] No dialogue boxes processed in this session.</td></tr>';
    setBanner(pgStatusBox, "loading", "IDLE", "Select an image file above and click Run translation.");
});

pgForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!selectedImageDataUrl) {
        setBanner(pgStatusBox, "err", "ERROR", "No image selected. Choose a .png, .jpg, or .webp file first.");
        pgFileInput.focus();
        return;
    }

    pgRunBtn.disabled = true;
    pgRunBtn.textContent = "Translating...";
    setBanner(pgStatusBox, "loading", "LOADING", "Running DirectML OCR, translation, and bubble inpainting...");

    const bodyParams = new URLSearchParams();
    bodyParams.set("src", selectedImageDataUrl);
    bodyParams.set("sourceLang", pgSourceLang.value);
    bodyParams.set("targetLang", pgTargetLang.value);

    const t0 = performance.now();
    try {
        const res = await fetch("/translate", {
            method: "POST",
            headers: { "Content-Type": "application/x-www-form-urlencoded" },
            body: bodyParams.toString()
        });
        const data = await res.json();
        const elapsedMs = Math.round(performance.now() - t0);

        if (!data.success) {
            setBanner(pgStatusBox, "err", "ERROR", data.message || "Translation pipeline failed.");
            return;
        }

        if (data.img) {
            const outDataUrl = "data:image/webp;base64," + data.img;
            pgOutFrame.innerHTML = "";
            const outImg = document.createElement("img");
            outImg.src = outDataUrl;
            outImg.alt = "Translated comic page output";
            pgOutFrame.appendChild(outImg);
            pgDownloadBtn.href = outDataUrl;
            pgDownloadBtn.style.display = "inline-flex";
        }

        const boxes = (data.imgMap && Array.isArray(data.imgMap.boxes)) ? data.imgMap.boxes : [];
        if (boxes.length === 0) {
            pgBoxesTbody.innerHTML = '<tr><td colspan="4" class="mono" style="color:var(--muted);">[EMPTY] No speech bubbles detected on this image.</td></tr>';
        } else {
            pgBoxesTbody.innerHTML = boxes.map((b, idx) => {
                const g = b.geometry || { X: 0, Y: 0, width: 0, height: 0 };
                return `<tr>
                    <td class="mono">${idx + 1}</td>
                    <td>${escapeHtml(b.text || "")}</td>
                    <td>${escapeHtml(b.target || "")}</td>
                    <td class="mono">${g.X}, ${g.Y}, ${g.width}x${g.height}</td>
                </tr>`;
            }).join("");
        }

        setBanner(pgStatusBox, "ok", "DONE", "Translated " + boxes.length + " dialogue box(es) in " + elapsedMs + " ms.");
        loadRuntimeState(false);
    } catch (err) {
        setBanner(pgStatusBox, "err", "ERROR", "Request failed during /translate: " + err.message);
    } finally {
        pgRunBtn.disabled = false;
        pgRunBtn.textContent = "Run translation";
    }
});

chatClearBtn.addEventListener("click", () => {
    chatStream.innerHTML = `
        <div class="chat-row incoming">
            <div class="chat-bubble bubble-incoming">
                <div class="bubble-meta">
                    <span>MyTranslation</span>
                    <span>SYSTEM</span>
                </div>
                <div class="bubble-body">[READY] Enter dialogue text in the field below. Translated responses appear here on the left side in real time.</div>
            </div>
        </div>
    `;
    chatInput.value = "";
    chatInput.focus();
});

chatInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || !e.shiftKey)) {
        e.preventDefault();
        chatForm.dispatchEvent(new Event("submit", { cancelable: true }));
    }
});

chatForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    const text = chatInput.value.trim();
    if (!text) {
        chatInput.focus();
        return;
    }

    const srcLang = chatSrcLang.value;
    const tgtLang = chatTgtLang.value;
    const timeStr = formatClock();
    const replyId = "reply-" + Date.now();

    chatStream.insertAdjacentHTML("beforeend", `
        <div class="chat-row outgoing">
            <div class="chat-bubble bubble-outgoing">
                <div class="bubble-meta">You &bull; ${timeStr}</div>
                <div class="bubble-body">${escapeHtml(text)}</div>
            </div>
        </div>
        <div class="chat-row incoming" id="${replyId}">
            <div class="chat-bubble bubble-incoming">
                <div class="bubble-meta">
                    <span>MyTranslation (${escapeHtml(activeModelName)})</span>
                    <span class="bubble-time">${timeStr}</span>
                </div>
                <div class="bubble-body">[TRANSLATING] Processing dialogue with ${escapeHtml(activeModelName)}...</div>
            </div>
        </div>
    `);

    chatInput.value = "";
    chatSendBtn.disabled = true;
    chatSendBtn.textContent = "Translating...";
    chatStream.scrollTop = chatStream.scrollHeight;

    const replyRow = document.getElementById(replyId);
    const replyBody = replyRow ? replyRow.querySelector(".bubble-body") : null;
    const replyTime = replyRow ? replyRow.querySelector(".bubble-time") : null;

    try {
        const res = await fetch("/api/translate/text", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
                text: text,
                sourceLang: srcLang,
                targetLang: tgtLang
            })
        });
        const data = await res.json();
        if (data.success && replyBody) {
            replyBody.textContent = data.target || "";
            if (replyTime) replyTime.textContent = data.elapsed_ms + " ms \u2022 " + formatClock();

            const actions = document.createElement("div");
            actions.className = "bubble-actions";
            const copyBtn = document.createElement("button");
            copyBtn.type = "button";
            copyBtn.className = "bubble-btn";
            copyBtn.textContent = "Copy text";
            copyBtn.addEventListener("click", async () => {
                try {
                    await navigator.clipboard.writeText(data.target || "");
                    copyBtn.textContent = "Copied";
                    setTimeout(() => { copyBtn.textContent = "Copy text"; }, 1500);
                } catch (_) {}
            });
            actions.appendChild(copyBtn);
            replyRow.querySelector(".chat-bubble").appendChild(actions);
        } else if (replyBody) {
            replyBody.textContent = "[ERROR] " + (data.message || "Translation failed.");
            replyBody.style.color = "var(--err-text)";
        }
    } catch (err) {
        if (replyBody) {
            replyBody.textContent = "[ERROR] Network error during translation: " + err.message;
            replyBody.style.color = "var(--err-text)";
        }
    } finally {
        chatSendBtn.disabled = false;
        chatSendBtn.textContent = "Send";
        chatStream.scrollTop = chatStream.scrollHeight;
        chatInput.focus();
    }
});
