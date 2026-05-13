/**
 * Settings panel — provider configuration and API key management.
 *
 * Sections:
 *   1. AI Provider — dropdown, API key inputs, Test buttons
 *   2. Recognition — OCR mode toggle (fast/accurate)
 *   3. About — version info
 *
 * API keys are stored in the OS keychain via Rust (keyring crate).
 * Falls back to environment variables for development.
 */

import { invoke } from "@tauri-apps/api/core";
import { renderLocalModelsSection, attachLocalModelHandlers } from "./settings-local";

interface ProviderInfo {
  id: string;
  name: string;
  envKey: string;
  costPerSnip: string;
  speedStars: number;
  qualityStars: number;
}

interface ProviderConfig {
  activeProvider: string;
  providers: ProviderInfo[];
  configuredProviders: string[];
}

function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function stars(count: number): string {
  return "\u2605".repeat(count) + "\u2606".repeat(5 - count);
}

async function loadSettings(): Promise<void> {
  const container = document.getElementById("settings")!;

  let config: ProviderConfig;
  try {
    config = await invoke<ProviderConfig>("get_provider_config");
  } catch (e) {
    container.innerHTML = `<div style="padding:20px;color:#f87171;">Failed to load settings: ${e}</div>`;
    return;
  }

  container.innerHTML = `
    <div style="padding: 20px; max-width: 480px; margin: 0 auto;">

      <h1 style="font-size: 18px; font-weight: 600; margin-bottom: 20px; color: #e2e8f0;">
        Settings
      </h1>

      <!-- AI Provider Section -->
      <section style="margin-bottom: 24px;">
        <h2 style="font-size: 14px; font-weight: 500; color: rgba(255,255,255,0.5);
                    text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 12px;">
          AI Provider
        </h2>

        <div style="margin-bottom: 16px;">
          <label style="font-size: 13px; color: rgba(255,255,255,0.7); display: block; margin-bottom: 6px;">
            Active Provider
          </label>
          <select id="provider-select" style="
            width: 100%;
            padding: 8px 12px;
            background: #16213e;
            border: 1px solid rgba(255,255,255,0.15);
            border-radius: 6px;
            color: #fff;
            font-size: 14px;
            outline: none;
            cursor: pointer;
          ">
            ${config.providers
              .map(
                (p) =>
                  `<option value="${p.id}" ${p.id === config.activeProvider ? "selected" : ""}>
                    ${escapeHtml(p.name)}${p.id === config.activeProvider ? " (Active)" : ""}
                  </option>`
              )
              .join("")}
          </select>
        </div>

        <div id="provider-cards">
          ${config.providers.map((p) => renderProviderCard(p, config)).join("")}
        </div>
      </section>

      <!-- Local Models Section (injected dynamically) -->
      <div id="local-models-section"></div>

      <!-- Recognition Mode Section -->
      <section style="margin-bottom: 24px;">
        <h2 style="font-size: 14px; font-weight: 500; color: rgba(255,255,255,0.5);
                    text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 12px;">
          Recognition
        </h2>

        <div style="
          background: #0f1629;
          border: 1px solid rgba(255,255,255,0.1);
          border-radius: 8px;
          padding: 14px;
        ">
          <div style="margin-bottom: 12px;">
            <label style="display: flex; align-items: center; gap: 8px; cursor: pointer; margin-bottom: 8px;">
              <input type="radio" name="ocr-mode" value="fast" id="ocr-fast" style="accent-color: #3b82f6;" />
              <span style="font-size: 14px;">Fast <span style="color: rgba(255,255,255,0.5); font-size: 12px;">(default)</span></span>
            </label>
            <div style="margin-left: 24px; font-size: 12px; color: rgba(255,255,255,0.5); margin-bottom: 10px;">
              ~26ms on macOS. Best for action classification.
            </div>

            <label style="display: flex; align-items: center; gap: 8px; cursor: pointer;">
              <input type="radio" name="ocr-mode" value="accurate" id="ocr-accurate" style="accent-color: #3b82f6;" />
              <span style="font-size: 14px;">Accurate</span>
            </label>
            <div style="margin-left: 24px; font-size: 12px; color: rgba(255,255,255,0.5);">
              ~98ms on macOS. Full text fidelity for exports.
            </div>
          </div>

          <div style="font-size: 11px; color: rgba(255,255,255,0.4); border-top: 1px solid rgba(255,255,255,0.08); padding-top: 10px;">
            Note: "Accurate" mode is used automatically for text-sensitive actions
            (Translate, Export CSV) regardless of this setting.
          </div>
        </div>
      </section>

      <!-- About Section -->
      <section style="
        border-top: 1px solid rgba(255,255,255,0.1);
        padding-top: 16px;
        color: rgba(255,255,255,0.5);
        font-size: 12px;
      ">
        <div style="margin-bottom: 4px;">Omni-Glass v0.1.0-alpha</div>
        <div style="margin-bottom: 4px;">The Open-Source Raycast for Screen Actions</div>
        <div style="margin-bottom: 4px;">License: MIT</div>
        <div>
          <a href="#" id="github-link" style="color: #60a5fa; text-decoration: none;">
            github.com/goshtasb/omni-glass
          </a>
        </div>
      </section>

    </div>
  `;

  // Set OCR mode radio button to current value
  try {
    const ocrMode = await invoke<string>("get_ocr_mode");
    const radio = document.getElementById(
      ocrMode === "accurate" ? "ocr-accurate" : "ocr-fast"
    ) as HTMLInputElement;
    if (radio) radio.checked = true;
  } catch {
    // Default to fast if command fails
    const radio = document.getElementById("ocr-fast") as HTMLInputElement;
    if (radio) radio.checked = true;
  }

  // Render local models section (async — fills in after main render)
  const localSection = document.getElementById("local-models-section");
  if (localSection) {
    const localHtml = await renderLocalModelsSection();
    localSection.innerHTML = localHtml;
    attachLocalModelHandlers(loadSettings);
  }

  // Wire up event handlers
  attachHandlers(config);
}

