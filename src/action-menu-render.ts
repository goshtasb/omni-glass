/**
 * Action menu rendering — skeleton, complete menu, and UI helpers.
 *
 * Exports rendering functions and shared types used by the action menu.
 * No Tauri API imports here — pure DOM manipulation.
 */

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition } from "@tauri-apps/api/dpi";

// ── Shared types ─────────────────────────────────────────────────────

export interface Action {
  id: string;
  label: string;
  icon: string;
  priority: number;
  description: string;
  requiresExecution: boolean;
}

export interface ActionMenu {
  contentType: string;
  confidence: number;
  summary: string;
  detectedLanguage: string | null;
  actions: Action[];
}

export interface ActionMenuSkeleton {
  contentType: string;
  summary: string;
}

// ── Icons ────────────────────────────────────────────────────────────

const ICON_MAP: Record<string, string> = {
  clipboard: "\u{1F4CB}",
  table: "\u{1F4CA}",
  code: "\u{1F4BB}",
  lightbulb: "\u{1F4A1}",
  wrench: "\u{1F527}",
  language: "\u{1F310}",
  search: "\u{1F50D}",
  file: "\u{1F4C4}",
  terminal: "\u{2B1B}",
  mail: "\u{1F4E7}",
  calculator: "\u{1F522}",
  link: "\u{1F517}",
  download: "\u{2B07}\u{FE0F}",
  eye: "\u{1F441}\u{FE0F}",
  edit: "\u{270F}\u{FE0F}",
  sparkles: "\u{2728}",
};

export function getIcon(name: string): string {
  return ICON_MAP[name] || "\u{2022}";
}

export function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

// ── Drag handle ─────────────────────────────────────────────────────

const DRAG_HANDLE = `
  <div class="drag-handle" style="
    height: 18px;
    cursor: grab;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255,255,255,0.03);
    border-bottom: 1px solid rgba(255,255,255,0.06);
  ">
    <div style="
      width: 32px;
      height: 4px;
      border-radius: 2px;
      background: rgba(255,255,255,0.2);
      pointer-events: none;
    "></div>
  </div>
`;

let dragging = false;
export function isDragging(): boolean { return dragging; }

