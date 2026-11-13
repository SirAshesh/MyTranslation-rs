const LANGUAGES = [
  { code: "auto", name: "Auto Detect" },
  { code: "id", name: "Indonesian" },
  { code: "en", name: "English" },
  { code: "ja", name: "Japanese" },
  { code: "zh", name: "Chinese" },
  { code: "ko", name: "Korean" },
  { code: "vi", name: "Vietnamese" },
  { code: "th", name: "Thai" },
  { code: "ms", name: "Malay" },
  { code: "es", name: "Spanish" },
  { code: "fr", name: "French" },
  { code: "de", name: "German" },
  { code: "ru", name: "Russian" },
  { code: "ar", name: "Arabic" }
];

let currentServerURL = "http://127.0.0.1:5000";

function populateSelect(selectEl, options, selectedValue) {
  selectEl.innerHTML = "";
  for (const item of options) {
    const opt = document.createElement("option");
    opt.value = item.code;
    opt.textContent = item.name;
    selectEl.appendChild(opt);
  }
  selectEl.value = selectedValue;
}

function showPopupError(text) {
  const errEl = document.getElementById("popup-error");
  errEl.textContent = text;
  errEl.hidden = false;
}

function checkServerHealth(serverUrl) {
  const badge = document.getElementById("server-status");
  const base = String(serverUrl || "http://127.0.0.1:5000").trim().replace(/\/+$/, "");
  chrome.runtime.sendMessage(
    { action: "proxyFetch", url: base + "/list", options: { method: "GET" } },
    (resp) => {
      if (chrome.runtime.lastError || !resp || !resp.ok) {
        badge.textContent = "Offline";
        badge.className = "status-pill status-offline";
        return;
      }
      badge.textContent = "Online";
      badge.className = "status-pill status-online";
    }
  );
}

function getActiveTab() {
  return new Promise((resolve) => {
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      resolve(tabs && tabs.length > 0 ? tabs[0] : null);
    });
  });
}

function ensureTabReady(tabId) {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ action: "ensureContentScripts", tabId }, (resp) => {
      if (chrome.runtime.lastError || !resp || !resp.ok) {
        resolve(false);
        return;
      }
      resolve(true);
    });
  });
}

async function sendToActiveTab(payload, closeAfter) {
  const tab = await getActiveTab();
  if (!tab || typeof tab.id !== "number") {
    showPopupError("No active tab found");
    return null;
  }
  const ready = await ensureTabReady(tab.id);
  if (!ready) {
    showPopupError("Cannot access this page (restricted browser page)");
    return null;
  }

  return new Promise((resolve) => {
    chrome.tabs.sendMessage(tab.id, payload, (response) => {
      if (chrome.runtime.lastError) {
        showPopupError(chrome.runtime.lastError.message);
        resolve(null);
        return;
      }
      if (closeAfter) {
        window.close();
      }
      resolve(response || null);
    });
  });
}

function updateAutoButton(btn, isActive) {
  btn.textContent = isActive ? "Stop Auto Translating" : "Start Auto Translating";
  btn.classList.toggle("is-active", Boolean(isActive));
}

document.addEventListener("DOMContentLoaded", () => {
  const sourceSelect = document.getElementById("source-lang");
  const targetSelect = document.getElementById("target-lang");
  const autoBtn = document.getElementById("auto-translate");

  chrome.storage.sync.get(
    {
      serverURL: "http://127.0.0.1:5000",
      sourceLang: "auto",
      targetLang: "id"
    },
    (items) => {
      currentServerURL = items.serverURL || "http://127.0.0.1:5000";
      populateSelect(sourceSelect, LANGUAGES, items.sourceLang || "auto");
      populateSelect(
        targetSelect,
        LANGUAGES.filter((l) => l.code !== "auto"),
        items.targetLang || "id"
      );
      checkServerHealth(currentServerURL);
    }
  );

  sourceSelect.addEventListener("change", () => {
    chrome.storage.sync.set({ sourceLang: sourceSelect.value });
  });
  targetSelect.addEventListener("change", () => {
    chrome.storage.sync.set({ targetLang: targetSelect.value });
  });

  document.getElementById("translate").addEventListener("click", () => {
    sendToActiveTab({ message: "translate", check: false }, true);
  });
  document.getElementById("alterlanguage").addEventListener("click", () => {
    sendToActiveTab({ message: "alterlanguage", check: false }, true);
  });
  document.getElementById("translate-check").addEventListener("click", () => {
    sendToActiveTab({ message: "translate", check: true }, true);
  });
  document.getElementById("alterlanguage-check").addEventListener("click", () => {
    sendToActiveTab({ message: "alterlanguage", check: true }, true);
  });
  document.getElementById("getimgsrc").addEventListener("click", () => {
    sendToActiveTab({ message: "getsrconly", check: false }, true);
  });
  document.getElementById("getimgsrc-check").addEventListener("click", () => {
    sendToActiveTab({ message: "getsrconly", check: true }, true);
  });
  autoBtn.addEventListener("click", async () => {
    const resp = await sendToActiveTab({ message: "toggleAutoTranslate" }, true);
    if (resp) {
      updateAutoButton(autoBtn, resp.active);
    }
  });
  document.getElementById("screen-capture").addEventListener("click", () => {
    sendToActiveTab({ message: "startScreenCapture" }, true);
  });

  document.getElementById("open-options").addEventListener("click", (event) => {
    event.preventDefault();
    if (typeof chrome.runtime.openOptionsPage === "function") {
      chrome.runtime.openOptionsPage();
    } else {
      chrome.tabs.create({ url: chrome.runtime.getURL("options.html") });
    }
    window.close();
  });

  document.getElementById("open-dashboard").addEventListener("click", (event) => {
    event.preventDefault();
    const base = String(currentServerURL || "http://127.0.0.1:5000").trim().replace(/\/+$/, "");
    chrome.tabs.create({ url: base + "/" });
    window.close();
  });

  sendToActiveTab({ message: "getAutoTranslateState" }, false).then((resp) => {
    if (resp) {
      updateAutoButton(autoBtn, resp.active);
    }
  });
});

