/**
 * Overlay module — handles the fullscreen snip interaction.
 *
 * Flow:
 * 1. Fetches screenshot path from Rust via get_capture_info command.
 * 2. Draws it on a canvas with a 50% dark overlay.
 * 3. User drags a rectangle to select a region.
 * 4. On mouseup, sends the rectangle coordinates to Rust via process_snip.
 * 5. Rust crops → OCR → LLM → opens action menu.
 */

import { invoke, convertFileSrc } from "@tauri-apps/api/core";

interface SelectionRect {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

interface CaptureInfo {
  image_path: string;
  click_epoch_ms: number;
}

export function setupOverlay(): void {
  const canvas = document.getElementById("overlay-canvas") as HTMLCanvasElement;
  if (!canvas) return;

  const ctx = canvas.getContext("2d")!;
  const dpr = window.devicePixelRatio || 1;
  let screenshotImage: HTMLImageElement | null = null;
  let selection: SelectionRect | null = null;
  let isDragging = false;

  // Resize canvas to fill the screen at physical pixel resolution.
  // All drawing uses CSS coordinates thanks to ctx.scale(dpr, dpr).
  function resizeCanvas(): void {
    const cssW = window.innerWidth;
    const cssH = window.innerHeight;
    canvas.width = cssW * dpr;
    canvas.height = cssH * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (screenshotImage) {
      drawOverlay();
    }
  }

  // Draw the screenshot with dark overlay and selection rectangle.
  // All coordinates are in CSS pixels (ctx is scaled by dpr).
  function drawOverlay(): void {
    if (!screenshotImage) return;

    const cssW = window.innerWidth;
    const cssH = window.innerHeight;

    // Draw the screenshot scaled to fill the CSS viewport
    ctx.drawImage(screenshotImage, 0, 0, cssW, cssH);

    // Dark overlay (50% opacity)
    ctx.fillStyle = "rgba(0, 0, 0, 0.5)";
    ctx.fillRect(0, 0, cssW, cssH);

    // If there's an active selection, cut through the overlay
    if (selection) {
      const x = Math.min(selection.startX, selection.endX);
      const y = Math.min(selection.startY, selection.endY);
      const w = Math.abs(selection.endX - selection.startX);
      const h = Math.abs(selection.endY - selection.startY);

      if (w > 0 && h > 0) {
        // Clear the dark overlay in the selected region
        ctx.clearRect(x, y, w, h);
        // Redraw the screenshot in the selected region (no dimming).
        // Source coords must be in the image's pixel space.
        const imgScaleX = screenshotImage.width / cssW;
        const imgScaleY = screenshotImage.height / cssH;
        ctx.drawImage(
          screenshotImage,
          x * imgScaleX, y * imgScaleY, w * imgScaleX, h * imgScaleY,
          x, y, w, h
        );

        // Selection border
        ctx.strokeStyle = "#00b4ff";
        ctx.lineWidth = 2;
        ctx.strokeRect(x, y, w, h);

        // Dimension label
        ctx.fillStyle = "#00b4ff";
        ctx.font = "12px monospace";
        ctx.fillText(`${Math.round(w)} × ${Math.round(h)}`, x, y - 6);
      }
    }
  }

  // Mouse event handlers (clientX/clientY are in CSS pixels)
  canvas.addEventListener("mousedown", (e: MouseEvent) => {
    isDragging = true;
    selection = {
      startX: e.clientX,
      startY: e.clientY,
      endX: e.clientX,
      endY: e.clientY,
    };
  });

  canvas.addEventListener("mousemove", (e: MouseEvent) => {
    if (!isDragging || !selection) return;
    selection.endX = e.clientX;
    selection.endY = e.clientY;
    drawOverlay();
  });

  canvas.addEventListener("mouseup", async (e: MouseEvent) => {
    if (!isDragging || !selection) return;
    isDragging = false;
    selection.endX = e.clientX;
    selection.endY = e.clientY;

    const x = Math.min(selection.startX, selection.endX);
    const y = Math.min(selection.startY, selection.endY);
    const w = Math.abs(selection.endX - selection.startX);
    const h = Math.abs(selection.endY - selection.startY);

    // Ignore tiny selections (accidental clicks)
    if (w < 10 || h < 10) {
      // Close overlay on click without drag (escape hatch)
      await invoke("close_overlay");
      return;
    }

    console.log(`Selection: ${w}×${h} at (${x}, ${y})`);

    try {
      // Map CSS pixel coordinates to screenshot pixel coordinates.
      // Use actual image dimensions — devicePixelRatio doesn't match the
      // screenshot resolution on macOS scaled displays (e.g. "Looks like
      // 1440x900" on a 2560x1600 panel gives dpr=2 but xcap captures at
      // 2560x1600, not 2880x1800).
      const cssW = window.innerWidth;
      const cssH = window.innerHeight;
      const imgW = screenshotImage?.width || cssW;
      const imgH = screenshotImage?.height || cssH;
      const scaleX = imgW / cssW;
      const scaleY = imgH / cssH;
      console.log(`[PIPELINE] Starting process_snip... scale=${scaleX.toFixed(2)}x${scaleY.toFixed(2)} img=${imgW}x${imgH} css=${cssW}x${cssH}`);
      await invoke("process_snip", {
        x: Math.round(x * scaleX),
        y: Math.round(y * scaleY),
        width: Math.round(w * scaleX),
        height: Math.round(h * scaleY),
        menuX: x,       // CSS pixels for action menu window position
        menuY: y + h,    // Bottom edge of bounding box
      });
      // Overlay is closed by Rust after pipeline completes
    } catch (err) {
      console.error("Pipeline failed:", err);
      try { await invoke("close_overlay"); } catch { /* window may be gone */ }
    }
  });

  // Escape key closes the overlay
  document.addEventListener("keydown", async (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      await invoke("close_overlay");
    }
  });