function attachDragListener(): void {
  const handle = document.querySelector(".drag-handle") as HTMLElement | null;
  if (!handle) return;
  const win = getCurrentWindow();
  let startX = 0;
  let startY = 0;
  let winX = 0;
  let winY = 0;

  handle.addEventListener("mousedown", async (e) => {
    e.preventDefault();
    dragging = true;
    const scale = window.devicePixelRatio;
    startX = e.screenX;
    startY = e.screenY;
    const pos = await win.outerPosition();
    winX = pos.x;
    winY = pos.y;
    handle.style.cursor = "grabbing";

    const onMove = (ev: MouseEvent) => {
      const dx = (ev.screenX - startX) * scale;
      const dy = (ev.screenY - startY) * scale;
      win.setPosition(new PhysicalPosition(winX + dx, winY + dy));
    };

    const onUp = () => {
      dragging = false;
      handle.style.cursor = "grab";
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  });
}

// ── Skeleton (State 1) ──────────────────────────────────────────────

export function renderSkeleton(): void {
  const container = document.getElementById("action-menu")!;

  container.innerHTML = `
    <div style="
      background: #1a1a2e;
      border-radius: 8px;
      box-shadow: 0 4px 12px rgba(0,0,0,0.3);
      width: 280px;
      overflow: hidden;
      border: 1px solid rgba(255,255,255,0.1);
    ">
      ${DRAG_HANDLE}
      <div id="menu-summary" style="
        padding: 10px 14px;
        font-size: 13px;
        color: rgba(255,255,255,0.7);
        border-bottom: 1px solid rgba(255,255,255,0.1);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        min-height: 35px;
        display: flex;
        align-items: center;
      ">
        <div class="shimmer" style="height:14px;width:70%;"></div>
      </div>
      <div id="menu-actions">
        <div class="action-row" data-action-id="copy_text" style="
          padding: 0 14px;
          height: 36px;
          display: flex;
          align-items: center;
          gap: 10px;
          cursor: pointer;
          transition: background 0.1s;
        " title="Copy the extracted text to clipboard">
          <span style="font-size: 16px; width: 20px; text-align: center;">\u{1F4CB}</span>
          <span style="flex: 1; font-size: 14px;">Copy Text</span>
        </div>
        <div style="padding:8px 14px;display:flex;align-items:center;gap:10px;">
          <div class="shimmer" style="width:20px;height:20px;"></div>
          <div class="shimmer" style="flex:1;height:14px;"></div>
        </div>
        <div style="padding:8px 14px;display:flex;align-items:center;gap:10px;">
          <div class="shimmer" style="width:20px;height:20px;"></div>
          <div class="shimmer" style="flex:1;height:14px;"></div>
        </div>
        <div style="padding:8px 14px;display:flex;align-items:center;gap:10px;">
          <div class="shimmer" style="width:20px;height:20px;"></div>
          <div class="shimmer" style="flex:1;height:14px;"></div>
        </div>
      </div>
    </div>
  `;
  attachDragListener();
}

// ── Skeleton update ─────────────────────────────────────────────────

export function updateSummary(skeleton: ActionMenuSkeleton): void {
  const summaryEl = document.getElementById("menu-summary");
  if (summaryEl) {
    summaryEl.innerHTML = "";
    summaryEl.textContent = skeleton.summary;
    console.log(
      `[RENDER] Skeleton updated: type=${skeleton.contentType}, summary="${skeleton.summary}"`
    );
  }
}

// ── Complete menu (State 2) ─────────────────────────────────────────

export function renderMenu(menu: ActionMenu): void {
  const container = document.getElementById("action-menu")!;
  const sorted = [...menu.actions].sort((a, b) => a.priority - b.priority);

  container.innerHTML = `
    <div style="
      background: #1a1a2e;
      border-radius: 8px;
      box-shadow: 0 4px 12px rgba(0,0,0,0.3);
      width: 280px;
      overflow: hidden;
      border: 1px solid rgba(255,255,255,0.1);
    ">
      ${DRAG_HANDLE}
      <div style="
        padding: 10px 14px;
        font-size: 13px;
        color: rgba(255,255,255,0.7);
        border-bottom: 1px solid rgba(255,255,255,0.1);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      ">
        ${escapeHtml(menu.summary)}
      </div>
      <div id="menu-actions">
      ${sorted
        .map(
          (action) => `
        <div class="action-row" data-action-id="${action.id}" style="
          padding: 0 14px;
          height: 36px;
          display: flex;
          align-items: center;
          gap: 10px;
          cursor: pointer;
          transition: background 0.1s;
        " title="${escapeHtml(action.description)}">
          <span style="font-size: 16px; width: 20px; text-align: center;">${getIcon(action.icon)}</span>
          <span style="flex: 1; font-size: 14px;">${escapeHtml(action.label)}</span>
        </div>
      `
        )
        .join("")}
      </div>
    </div>
  `;

  attachDragListener();

  console.log(
    `[RENDER] Complete menu: ${menu.actions.length} actions, type=${menu.contentType}`
  );
}

// ── UI helpers ──────────────────────────────────────────────────────

export function showLoading(actionId: string): void {
  const actionsEl = document.getElementById("menu-actions");
  if (actionsEl) {
    actionsEl.querySelectorAll(".action-row").forEach((row) => {
      const el = row as HTMLElement;
      if (el.dataset.actionId === actionId) {
        el.style.opacity = "0.6";
        el.style.pointerEvents = "none";
        const label = el.querySelector("span:last-child");
        if (label) label.textContent = "Working...";
      }
    });
  }
}

export function showFeedback(message: string, isError = false): void {
  const container = document.getElementById("action-menu")!;
  const feedback = document.createElement("div");
  const color = isError ? "#fca5a5" : "#4ade80";
  feedback.style.cssText = `padding:8px 14px;text-align:center;color:${color};font-size:13px;background:#1a1a2e;border-top:1px solid rgba(255,255,255,0.1)`;
  feedback.textContent = message;
  container.querySelector("div")?.appendChild(feedback);
}

export function closeAfterDelay(ms: number): void {
  setTimeout(async () => {
    try { await invoke("close_action_menu"); } catch { /* closing */ }
  }, ms);
}
