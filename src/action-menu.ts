/**
 * Action menu UI — two-state streaming renderer.
 *
 * State 1 (Skeleton): Shows immediately when the window opens.
 *   - Shimmer placeholder for summary
 *   - Copy Text button (always available — OCR text is already stored)
 *   - 3 shimmer placeholders for loading actions
 *
 * State 2 (Complete): Fills in when the streaming LLM response finishes.
 *   - Real summary text (replaces shimmer)
 *   - All action buttons with icons and labels
 *
 * Events from Rust:
 *   - "action-menu-skeleton": { contentType, summary } — updates summary text
 *   - "action-menu-complete": full ActionMenu JSON — renders all actions
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";

import {
  ActionMenu,
  ActionMenuSkeleton,
  renderSkeleton,
  updateSummary,
  renderMenu,
  showLoading,
  showFeedback,
  closeAfterDelay,
} from "./action-menu-render";

import {
  ActionResult,
  showTextResult,
  handleFileResult,
  handleCommandResult,
} from "./action-menu-results";

// ── State ───────────────────────────────────────────────────────────

let menuRendered = false;
let actionInProgress = false;

// ── Action execution ─────────────────────────────────────────────────

async function executeAction(actionId: string): Promise<void> {
  try {
    // Local actions — no LLM call needed
    if (actionId === "copy_text" || actionId === "copy_command" || actionId === "copy_traceback" || actionId === "copy_code") {
      const text = await invoke<string>("get_ocr_text");
      await invoke("copy_to_clipboard", { text });
      showFeedback(`Copied ${text.length} chars`);
      closeAfterDelay(800);
      return;
    }

    if (actionId === "search_web" || actionId === "search_error" || actionId === "search_command" || actionId === "search_online" || actionId === "search_docs") {
      const text = await invoke<string>("get_ocr_text");
      const query = text.slice(0, 200).trim();
      const url = `https://www.google.com/search?q=${encodeURIComponent(query)}`;
      await open(url);
      showFeedback("Opening search...");
      closeAfterDelay(800);
      return;
    }

    // LLM-backed actions — call execute_action Tauri command
    actionInProgress = true;
    showLoading(actionId);

    const result = await invoke<ActionResult>("execute_action", { actionId });
    console.log(`[ACTION] Result: status=${result.status}, type=${result.result.type}`);

    if (result.status === "error") {
      console.error(`[ACTION] Execute error: ${result.result.text}`);
      showFeedback(result.result.text || "Action failed", true);
      return;
    }

    switch (result.result.type) {
      case "text":
        showTextResult(result.result.text || "No content returned.");
        break;
      case "clipboard":
        if (result.result.clipboardContent) {
          await invoke("copy_to_clipboard", { text: result.result.clipboardContent });
          showFeedback("Copied to clipboard");
          closeAfterDelay(800);
        }
        break;
      case "file":
        await handleFileResult(result);
        break;
      case "command":
        await handleCommandResult(result);
        break;
      default:
        showFeedback(`Unknown result type: ${result.result.type}`, true);
    }
  } catch (err) {
    console.error(`[ACTION] Failed to execute ${actionId}:`, err);
    showFeedback(`Error: ${err}`, true);
  }
}

// ── Init ─────────────────────────────────────────────────────────────

async function init(): Promise<void> {
  renderSkeleton();

  // Inject hover CSS (replaces per-element JS hover handlers)
  const style = document.createElement("style");
  style.textContent = `.action-row:hover { background: #0f3460 !important; }`;
  document.head.appendChild(style);

  // Event delegation for action clicks
  document.getElementById("action-menu")!.addEventListener("click", async (e) => {
    const row = (e.target as HTMLElement).closest(".action-row") as HTMLElement | null;
    if (row?.dataset.actionId) {
      console.log(`[ACTION] User clicked: ${row.dataset.actionId}`);
      await executeAction(row.dataset.actionId);
    }
  });

  // Listen for streaming events from Rust
  listen<ActionMenuSkeleton>("action-menu-skeleton", (event) => {
    console.log("[RENDER] Received skeleton event");
    updateSummary(event.payload);
  });

  listen<ActionMenu>("action-menu-complete", (event) => {
    console.log("[RENDER] Received complete event:", event.payload.contentType);
    menuRendered = true;
    renderMenu(event.payload);
  });

  pollForMenu();
}

// ── Polling ──────────────────────────────────────────────────────────

async function pollForMenu(): Promise<void> {
  const MAX_POLLS = 20;
  let polls = 0;

  const timer = setInterval(async () => {
    polls++;
    if (menuRendered) {
      clearInterval(timer);
      return;
    }

    try {
      const menu = (await invoke("get_action_menu")) as ActionMenu;
      console.log(`[RENDER] Poll #${polls}: got menu (type=${menu.contentType})`);
      menuRendered = true;
      renderMenu(menu);
      clearInterval(timer);
    } catch {
      if (polls >= MAX_POLLS) {
        console.log(`[RENDER] Poll timeout after ${polls} attempts`);
        clearInterval(timer);
      }
    }
  }, 500);
}

// ── Global event handlers ────────────────────────────────────────────

document.addEventListener("keydown", async (e: KeyboardEvent) => {
  if (e.key === "Escape") {
    try { await invoke("close_action_menu"); } catch { /* closing */ }
  }
});

window.addEventListener("blur", async () => {
  if (!menuRendered || actionInProgress) return;
  try { await invoke("close_action_menu"); } catch { /* closing */ }
});

init();
