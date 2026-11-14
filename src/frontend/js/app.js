const navButtons = Array.from(document.querySelectorAll(".nav-btn"));
const viewPanels = {
    home: document.getElementById("panel-home"),
    playground: document.getElementById("panel-playground"),
    translation: document.getElementById("panel-translation"),
    settings: document.getElementById("panel-settings")
};

const generalTabBtn = document.getElementById("tab-general-btn");
const modeButtons = Array.from(document.querySelectorAll(".mode-btn[data-mode]"));
const modeSections = {
    general: document.getElementById("section-general"),
    openai: document.getElementById("section-openai"),
    claude: document.getElementById("section-claude"),
    local: document.getElementById("section-local")
};

let activeSettingsTab = "general";
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

const generalDisableBatchInput = document.getElementById("general-disable-batch");
const generalEnableQueueInput = document.getElementById("general-enable-queue");
const generalQueueMaxInput = document.getElementById("general-queue-max");
const generalEnableRateLimitInput = document.getElementById("general-enable-rate-limit");
const generalRateMaxInput = document.getElementById("general-rate-max");
const generalRateWindowInput = document.getElementById("general-rate-window");
const generalMaxImageMbInput = document.getElementById("general-max-image-mb");

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

function syncGeneralInputStates() {
    if (generalQueueMaxInput && generalEnableQueueInput) {
        generalQueueMaxInput.disabled = !generalEnableQueueInput.checked;
    }
    if (generalEnableRateLimitInput) {
        const rateDisabled = !generalEnableRateLimitInput.checked;
        if (generalRateMaxInput) generalRateMaxInput.disabled = rateDisabled;
        if (generalRateWindowInput) generalRateWindowInput.disabled = rateDisabled;
    }
}

if (generalEnableQueueInput) {
    generalEnableQueueInput.addEventListener("change", syncGeneralInputStates);
}
if (generalEnableRateLimitInput) {
    generalEnableRateLimitInput.addEventListener("change", syncGeneralInputStates);
}

function showSettingsTab(tab) {
    activeSettingsTab = tab;
    if (generalTabBtn) {
        const isGeneral = tab === "general";
        generalTabBtn.setAttribute("data-viewing", isGeneral ? "true" : "false");
        generalTabBtn.setAttribute("aria-selected", isGeneral ? "true" : "false");
    }
    modeButtons.forEach((btn) => {
        const isMatch = btn.getAttribute("data-mode") === tab;
        btn.setAttribute("data-viewing", isMatch ? "true" : "false");
    });
    Object.keys(modeSections).forEach((k) => {
        if (modeSections[k]) {
            modeSections[k].classList.toggle("active", k === tab);
        }
    });
    if (testBtn) {
        testBtn.hidden = tab === "general";
    }
    if (tab === "local" && typeof pollLocalStatus === "function") {
        pollLocalStatus();
    }
}

function selectMode(mode, openTab = true) {
    activeMode = mode;
    modeButtons.forEach((btn) => {
        const isMatch = btn.getAttribute("data-mode") === mode;
        btn.setAttribute("aria-checked", isMatch ? "true" : "false");
    });
    if (openTab) {
        showSettingsTab(mode);
    }
}

if (generalTabBtn) {
    generalTabBtn.addEventListener("click", () => {
        showSettingsTab("general");
    });
}

modeButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
        selectMode(btn.getAttribute("data-mode"), true);
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

function parseBoundedInt(inputEl, fallback, min, max) {
    if (!inputEl) return fallback;
    const n = parseInt(inputEl.value, 10);
    if (!Number.isFinite(n)) return fallback;
    return Math.min(max, Math.max(min, n));
}

function applyGeneralSettingsState(s) {
    if (generalDisableBatchInput) {
        generalDisableBatchInput.checked = Boolean(s.disable_batch_endpoint);
    }
    if (generalEnableQueueInput) {
        generalEnableQueueInput.checked = Boolean(s.enable_queue);
    }
    if (generalQueueMaxInput) {
        generalQueueMaxInput.value = String(s.queue_max_concurrent ?? 10);
    }
    if (generalEnableRateLimitInput) {
        generalEnableRateLimitInput.checked = Boolean(s.enable_rate_limit);
    }
    if (generalRateMaxInput) {
        generalRateMaxInput.value = String(s.rate_limit_max_images ?? 10);
    }
    if (generalRateWindowInput) {
        generalRateWindowInput.value = String(s.rate_limit_window_secs ?? 10);
    }
    if (generalMaxImageMbInput) {
        generalMaxImageMbInput.value = String(s.max_image_size_mb ?? 8);
    }
    syncGeneralInputStates();
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
        selectMode(mode, false);
        showSettingsTab(activeSettingsTab);
        statMode.textContent = formatModeName(mode);

        applyGeneralSettingsState(s);

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
        local_model: localModelSelect ? localModelSelect.value : savedLocalModel,
        disable_batch_endpoint: generalDisableBatchInput ? generalDisableBatchInput.checked : false,
        enable_queue: generalEnableQueueInput ? generalEnableQueueInput.checked : false,
        queue_max_concurrent: parseBoundedInt(generalQueueMaxInput, 10, 1, 100),
        enable_rate_limit: generalEnableRateLimitInput ? generalEnableRateLimitInput.checked : false,
        rate_limit_max_images: parseBoundedInt(generalRateMaxInput, 10, 1, 1000),
        rate_limit_window_secs: parseBoundedInt(generalRateWindowInput, 10, 1, 3600),
        max_image_size_mb: parseBoundedInt(generalMaxImageMbInput, 8, 1, 100)
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

showSettingsTab(activeSettingsTab);
loadRuntimeState(false);