function renderProviderCard(provider: ProviderInfo, config: ProviderConfig): string {
  const isConfigured = config.configuredProviders.includes(provider.id);
  const isActive = provider.id === config.activeProvider;

  return `
    <div class="provider-card" data-provider-id="${provider.id}" style="
      background: ${isActive ? "#16213e" : "#0f1629"};
      border: 1px solid ${isActive ? "#3b82f6" : "rgba(255,255,255,0.1)"};
      border-radius: 8px;
      padding: 14px;
      margin-bottom: 10px;
      transition: border-color 0.2s;
    ">
      <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
        <span style="font-weight: 500; font-size: 14px;">${escapeHtml(provider.name)}</span>
        ${isActive ? '<span style="font-size: 11px; background: #3b82f6; padding: 2px 8px; border-radius: 10px;">Active</span>' : ""}
      </div>

      <div style="display: flex; gap: 16px; font-size: 12px; color: rgba(255,255,255,0.6); margin-bottom: 10px;">
        <span>Speed: ${stars(provider.speedStars)}</span>
        <span>Quality: ${stars(provider.qualityStars)}</span>
        <span>Cost: ${escapeHtml(provider.costPerSnip)}</span>
      </div>

      ${provider.id === "local" ? `
      <div style="font-size:12px;color:rgba(255,255,255,0.5);">
        No API key needed. Manage models in the Local Models section below.
      </div>
      ` : `
      <div style="display: flex; gap: 8px; align-items: center;">
        <input
          type="password"
          class="api-key-input"
          data-provider="${provider.id}"
          placeholder="API Key"
          style="
            flex: 1;
            padding: 6px 10px;
            background: #0d1117;
            border: 1px solid rgba(255,255,255,0.15);
            border-radius: 4px;
            color: #fff;
            font-size: 13px;
            font-family: monospace;
            outline: none;
          "
          value="${isConfigured ? "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022" : ""}"
        />
        <button
          class="save-key-btn"
          data-provider="${provider.id}"
          style="
            padding: 6px 12px;
            background: #16213e;
            border: 1px solid rgba(255,255,255,0.2);
            border-radius: 4px;
            color: #fff;
            font-size: 13px;
            cursor: pointer;
          "
        >Save</button>
        <button
          class="test-btn"
          data-provider="${provider.id}"
          style="
            padding: 6px 12px;
            background: #16213e;
            border: 1px solid rgba(255,255,255,0.2);
            border-radius: 4px;
            color: #fff;
            font-size: 13px;
            cursor: pointer;
          "
        >Test</button>
        <span class="test-result" data-provider="${provider.id}" style="font-size: 14px; width: 20px; text-align: center;">
          ${isConfigured ? "\u2713" : ""}
        </span>
      </div>
      `}
    </div>
  `;
}

