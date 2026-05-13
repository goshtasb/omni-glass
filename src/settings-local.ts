/**
 * Local model management UI — rendered inside the Settings panel.
 *
 * Shows available models, download status, progress bars, and delete buttons.
 * Listens for `model-download-progress` Tauri events to update download UI.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface LocalModelInfo {
  id: string;
  name: string;
  sizeBytes: number;
  ramRequiredGb: number;
  description: string;
  downloaded: boolean;
}

interface LocalModelsResponse {
  featureEnabled: boolean;
  models: LocalModelInfo[];
}

interface DownloadProgress {
  modelId: string;
  downloaded: number;
  total: number;
  percent: number;
}

function formatSize(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`;
  return `${bytes} bytes`;
}

/** Render the local models section HTML (for injection into the Settings panel). */
export async function renderLocalModelsSection(): Promise<string> {
  let data: LocalModelsResponse;
  try {
    data = await invoke<LocalModelsResponse>("get_local_models");
  } catch {
    return "";
  }

  if (!data.featureEnabled) return "";

  const modelCards = data.models
    .map((m) => {
      const statusBadge = m.downloaded
        ? `<span style="font-size:11px;background:#22c55e;color:#000;padding:2px 8px;border-radius:10px;">Downloaded</span>`
        : `<span style="font-size:11px;background:rgba(255,255,255,0.15);padding:2px 8px;border-radius:10px;">Not downloaded</span>`;

      const actionBtn = m.downloaded
        ? `<button class="local-delete-btn" data-model="${m.id}" style="
            padding:5px 10px;background:#7f1d1d;border:1px solid #dc2626;
            border-radius:4px;color:#fca5a5;font-size:12px;cursor:pointer;">Delete</button>`
        : `<button class="local-download-btn" data-model="${m.id}" style="
            padding:5px 10px;background:#1e3a5f;border:1px solid #3b82f6;
            border-radius:4px;color:#93c5fd;font-size:12px;cursor:pointer;">Download</button>`;

      return `
      <div class="local-model-card" data-model="${m.id}" style="
        background:#0f1629;border:1px solid rgba(255,255,255,0.1);
        border-radius:8px;padding:12px;margin-bottom:8px;">
        <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:6px;">
          <span style="font-weight:500;font-size:13px;">${m.name}</span>
          ${statusBadge}
        </div>
        <div style="font-size:12px;color:rgba(255,255,255,0.5);margin-bottom:8px;">
          ${formatSize(m.sizeBytes)} &middot; ${m.ramRequiredGb} GB RAM required
        </div>
        <div style="font-size:12px;color:rgba(255,255,255,0.5);margin-bottom:8px;">
          ${m.description}
        </div>
        <div style="display:flex;align-items:center;gap:8px;">
          ${actionBtn}
          <div class="download-progress" data-model="${m.id}" style="
            flex:1;display:none;align-items:center;gap:8px;">
            <div style="flex:1;height:6px;background:rgba(255,255,255,0.1);border-radius:3px;overflow:hidden;">
              <div class="progress-bar" data-model="${m.id}" style="
                height:100%;background:#3b82f6;border-radius:3px;width:0%;transition:width 0.3s;">
              </div>
            </div>
            <span class="progress-text" data-model="${m.id}" style="font-size:11px;color:rgba(255,255,255,0.6);min-width:40px;">0%</span>
          </div>
        </div>
      </div>`;
    })
    .join("");

  return `
    <section style="margin-bottom:24px;">
      <h2 style="font-size:14px;font-weight:500;color:rgba(255,255,255,0.5);
                  text-transform:uppercase;letter-spacing:0.05em;margin-bottom:12px;">
        Local Models
      </h2>
      ${modelCards}
      <div style="font-size:11px;color:rgba(255,255,255,0.4);padding-top:4px;">
        Local models run on-device — no internet or API key required.
        3-5x slower than cloud providers. Best for privacy and offline use.
      </div>
    </section>`;
}

/** Attach event handlers for local model buttons + download progress events. */
export function attachLocalModelHandlers(reloadSettings: () => Promise<void>): void {
  // Download buttons
  document.querySelectorAll(".local-download-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const modelId = (btn as HTMLElement).dataset.model!;
      const progressEl = document.querySelector(
        `.download-progress[data-model="${modelId}"]`
      ) as HTMLElement;
      if (progressEl) progressEl.style.display = "flex";
      (btn as HTMLButtonElement).disabled = true;
      (btn as HTMLButtonElement).textContent = "Downloading...";

      try {
        await invoke("download_local_model", { modelId });
        await reloadSettings();
      } catch (e) {
        console.error("Download failed:", e);
        (btn as HTMLButtonElement).disabled = false;
        (btn as HTMLButtonElement).textContent = "Retry";
        if (progressEl) progressEl.style.display = "none";
      }
    });
  });

  // Delete buttons
  document.querySelectorAll(".local-delete-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const modelId = (btn as HTMLElement).dataset.model!;
      try {
        await invoke("delete_local_model", { modelId });
        await reloadSettings();
      } catch (e) {
        console.error("Delete failed:", e);
      }
    });
  });

  // Listen for download progress events
  listen<DownloadProgress>("model-download-progress", (event) => {
    const { modelId, percent } = event.payload;
    const bar = document.querySelector(
      `.progress-bar[data-model="${modelId}"]`
    ) as HTMLElement;
    const text = document.querySelector(
      `.progress-text[data-model="${modelId}"]`
    ) as HTMLElement;
    if (bar) bar.style.width = `${percent}%`;
    if (text) text.textContent = `${percent}%`;
  });
}
