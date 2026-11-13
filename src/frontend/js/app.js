const navButtons = Array.from(document.querySelectorAll(".nav-btn"));
const viewPanels = {
    home: document.getElementById("panel-home"),
    playground: document.getElementById("panel-playground"),
    translation: document.getElementById("panel-translation"),
    settings: document.getElementById("panel-settings")
};

const modeButtons = Array.from(document.querySelectorAll(".mode-btn"));
const modeSections = {
    openai: document.getElementById("section-openai"),
    claude: document.getElementById("section-claude"),
    local: document.getElementById("section-local")
};

let activeMode = "openai";
let activeModelName = "deepseek-chat";
let selectedImageDataUrl = "";

const themeBtn = document.getElementById("theme-btn");
const themeLabel = document.getElementById("theme-label");
const logoutBtn = document.getElementById("logout-btn");
const refreshBtn = document.getElementById("refresh-btn");
const copyUrlBtn = document.getElementById("copy-url-btn");
const gotoSettingsLink = document.getElementById("goto-settings-link");
const serverUrlCode = document.getElementById("server-url-code");
const homeStatusBox = document.getElementById("home-status-box");
const settingsStatusBox = document.getElementById("settings-status-box");

const statStatus = document.getElementById("stat-status");
const statMode = document.getElementById("stat-mode");
const statModel = document.getElementById("stat-model");
const statKey = document.getElementById("stat-key");
const statPages = document.getElementById("stat-pages");
const statPort = document.getElementById("stat-port");

const openaiUrlInput = document.getElementById("openai-url");
const openaiModelInput = document.getElementById("openai-model");
const openaiKeyInput = document.getElementById("openai-key");
const toggleOpenaiKeyBtn = document.getElementById("toggle-openai-key");

const claudeUrlInput = document.getElementById("claude-url");
const claudeModelInput = document.getElementById("claude-model");
const claudeKeyInput = document.getElementById("claude-key");
const toggleClaudeKeyBtn = document.getElementById("toggle-claude-key");

let savedLocalModel = "Q6_K";
const localModelSelect = document.getElementById("local-model");

const settingsForm = document.getElementById("settings-form");
const saveBtn = document.getElementById("save-btn");
const testBtn = document.getElementById("test-btn");

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

function setBanner(el, state, prefix, text) {
    el.setAttribute("data-state", state);
    el.textContent = "[" + prefix + "] " + text;
}

function switchView(target) {
    navButtons.forEach((btn) => {
        const isMatch = btn.getAttribute("data-target") === target;
        btn.setAttribute("aria-selected", isMatch ? "true" : "false");
    });
    Object.keys(viewPanels).forEach((k) => {
        viewPanels[k].classList.toggle("active", k === target);
    });
}

navButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
        switchView(btn.getAttribute("data-target"));
    });
});

gotoSettingsLink.addEventListener("click", (e) => {
    e.preventDefault();
    switchView("settings");
});

function selectMode(mode) {
    activeMode = mode;
    modeButtons.forEach((btn) => {
        const isMatch = btn.getAttribute("data-mode") === mode;
        btn.setAttribute("aria-checked", isMatch ? "true" : "false");
    });
    Object.keys(modeSections).forEach((k) => {
        if (modeSections[k]) {
            modeSections[k].classList.toggle("active", k === mode);
        }
    });
    if (mode === "local" && typeof pollLocalStatus === "function") {
        pollLocalStatus();
    }
}

modeButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
        selectMode(btn.getAttribute("data-mode"));
    });
});

function applyTheme(theme) {
    document.documentElement.setAttribute("data-theme", theme);
    const isLight = theme === "light";
    themeBtn.setAttribute("aria-pressed", isLight ? "true" : "false");
    if (themeLabel) {
        themeLabel.textContent = isLight ? "Theme: Light" : "Theme: Dark";
    }
    try {
        localStorage.setItem("mytranslation_theme", theme);
    } catch (_) {}
}

try {
    const savedTheme = localStorage.getItem("mytranslation_theme");
    if (savedTheme === "light" || savedTheme === "dark") {
        applyTheme(savedTheme);
    }
} catch (_) {}

themeBtn.addEventListener("click", () => {
    const current = document.documentElement.getAttribute("data-theme") || "dark";
    applyTheme(current === "dark" ? "light" : "dark");
});

if (logoutBtn) {
    logoutBtn.addEventListener("click", async () => {
        logoutBtn.disabled = true;
        try {
            await fetch("/api/auth/logout", { method: "POST" });
        } catch (_) {}
        window.location.href = "/login";
    });
}