  // Fetch screenshot info from Rust via command (not event).
  // Commands only execute after JS is fully loaded — no race condition.
  loadScreenshot();

  async function loadScreenshot(): Promise<void> {
    try {
      const fetchStartMs = Date.now();
      const info = await invoke<CaptureInfo>("get_capture_info");
      const clickEpochMs = info.click_epoch_ms;
      const commandMs = Date.now() - fetchStartMs;
      console.log(`[LATENCY] get_capture_info: ${commandMs}ms`);

      // Convert file path to asset URL that the webview can load
      const assetUrl = convertFileSrc(info.image_path);
      console.log(`[LATENCY] loading screenshot from: ${assetUrl}`);

      const img = new Image();
      img.onload = () => {
        screenshotImage = img;
        resizeCanvas();
        const overlayVisibleMs = Date.now();
        const clickToVisibleMs = overlayVisibleMs - clickEpochMs;
        console.log(
          `[LATENCY] overlay_visible: click-to-visible=${clickToVisibleMs.toFixed(1)}ms`
        );
        console.log(`Screenshot loaded: ${img.width}×${img.height}, dpr=${dpr}`);
      };
      img.onerror = (e) => {
        console.error(`Failed to load screenshot from: ${assetUrl}`, e);
        const errDiv = document.createElement("div");
        errDiv.style.cssText = "position:fixed;top:20px;left:20px;color:red;font:16px monospace;z-index:9999;background:rgba(0,0,0,0.8);padding:12px;border-radius:4px;max-width:80vw;word-break:break-all";
        errDiv.textContent = `IMG LOAD FAILED: ${assetUrl}`;
        document.body.appendChild(errDiv);
      };
      img.src = assetUrl;
    } catch (err) {
      console.error("Failed to get capture info:", err);
      const errDiv = document.createElement("div");
      errDiv.style.cssText = "position:fixed;top:20px;left:20px;color:red;font:16px monospace;z-index:9999;background:rgba(0,0,0,0.8);padding:12px;border-radius:4px";
      errDiv.textContent = `get_capture_info FAILED: ${err}`;
      document.body.appendChild(errDiv);
    }
  }

  window.addEventListener("resize", resizeCanvas);
  resizeCanvas();
}
