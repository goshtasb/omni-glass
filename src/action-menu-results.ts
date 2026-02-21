/**
 * Action menu result handlers — displays results from LLM execute.
 *
 * Handles text results (with code block extraction), file export
 * (native save dialog), and command confirmation (opens dialog window).
 */

import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { save } from "@tauri-apps/plugin-dialog";
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { escapeHtml, showFeedback, closeAfterDelay } from "./action-menu-render";

// ── Types ────────────────────────────────────────────────────────────

export interface ActionResultBody {
  type: string;
  text?: string;
  filePath?: string;
  command?: string;
  clipboardContent?: string;
  mimeType?: string;
}

export interface ActionResultMeta {
  tokensUsed?: number;
  processingNote?: string;
}

export interface ActionResult {
  status: string;
  actionId: string;
  result: ActionResultBody;
  metadata?: ActionResultMeta;
}

// ── Text result ──────────────────────────────────────────────────────

export async function showTextResult(text: string): Promise<void> {
  const container = document.getElementById("action-menu")!;
  const wrapper = container.querySelector("div")!;
  wrapper.style.width = "380px";

  const codeBlock = extractCodeBlock(text);
  const rendered = renderMarkdownLight(text);

  const actionsEl = document.getElementById("menu-actions");
  if (actionsEl) {
    actionsEl.innerHTML = `
      <div style="
        padding: 12px 14px;
        font-size: 13px;
        color: rgba(255,255,255,0.9);
        line-height: 1.5;
        max-height: 300px;
        overflow-y: auto;
        word-wrap: break-word;
      ">${rendered}</div>
      <div style="
        padding: 6px 14px 8px;
        display: flex;
        gap: 8px;
        justify-content: flex-end;
        border-top: 1px solid rgba(255,255,255,0.1);
      ">
        ${codeBlock ? `<button id="btn-copy-fix" style="
          background: rgba(74,222,128,0.15);
          border: 1px solid rgba(74,222,128,0.4);
          color: #4ade80;
          padding: 4px 12px;
          border-radius: 4px;
          cursor: pointer;
          font-size: 12px;
        ">Copy Fix</button>` : ""}
        <button id="btn-copy-result" style="
          background: transparent;
          border: 1px solid rgba(255,255,255,0.2);
          color: rgba(255,255,255,0.8);
          padding: 4px 12px;
          border-radius: 4px;
          cursor: pointer;
          font-size: 12px;
        ">Copy All</button>
        <button id="btn-close-result" style="
          background: transparent;
          border: 1px solid rgba(255,255,255,0.2);
          color: rgba(255,255,255,0.8);
          padding: 4px 12px;
          border-radius: 4px;
          cursor: pointer;
          font-size: 12px;
        ">Close</button>
      </div>
    `;

    if (codeBlock) {
      document.getElementById("btn-copy-fix")?.addEventListener("click", async () => {
        await invoke("copy_to_clipboard", { text: codeBlock });
        showFeedback("Fix copied");
        closeAfterDelay(600);
      });
    }

    document.getElementById("btn-copy-result")?.addEventListener("click", async () => {
      await invoke("copy_to_clipboard", { text });
      showFeedback("Copied");
      closeAfterDelay(600);
    });

    document.getElementById("btn-close-result")?.addEventListener("click", async () => {
      try { await invoke("close_action_menu"); } catch { /* closing */ }
    });

    requestAnimationFrame(async () => {
      const contentEl = actionsEl.querySelector("div");
      if (contentEl) {
        const contentHeight = contentEl.scrollHeight;
        const totalHeight = Math.min(contentHeight + 110, 500);
        try {
          const win = getCurrentWebviewWindow();
          await win.setSize(new LogicalSize(400, totalHeight));
        } catch { /* resize not critical */ }
      }
    });
  }
}

// ── File result ──────────────────────────────────────────────────────

export async function handleFileResult(result: ActionResult): Promise<void> {
  const content = result.result.text || "";
  const filename = result.result.filePath || "export.csv";
  const ext = filename.split(".").pop() || "csv";

  try {
    const chosenPath = await save({
      defaultPath: filename,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });

    if (!chosenPath) return;

    await invoke<string>("write_file_to_path", { filePath: chosenPath, content });
    console.log(`[ACTION] File written to: ${chosenPath}`);

    const savedName = chosenPath.split("/").pop() || filename;

    const actionsEl = document.getElementById("menu-actions");
    if (actionsEl) {
      actionsEl.innerHTML = `
        <div style="padding: 16px 14px; text-align: center;">
          <div style="color: #4ade80; font-size: 14px; font-weight: 600; margin-bottom: 6px;">
            File saved
          </div>
          <div style="
            color: rgba(255,255,255,0.7);
            font-size: 12px;
            font-family: 'SF Mono', Menlo, monospace;
            background: rgba(0,0,0,0.3);
            padding: 6px 10px;
            border-radius: 4px;
            margin-bottom: 12px;
            word-break: break-all;
          ">${escapeHtml(savedName)}</div>
          <div style="display: flex; gap: 8px; justify-content: center;">
            <button id="btn-open-file" style="
              background: rgba(74,222,128,0.15);
              border: 1px solid rgba(74,222,128,0.4);
              color: #4ade80;
              padding: 5px 14px;
              border-radius: 4px;
              cursor: pointer;
              font-size: 12px;
            ">Open File</button>
            <button id="btn-close-file" style="
              background: transparent;
              border: 1px solid rgba(255,255,255,0.2);
              color: rgba(255,255,255,0.7);
              padding: 5px 14px;
              border-radius: 4px;
              cursor: pointer;
              font-size: 12px;
            ">Done</button>
          </div>
        </div>
      `;

      document.getElementById("btn-open-file")?.addEventListener("click", async () => {
        try { await open(chosenPath); } catch { /* best effort */ }
        try { await invoke("close_action_menu"); } catch { /* closing */ }
      });

      document.getElementById("btn-close-file")?.addEventListener("click", async () => {
        try { await invoke("close_action_menu"); } catch { /* closing */ }
      });
    }
  } catch (err) {
    showFeedback(`File export failed: ${err}`, true);
  }
}

// ── Command result ───────────────────────────────────────────────────

export async function handleCommandResult(result: ActionResult): Promise<void> {
  const command = result.result.command || "";
  const explanation = result.result.text || "Run this command?";

  try {
    const confirmWindow = new WebviewWindow("confirm-dialog", {
      url: "confirm-dialog.html",
      title: "Confirm Command",
      width: 480,
      height: 300,
      decorations: false,
      alwaysOnTop: true,
      resizable: false,
      skipTaskbar: true,
    });

    confirmWindow.once("tauri://created", async () => {
      setTimeout(async () => {
        await emit("confirm-command", {
          command,
          explanation,
          actionId: result.actionId,
        });
      }, 200);
    });

    try { await invoke("close_action_menu"); } catch { /* closing */ }
  } catch (err) {
    console.error("[ACTION] Failed to open confirmation dialog:", err);
    showFeedback(`Error: ${err}`, true);
  }
}

// ── Internal helpers ─────────────────────────────────────────────────

/** Extract content from the first ``` code block, or null if none found. */
function extractCodeBlock(text: string): string | null {
  const match = text.match(/```[\w]*\n([\s\S]*?)```/);
  return match ? match[1].trim() : null;
}

/** Lightweight markdown to HTML: code blocks, inline code, bold, line breaks. */
function renderMarkdownLight(text: string): string {
  let html = escapeHtml(text);

  html = html.replace(
    /```(\w*)\n([\s\S]*?)```/g,
    (_match, _lang, code) => `<pre style="
      background: rgba(0,0,0,0.4);
      border: 1px solid rgba(255,255,255,0.1);
      border-radius: 4px;
      padding: 8px 10px;
      margin: 6px 0;
      font-family: 'SF Mono', Menlo, monospace;
      font-size: 12px;
      line-height: 1.4;
      overflow-x: auto;
      white-space: pre;
      color: #e2e8f0;
    ">${code.trim()}</pre>`
  );

  html = html.replace(
    /`([^`]+)`/g,
    `<code style="
      background: rgba(0,0,0,0.3);
      padding: 1px 4px;
      border-radius: 3px;
      font-family: 'SF Mono', Menlo, monospace;
      font-size: 12px;
    ">$1</code>`
  );

  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replace(/\n/g, "<br>");
  html = html.replace(/<pre([^>]*)>([\s\S]*?)<\/pre>/g, (_m, attrs, content) => {
    return `<pre${attrs}>${content.replace(/<br>/g, "\n")}</pre>`;
  });

  return html;
}