function bindPasswordToggle(btn, input) {
    btn.addEventListener("click", () => {
        const showing = input.type === "text";
        input.type = showing ? "password" : "text";
        btn.textContent = showing ? "Show" : "Hide";
        btn.setAttribute("aria-pressed", showing ? "false" : "true");
    });
}

bindPasswordToggle(toggleOpenaiKeyBtn, openaiKeyInput);
bindPasswordToggle(toggleClaudeKeyBtn, claudeKeyInput);

copyUrlBtn.addEventListener("click", async () => {
    const urlText = serverUrlCode.textContent.trim();
    try {
        await navigator.clipboard.writeText(urlText);
        copyUrlBtn.textContent = "Copied";
        setTimeout(() => {
            copyUrlBtn.textContent = "Copy URL";
        }, 1500);
    } catch (_) {
        setBanner(homeStatusBox, "warn", "NOTICE", "Clipboard permission denied. Select and copy " + urlText + " manually.");
    }
});

function formatModeName(mode) {
    if (mode === "claude") return "Claude Compatible Endpoint";
    if (mode === "local") return "Local Model (LFM / GGUF)";
    if (mode === "free") return "Free-Tier LLM API";
    return "OpenAI Compatible Endpoint";
}

function formatClock() {
    const d = new Date();
    const h = String(d.getHours()).padStart(2, "0");
    const m = String(d.getMinutes()).padStart(2, "0");
    const s = String(d.getSeconds()).padStart(2, "0");
    return h + ":" + m + ":" + s;
}

async function loadRuntimeState(announce) {
    if (announce) {
        setBanner(homeStatusBox, "loading", "LOADING", "Refreshing runtime status...");
    }
    try {
        const [settingsRes, statsRes] = await Promise.all([
            fetch("/api/settings"),
            fetch("/stats")
        ]);
        if (settingsRes.status === 401 || statsRes.status === 401) {
            window.location.href = "/login";
            return;
        }
        if (!settingsRes.ok || !statsRes.ok) {
            throw new Error("Server returned HTTP " + settingsRes.status);
        }
        const s = await settingsRes.json();
        const st = await statsRes.json();

        const port = s.port || 5000;
        statPort.textContent = String(port);
        serverUrlCode.textContent = "http://127.0.0.1:" + port;
        statStatus.textContent = st.status === "online" ? "Online" : "Degraded";
        statPages.textContent = String(st.total_processed ?? 0);

        let mode = s.translation_mode || "openai";
        const supportsFreeMode = Boolean(window.freeModeHandlers);
        if (mode !== "claude" && mode !== "openai" && mode !== "local" && !(supportsFreeMode && mode === "free")) {
            mode = "openai";
        }
        selectMode(mode);
        statMode.textContent = formatModeName(mode);

        openaiUrlInput.value = s.openai_base_url || "https://api.deepseek.com";
        openaiModelInput.value = s.openai_model || "deepseek-chat";
        openaiKeyInput.placeholder = s.openai_key_configured ? "Saved (" + s.openai_masked_key + ")" : "Enter sk-...";

        claudeUrlInput.value = s.claude_base_url || "https://api.anthropic.com";
        claudeModelInput.value = s.claude_model || "claude-3-5-sonnet-latest";
        claudeKeyInput.placeholder = s.claude_key_configured ? "Saved (" + s.claude_masked_key + ")" : "Enter sk-ant-...";

        if (supportsFreeMode) {
            window.freeModeHandlers.applyState(s);
        }

        if (s.local_model) {
            savedLocalModel = s.local_model;
            if (localModelSelect) {
                localModelSelect.value = s.local_model;
            }
        }

        if (mode === "claude") {
            activeModelName = s.claude_model || "claude-3-5-sonnet-latest";
            statModel.textContent = activeModelName;
            statKey.textContent = s.claude_key_configured ? s.claude_masked_key : "Not set";
        } else if (mode === "local") {
            activeModelName = "LiquidAI LFM3 1.7B (" + (s.local_model || "Q6_K") + ")";
            statModel.textContent = activeModelName;
            statKey.textContent = "Offline (Local)";
        } else if (mode === "free" && supportsFreeMode) {
            const summary = window.freeModeHandlers.getSummary(s);
            activeModelName = summary.modelName;
            statModel.textContent = activeModelName;
            statKey.textContent = summary.keyStatus;
        } else {
            activeModelName = s.openai_model || "deepseek-chat";
            statModel.textContent = activeModelName;
            statKey.textContent = s.openai_key_configured ? s.openai_masked_key : "Not set";
        }

        if (s.active_configured) {
            const readyText = "Active mode (" + formatModeName(mode) + ") is configured and ready.";
            setBanner(homeStatusBox, "ok", "READY", readyText);
            if (settingsStatusBox.getAttribute("data-state") === "loading") {
                setBanner(settingsStatusBox, "ok", "READY", readyText);
            }
        } else {
            const emptyText = mode === "local"
                ? "Local model server is stopped. Click Download & start in the Local Model tab."
                : "Active mode (" + formatModeName(mode) + ") has no API key saved in config.json. Configure it in Settings.";
            setBanner(homeStatusBox, "warn", "EMPTY", emptyText);
            if (settingsStatusBox.getAttribute("data-state") === "loading") {
                setBanner(settingsStatusBox, "warn", "EMPTY", emptyText);
            }
        }
    } catch (err) {
        statStatus.textContent = "Unreachable";
        setBanner(homeStatusBox, "err", "ERROR", "Could not reach local MyTranslation process: " + err.message);
        setBanner(settingsStatusBox, "err", "ERROR", "Could not reach local MyTranslation process: " + err.message);
    }
}

