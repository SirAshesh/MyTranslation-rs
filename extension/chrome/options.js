const DEFAULT_SETTINGS = {
  serverURL: "http://127.0.0.1:5000",
  sourceLang: "auto",
  targetLang: "id",
  pickingWay: "1",
  useCanvas: true,
  sendRequestsViaBackground: true,
  showFloatingButton: false
};

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

function fillSelect(selectEl, items, selected) {
  selectEl.innerHTML = "";
  for (const item of items) {
    const opt = document.createElement("option");
    opt.value = item.code;
    opt.textContent = item.name;
    selectEl.appendChild(opt);
  }
  selectEl.value = selected;
}

function applyToForm(items) {
  document.getElementById("server-url").value = items.serverURL || DEFAULT_SETTINGS.serverURL;
  document.getElementById("send-bg").checked = items.sendRequestsViaBackground !== false;
  fillSelect(document.getElementById("source-lang"), LANGUAGES, items.sourceLang || "auto");
  fillSelect(
    document.getElementById("target-lang"),
    LANGUAGES.filter((l) => l.code !== "auto"),
    items.targetLang || "id"
  );
  document.getElementById("picking-way").value = items.pickingWay || "1";
  document.getElementById("use-canvas").checked = items.useCanvas !== false;
  document.getElementById("show-floating-btn").checked = Boolean(items.showFloatingButton);
}

function readFromForm() {
  const rawUrl = document.getElementById("server-url").value.trim() || DEFAULT_SETTINGS.serverURL;
  return {
    serverURL: rawUrl.replace(/\/+$/, ""),
    sendRequestsViaBackground: document.getElementById("send-bg").checked,
    sourceLang: document.getElementById("source-lang").value,
    targetLang: document.getElementById("target-lang").value,
    pickingWay: document.getElementById("picking-way").value,
    useCanvas: document.getElementById("use-canvas").checked,
    showFloatingButton: document.getElementById("show-floating-btn").checked
  };
}

function testServerConnection() {
  const resultEl = document.getElementById("conn-result");
  const baseUrl = (document.getElementById("server-url").value.trim() || DEFAULT_SETTINGS.serverURL).replace(/\/+$/, "");
  resultEl.className = "hint-status";
  resultEl.textContent = "Testing connection...";

  chrome.runtime.sendMessage(
    { action: "proxyFetch", url: baseUrl + "/list", options: { method: "GET" } },
    (resp) => {
      if (chrome.runtime.lastError || !resp || !resp.ok) {
        resultEl.className = "hint-status is-err";
        resultEl.textContent = "Connection failed: " + ((resp && resp.error) || "Server unreachable");
        return;
      }
      const list = Array.isArray(resp.data) ? resp.data : [];
      const names = list.map((item) => item.name).join(", ") || "MyTranslation";
      resultEl.className = "hint-status is-ok";
      resultEl.textContent = "Connected (" + names + ")";
    }
  );
}

document.addEventListener("DOMContentLoaded", () => {
  chrome.storage.sync.get(DEFAULT_SETTINGS, applyToForm);

  document.getElementById("test-conn").addEventListener("click", testServerConnection);

  document.getElementById("save-btn").addEventListener("click", () => {
    const updated = readFromForm();
    chrome.storage.sync.set(updated, () => {
      const statusEl = document.getElementById("save-status");
      statusEl.hidden = false;
      setTimeout(() => {
        statusEl.hidden = true;
      }, 2000);
    });
  });

  document.getElementById("reset-btn").addEventListener("click", () => {
    chrome.storage.sync.set(DEFAULT_SETTINGS, () => {
      applyToForm(DEFAULT_SETTINGS);
      const statusEl = document.getElementById("save-status");
      statusEl.hidden = false;
      setTimeout(() => {
        statusEl.hidden = true;
      }, 2000);
    });
  });
});

