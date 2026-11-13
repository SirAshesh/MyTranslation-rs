(function () {
  if (window.MyTransCapture) {
    return;
  }

  let cursorX = 0;
  let cursorY = 0;
  let screenCaptureActive = false;

  document.addEventListener("mousemove", (event) => {
    cursorX = event.clientX;
    cursorY = event.clientY;
  });

  function isImgElement(el) {
    return Boolean(el && el.tagName && el.tagName.toLowerCase() === "img");
  }

  function inDisplay(img) {
    const rect = img.getBoundingClientRect();
    return rect.top <= 0 && rect.bottom >= 0;
  }

  function findImageInside(elements, checkInDisplay) {
    for (let i = 0; i < elements.length; i++) {
      const candidate = elements[i];
      if (isImgElement(candidate)) {
        if (!checkInDisplay || inDisplay(candidate)) {
          return candidate;
        }
      } else if (candidate.getElementsByTagName) {
        const nested = candidate.getElementsByTagName("img");
        if (nested.length > 0) {
          const found = findImageInside(nested, checkInDisplay);
          if (found) {
            return found;
          }
        }
      }
    }
    return null;
  }

  function getImageAtPoint(x, y, checkInDisplay) {
    const topEl = document.elementFromPoint(x, y);
    if (!topEl) {
      return null;
    }
    if (isImgElement(topEl)) {
      if (!checkInDisplay || inDisplay(topEl)) {
        return topEl;
      }
    }
    const stack = document.elementsFromPoint(x, y);
    return findImageInside(stack, checkInDisplay);
  }

  function getImageSrc(img) {
    if (!img) {
      return "";
    }
    if (img.hasAttribute("original-src")) {
      return img.getAttribute("original-src") || "";
    }
    return img.currentSrc || img.src || "";
  }

  function getImageBySrc(targetSrc, checkOriginal) {
    const imgs = document.getElementsByTagName("img");
    for (let i = 0; i < imgs.length; i++) {
      let current = imgs[i].currentSrc || imgs[i].src || "";
      if (checkOriginal && imgs[i].hasAttribute("original-src")) {
        current = imgs[i].getAttribute("original-src") || "";
      }
      if (current === targetSrc) {
        return imgs[i];
      }
    }
    return null;
  }

  function compressDataUrlToWebP(dataURL, quality) {
    const q = quality || 0.85;
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        try {
          const canvas = document.createElement("canvas");
          canvas.width = img.naturalWidth || img.width;
          canvas.height = img.naturalHeight || img.height;
          const ctx = canvas.getContext("2d");
          ctx.drawImage(img, 0, 0);
          resolve(canvas.toDataURL("image/webp", q));
        } catch (err) {
          reject(err);
        }
      };
      img.onerror = () => reject(new Error("Image decode error"));
      img.src = dataURL;
    });
  }

  function drawElementToDataUrl(img) {
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth || img.width;
    canvas.height = img.naturalHeight || img.height;
    if (!canvas.width || !canvas.height) {
      throw new Error("Image has zero dimensions");
    }
    const ctx = canvas.getContext("2d");
    ctx.drawImage(img, 0, 0);
    return canvas.toDataURL("image/webp", 0.85);
  }

  function downloadViaBackground(src) {
    return new Promise((resolve, reject) => {
      chrome.runtime.sendMessage({ action: "downloadImage", url: src }, (resp) => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
          return;
        }
        if (!resp || resp.error || !resp.dataURL) {
          reject(new Error((resp && resp.error) || "Background download failed"));
          return;
        }
        compressDataUrlToWebP(resp.dataURL, 0.85)
          .then(resolve)
          .catch(() => resolve(resp.dataURL));
      });
    });
  }

  function captureRectViaTabScreenshot(rect) {
    return new Promise((resolve, reject) => {
      const overlays = Array.from(document.querySelectorAll(".mytrans-overlay"));
      overlays.forEach((node) => {
        node.style.visibility = "hidden";
      });

      requestAnimationFrame(() => {
        chrome.runtime.sendMessage({ action: "captureVisibleTab" }, (resp) => {
          overlays.forEach((node) => {
            node.style.visibility = "";
          });
          if (chrome.runtime.lastError) {
            reject(new Error(chrome.runtime.lastError.message));
            return;
          }
          if (!resp || resp.error || !resp.dataURL) {
            reject(new Error((resp && resp.error) || "Tab screenshot capture failed"));
            return;
          }
          cropDataUrl(resp.dataURL, rect).then(resolve).catch(reject);
        });
      });
    });
  }

  function cropDataUrl(fullDataUrl, rect) {
    return new Promise((resolve, reject) => {
      const screenImg = new Image();
      screenImg.onload = () => {
        try {
          const dpr = window.devicePixelRatio || 1;
          const sx = Math.max(0, Math.round(rect.left * dpr));
          const sy = Math.max(0, Math.round(rect.top * dpr));
          const sw = Math.min(screenImg.width - sx, Math.round(rect.width * dpr));
          const sh = Math.min(screenImg.height - sy, Math.round(rect.height * dpr));
          if (sw <= 1 || sh <= 1) {
            reject(new Error("Selected region is outside visible viewport"));
            return;
          }
          const canvas = document.createElement("canvas");
          canvas.width = sw;
          canvas.height = sh;
          const ctx = canvas.getContext("2d");
          ctx.drawImage(screenImg, sx, sy, sw, sh, 0, 0, sw, sh);
          resolve(canvas.toDataURL("image/webp", 0.9));
        } catch (err) {
          reject(err);
        }
      };
      screenImg.onerror = () => reject(new Error("Screenshot decode failed"));
      screenImg.src = fullDataUrl;
    });
  }

  async function captureImageDataUrl(img, src, useCanvas) {
    if (!src) {
      throw new Error("No image source found");
    }
    if (src.startsWith("data:")) {
      return src;
    }

    if ((useCanvas || src.startsWith("blob:")) && img) {
      try {
        return drawElementToDataUrl(img);
      } catch (err) {
        console.debug("Direct canvas export unavailable, falling back:", err.message);
      }
    }

    try {
      return await downloadViaBackground(src);
    } catch (bgErr) {
      if (img) {
        const rect = img.getBoundingClientRect();
        if (rect.width > 10 && rect.height > 10) {
          return captureRectViaTabScreenshot(rect);
        }
      }
      throw bgErr;
    }
  }

  function startScreenCapture(onRegionCaptured) {
    if (screenCaptureActive) {
      return;
    }
    screenCaptureActive = true;

    const overlay = document.createElement("div");
    overlay.id = "mytrans-sc-overlay";
    const selection = document.createElement("div");
    selection.id = "mytrans-sc-selection";
    document.body.appendChild(selection);
    document.body.appendChild(overlay);

    let startX = 0;
    let startY = 0;
    let dragging = false;

    function cleanup() {
      screenCaptureActive = false;
      overlay.remove();
      selection.remove();
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("keydown", onKeyDown);
    }

    function onKeyDown(event) {
      if (event.key === "Escape") {
        cleanup();
      }
    }

    function onMove(event) {
      if (!dragging) {
        return;
      }
      const x = Math.min(startX, event.clientX);
      const y = Math.min(startY, event.clientY);
      const w = Math.abs(event.clientX - startX);
      const h = Math.abs(event.clientY - startY);
      selection.style.left = x + "px";
      selection.style.top = y + "px";
      selection.style.width = w + "px";
      selection.style.height = h + "px";
    }

    function onUp(event) {
      if (!dragging) {
        return;
      }
      dragging = false;
      const rect = {
        left: Math.min(startX, event.clientX),
        top: Math.min(startY, event.clientY),
        width: Math.abs(event.clientX - startX),
        height: Math.abs(event.clientY - startY)
      };
      cleanup();
      if (rect.width < 12 || rect.height < 12) {
        return;
      }
      captureRectViaTabScreenshot(rect)
        .then((cropped) => onRegionCaptured(cropped, rect))
        .catch((err) => onRegionCaptured(null, rect, err));
    }

    overlay.addEventListener("mousedown", (event) => {
      dragging = true;
      startX = event.clientX;
      startY = event.clientY;
      selection.style.display = "block";
      selection.style.left = startX + "px";
      selection.style.top = startY + "px";
      selection.style.width = "0px";
      selection.style.height = "0px";
      event.preventDefault();
      event.stopPropagation();
    });

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("keydown", onKeyDown);
  }

  window.MyTransCapture = {
    getLastMousePosition: () => ({ x: cursorX, y: cursorY }),
    getImageAtPoint,
    getImageSrc,
    getImageBySrc,
    captureImageDataUrl,
    startScreenCapture
  };
})();