refreshBtn.addEventListener("click", () => {
    loadRuntimeState(true);
});

settingsForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    saveBtn.disabled = true;
    saveBtn.textContent = "Saving...";
    setBanner(settingsStatusBox, "loading", "LOADING", "Writing configuration to config.json...");

    const payload = Object.assign({
        translation_mode: activeMode,
        openai_base_url: openaiUrlInput.value.trim(),
        openai_model: openaiModelInput.value.trim(),
        deepseek_api_key: openaiKeyInput.value.trim(),
        claude_base_url: claudeUrlInput.value.trim(),
        claude_model: claudeModelInput.value.trim(),
        claude_api_key: claudeKeyInput.value.trim(),
        local_model: localModelSelect ? localModelSelect.value : savedLocalModel
    }, window.freeModeHandlers ? window.freeModeHandlers.buildPayload() : {});

    try {
        const res = await fetch("/api/settings", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload)
        });
        if (res.status === 401) {
            window.location.href = "/login";
            return;
        }
        const data = await res.json();
        if (data.success) {
            openaiKeyInput.value = "";
            claudeKeyInput.value = "";
            if (window.freeModeHandlers && window.freeModeHandlers.clearKeyInputs) {
                window.freeModeHandlers.clearKeyInputs();
            }
            await loadRuntimeState(false);
            setBanner(settingsStatusBox, "ok", "SAVED", data.message);
        } else {
            setBanner(settingsStatusBox, "err", "ERROR", data.message || "Failed to save configuration.");
        }
    } catch (err) {
        setBanner(settingsStatusBox, "err", "ERROR", "Request failed while saving settings: " + err.message);
    } finally {
        saveBtn.disabled = false;
        saveBtn.textContent = "Save settings";
    }
});

testBtn.addEventListener("click", async () => {
    testBtn.disabled = true;
    testBtn.textContent = "Verifying...";
    setBanner(settingsStatusBox, "loading", "LOADING", "Testing connection for " + formatModeName(activeMode) + "...");

    const payload = Object.assign({
        translation_mode: activeMode,
        openai_base_url: openaiUrlInput.value.trim(),
        openai_model: openaiModelInput.value.trim(),
        api_key: openaiKeyInput.value.trim(),
        claude_base_url: claudeUrlInput.value.trim(),
        claude_model: claudeModelInput.value.trim(),
        claude_api_key: claudeKeyInput.value.trim(),
        local_model: localModelSelect ? localModelSelect.value : savedLocalModel
    }, window.freeModeHandlers ? window.freeModeHandlers.buildPayload() : {});

    try {
        const res = await fetch("/api/test-key", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload)
        });
        const data = await res.json();
        if (data.success) {
            setBanner(settingsStatusBox, "ok", "VERIFIED", data.message);
        } else {
            setBanner(settingsStatusBox, "err", "ERROR", data.message || "Verification failed.");
        }
    } catch (err) {
        setBanner(settingsStatusBox, "err", "ERROR", "Network error during verification: " + err.message);
    } finally {
        testBtn.disabled = false;
        testBtn.textContent = "Verify connection";
    }
});

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
        setBanner(pgStatusBox, "loading", "READY", "Loaded " + file.name + " (" + Math.round(file.size / 1024) + " KB). Click Run translation.");
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

function escapeHtml(str) {
    return String(str).replace(/[&<>"']/g, m => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[m]);
}

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

loadRuntimeState(false);