function attachHandlers(config: ProviderConfig): void {
  // Provider selection dropdown
  const select = document.getElementById("provider-select") as HTMLSelectElement;
  select.addEventListener("change", async () => {
    try {
      await invoke("set_active_provider", { providerId: select.value });
      // Reload to update UI state
      await loadSettings();
    } catch (e) {
      console.error("Failed to set provider:", e);
    }
  });

  // Save API key buttons
  document.querySelectorAll(".save-key-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const providerId = (btn as HTMLElement).dataset.provider!;
      const input = document.querySelector(
        `.api-key-input[data-provider="${providerId}"]`
      ) as HTMLInputElement;
      const key = input.value.trim();

      // Don't save masked placeholder
      if (!key || key === "\u2022".repeat(16)) {
        return;
      }

      try {
        await invoke("save_api_key", { providerId, apiKey: key });
        const result = document.querySelector(
          `.test-result[data-provider="${providerId}"]`
        ) as HTMLElement;
        result.textContent = "\u2713";
        result.style.color = "#4ade80";
        // Mask the input after saving
        input.value = "\u2022".repeat(16);
        input.type = "password";
      } catch (e) {
        console.error("Failed to save key:", e);
        const result = document.querySelector(
          `.test-result[data-provider="${providerId}"]`
        ) as HTMLElement;
        result.textContent = "\u2717";
        result.style.color = "#f87171";
      }
    });
  });

  // Test connection buttons
  document.querySelectorAll(".test-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const providerId = (btn as HTMLElement).dataset.provider!;
      const result = document.querySelector(
        `.test-result[data-provider="${providerId}"]`
      ) as HTMLElement;
      result.textContent = "\u22EF";
      result.style.color = "#facc15";

      try {
        const ok = await invoke<boolean>("test_provider", { providerId });
        result.textContent = ok ? "\u2713" : "\u2717";
        result.style.color = ok ? "#4ade80" : "#f87171";
      } catch (e) {
        result.textContent = "\u2717";
        result.style.color = "#f87171";
        console.error("Test failed:", e);
      }
    });
  });

  // Focus/blur on key inputs — show/hide password
  document.querySelectorAll(".api-key-input").forEach((input) => {
    const el = input as HTMLInputElement;
    el.addEventListener("focus", () => {
      if (el.value === "\u2022".repeat(16)) {
        el.value = "";
      }
      el.type = "text";
    });
    el.addEventListener("blur", () => {
      if (el.value === "") {
        const providerId = el.dataset.provider!;
        if (config.configuredProviders.includes(providerId)) {
          el.value = "\u2022".repeat(16);
        }
      }
      el.type = "password";
    });
  });

  // OCR mode radio buttons
  document.querySelectorAll('input[name="ocr-mode"]').forEach((radio) => {
    radio.addEventListener("change", async (e) => {
      const value = (e.target as HTMLInputElement).value;
      try {
        await invoke("set_ocr_mode", { mode: value });
      } catch (err) {
        console.error("Failed to set OCR mode:", err);
      }
    });
  });

  // GitHub link
  document.getElementById("github-link")?.addEventListener("click", async (e) => {
    e.preventDefault();
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open("https://github.com/goshtasb/omni-glass");
    } catch {
      // Shell plugin may not be available
    }
  });
}

// Escape closes settings
document.addEventListener("keydown", async (e: KeyboardEvent) => {
  if (e.key === "Escape") {
    try {
      await invoke("close_settings");
    } catch {
      // Window might already be closing
    }
  }
});

loadSettings();
