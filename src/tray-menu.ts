/**
 * Tray menu — appears when the user clicks the menu bar icon.
 *
 * Two options: Snip Screen (capture flow) or Type Command (text launcher).
 * Click outside or Escape dismisses the menu.
 */

import { invoke } from "@tauri-apps/api/core";

// ── Render ───────────────────────────────────────────────────────────

const container = document.getElementById("tray-menu")!;
container.innerHTML = `
  <div style="
    background: #1a1a2e;
    border: 1px solid rgba(255,255,255,0.15);
    border-radius: 8px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    overflow: hidden;
    user-select: none;
  ">
    <div class="row" id="snip-screen" style="
      padding: 10px 14px;
      color: #e2e8f0;
      font-size: 13px;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 10px;
    ">
      <span style="font-size: 15px;">📷</span>
      <span>Snip Screen</span>
    </div>
    <div style="height: 1px; background: rgba(255,255,255,0.08);"></div>
    <div class="row" id="type-command" style="
      padding: 10px 14px;
      color: #e2e8f0;
      font-size: 13px;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 10px;
    ">
      <span style="font-size: 15px;">⌨️</span>
      <span>Type Command</span>
    </div>
  </div>
`;

// ── Hover styles ─────────────────────────────────────────────────────

const style = document.createElement("style");
style.textContent = `.row:hover { background: rgba(255,255,255,0.08); }`;
document.head.appendChild(style);

// ── Actions ──────────────────────────────────────────────────────────

async function closeMenu(): Promise<void> {
  try { await invoke("close_tray_menu"); } catch { /* closing */ }
}

document.getElementById("snip-screen")?.addEventListener("click", async () => {
  await closeMenu();
  await invoke("start_snip");
});

document.getElementById("type-command")?.addEventListener("click", async () => {
  await closeMenu();
  await invoke("open_text_launcher");
});

// ── Dismiss on Escape ────────────────────────────────────────────────
// No blur listener — macOS doesn't give focus to borderless windows
// opened from tray icon clicks (app isn't "active"). The menu closes
// via: option click, Escape, or clicking the tray icon again (toggle).

document.addEventListener("keydown", (e: KeyboardEvent) => {
  if (e.key === "Escape") closeMenu();
});
