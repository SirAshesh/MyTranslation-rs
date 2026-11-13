const DEFAULT_SETTINGS = {
  serverURL: "http://127.0.0.1:5000",
  sourceLang: "auto",
  targetLang: "id",
  pickingWay: "1",
  useCanvas: true,
  sendRequestsViaBackground: true,
  showFloatingButton: false,
  floatingBtnAction: "translate"
};

function createContextMenus() {
  chrome.contextMenus.removeAll(() => {
    if (chrome.runtime.lastError) {
      console.warn("Context menu reset warning:", chrome.runtime.lastError.message);
    }
    const parentTitle = chrome.i18n.getMessage("ctxmenu_parent") || "MyTranslation";
    const translateTitle = chrome.i18n.getMessage("ctxmenu_translate") || "Translate Image";
    const alterTitle = chrome.i18n.getMessage("ctxmenu_alter") || "Toggle Original / Translated";

    chrome.contextMenus.create({
      id: "mytranslation-parent",
      title: parentTitle,
      contexts: ["image"]
    });
    chrome.contextMenus.create({
      id: "translate",
      parentId: "mytranslation-parent",
      title: translateTitle,
      contexts: ["image"]
    });
    chrome.contextMenus.create({
      id: "alter",
      parentId: "mytranslation-parent",
      title: alterTitle,
      contexts: ["image"]
    });
  });
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.sync.get(DEFAULT_SETTINGS, (existing) => {
    chrome.storage.sync.set({ ...DEFAULT_SETTINGS, ...existing });
  });
  createContextMenus();
});

async function ensureContentScripts(tabId) {
  const isReady = await new Promise((resolve) => {
    chrome.tabs.sendMessage(tabId, { message: "ping" }, (resp) => {
      if (chrome.runtime.lastError || !resp || !resp.ok) {
        resolve(false);
        return;
      }
      resolve(true);
    });
  });
  if (isReady) {
    return;
  }

  await chrome.scripting.insertCSS({
    target: { tabId },
    files: ["content/content.css"]
  });
  await chrome.scripting.executeScript({
    target: { tabId },
    files: ["content/ui.js", "content/capture.js", "content/content.js"]
  });
}

async function sendTabCommand(tabId, payload) {
  await ensureContentScripts(tabId);
  return new Promise((resolve) => {
    chrome.tabs.sendMessage(tabId, payload, (response) => {
      if (chrome.runtime.lastError) {
        resolve({ error: chrome.runtime.lastError.message });
        return;
      }
      resolve(response || { ok: true });
    });
  });
}

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (!tab || typeof tab.id !== "number") {
    return;
  }
  const command = info.menuItemId + "WithMenu";
  sendTabCommand(tab.id, { message: command, info }).catch((err) => {
    console.error("Context menu dispatch failed:", err);
  });
});

chrome.commands.onCommand.addListener((command) => {
  if (command !== "screen-capture-ocr") {
    return;
  }
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    if (tabs && tabs.length > 0 && typeof tabs[0].id === "number") {
      sendTabCommand(tabs[0].id, { message: "startScreenCapture" }).catch((err) => {
        console.error("Shortcut dispatch failed:", err);
      });
    }
  });
});

function arrayBufferToBase64(buffer) {
  const bytes = new Uint8Array(buffer);
  const chunkSize = 0x8000;
  const parts = [];
  for (let i = 0; i < bytes.length; i += chunkSize) {
    parts.push(String.fromCharCode.apply(null, bytes.subarray(i, i + chunkSize)));
  }
  return btoa(parts.join(""));
}

async function handleProxyFetch(request) {
  const resp = await fetch(request.url, request.options || {});
  const contentType = resp.headers.get("content-type") || "";
  const data = contentType.includes("application/json")
    ? await resp.json()
    : await resp.text();
  return { ok: resp.ok, status: resp.status, data };
}

async function handleDownloadImage(url) {
  const resp = await fetch(url);
  if (!resp.ok) {
    throw new Error("Image fetch failed with status " + resp.status);
  }
  const blob = await resp.blob();
  const mime = blob.type && blob.type.startsWith("image/") ? blob.type : "image/jpeg";
  const base64 = arrayBufferToBase64(await blob.arrayBuffer());
  return { dataURL: "data:" + mime + ";base64," + base64 };
}

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (!request) {
    return false;
  }

  if (request.action === "proxyFetch") {
    handleProxyFetch(request)
      .then((res) => sendResponse(res))
      .catch((err) => sendResponse({ ok: false, status: 0, data: null, error: err.message }));
    return true;
  }

  if (request.action === "downloadImage") {
    handleDownloadImage(request.url)
      .then((res) => sendResponse(res))
      .catch((err) => sendResponse({ error: err.message }));
    return true;
  }

  if (request.action === "captureVisibleTab") {
    const windowId = sender && sender.tab ? sender.tab.windowId : null;
    chrome.tabs.captureVisibleTab(windowId, { format: "png" }, (dataURL) => {
      if (chrome.runtime.lastError) {
        sendResponse({ error: chrome.runtime.lastError.message });
        return;
      }
      sendResponse({ dataURL });
    });
    return true;
  }

  if (request.action === "ensureContentScripts" && typeof request.tabId === "number") {
    ensureContentScripts(request.tabId)
      .then(() => sendResponse({ ok: true }))
      .catch((err) => sendResponse({ ok: false, error: err.message }));
    return true;
  }

  if (request.action === "openPopup") {
    if (chrome.action && typeof chrome.action.openPopup === "function") {
      try {
        const result = chrome.action.openPopup();
        if (result && typeof result.catch === "function") {
          result.catch(() => {});
        }
      } catch (err) {
        console.warn("openPopup unavailable:", err);
      }
    }
    sendResponse({ ok: true });
    return false;
  }

  if (request === "showOptions" || request.action === "showOptions") {
    if (typeof chrome.runtime.openOptionsPage === "function") {
      chrome.runtime.openOptionsPage();
    } else {
      chrome.tabs.create({ url: chrome.runtime.getURL("options.html") });
    }
    sendResponse({ ok: true });
    return false;
  }

  return false;
});
