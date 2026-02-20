# Week 2: Stitch the Vertical Slice

**Branch:** `feat/week-2-vertical-slice`  
**Baseline:** `b9cb88c` (Shell: 587-732ms, OCR .fast: 26ms median)  
**Budget remaining for LLM + render:** ~1,240ms  
**Goal:** User snips a screen region → action menu appears with correct contextual actions in < 2 seconds

---

## What Exists Right Now

You have two working modules that don't talk to each other:

1. **Shell** (`tray.rs` → `capture/`) — Tray click → screen freeze → bounding box drag → BMP pixel buffer on disk. Cost: ~650ms.
2. **Eyes** (`ocr-bench/` via `swift-bridge`) — Takes a PNG/image buffer → returns extracted text via Apple Vision `.fast` mode. Cost: ~26ms warm.

Week 2 connects them in sequence and adds two new stages: the Claude API call and the action menu UI.

---

## The Pipeline (4 Stages)

```
[STAGE 1: CAPTURE]          [STAGE 2: OCR]           [STAGE 3: RENDER]         [STAGE 4: LLM STREAM]
tray click                   Apple Vision              Tauri webview             Claude API (streaming)
  → screen freeze            .fast mode                skeleton menu             SSE events
  → bounding box             in-memory FFI             Copy Text ready           → skeleton at TTFT
  → crop to region           (no disk I/O)             positioned at snip        → full actions at end
  → PNG bytes in memory                                location

~650ms setup                 ~90ms                     ~0ms (async)              ~500ms TTFT
~120ms crop+encode                                                              ~2700ms total
```

**Key change from original plan:** Stage 3 (Render) happens BEFORE Stage 4 (LLM). The menu window opens immediately after OCR with a skeleton UI. The LLM streams into the already-visible window via Tauri events. This eliminates the "staring at nothing" problem.

### Stage 1 → Stage 2 Handoff: The Missing Link

Right now, Stage 1 saves a full-screen BMP and Stage 2 (the OCR bench CLI) reads a file from disk. The stitch requires:

**Crop the captured screenshot to the bounding box coordinates, then pass the cropped pixel buffer directly to the OCR FFI.**

The bounding box coordinates already come back from the frontend overlay (`mouseup` event sends `{x, y, width, height}` to Rust). The full-screen screenshot is already in memory as a `DynamicImage` from `xcap`. The crop is:

```rust
// In the Tauri command handler, after receiving coordinates from frontend:
let cropped = screenshot.crop_imm(x, y, width, height);
let rgba_bytes = cropped.to_rgba8().into_raw();

// Pass rgba_bytes + dimensions directly to OCR FFI
// Do NOT save to disk first — stay in memory
let ocr_result = recognize_text_from_buffer(
    &rgba_bytes,
    width,
    height,
    RecognitionLevel::Fast,
);
```

The OCR FFI bridge currently takes an image file path. You need to add a second entry point that accepts raw pixel bytes + dimensions. This avoids a redundant disk write → disk read round-trip.

If adding a buffer-based entry point to the Swift FFI is complex, the acceptable shortcut for the spike is: save the cropped region as a JPEG to a temp file and pass the path to the existing OCR function. This adds ~30-50ms but keeps the integration simple. Optimize later.

---

### Stage 3: Claude API Integration

This is the first real LLM call. Keep it dead simple.

**File to create:** `src-tauri/src/llm/mod.rs` (or equivalent module structure)

**What it does:**

1. Takes extracted OCR text + metadata (source app, window title, platform)
2. Builds the CLASSIFY request using the system prompt from the LLM Integration PRD (Section 4)
3. Sends it to the Anthropic Messages API
4. Parses the response as ActionMenu JSON
5. Returns the validated ActionMenu or a fallback

**The API call:**

```
POST https://api.anthropic.com/v1/messages
Headers:
  x-api-key: {from environment variable ANTHROPIC_API_KEY}
  anthropic-version: 2023-06-01
  content-type: application/json

Body:
{
  "model": "claude-sonnet-4-5-20250929",
  "max_tokens": 1024,
  "system": "<the CLASSIFY system prompt — copy verbatim from LLM PRD Section 4>",
  "messages": [
    {
      "role": "user",
      "content": "<snip_context>\n  <source_app>unknown</source_app>\n  <window_title>unknown</window_title>\n  <platform>macos</platform>\n  <ocr_confidence>0.95</ocr_confidence>\n  <has_table_structure>false</has_table_structure>\n  <has_code_structure>false</has_code_structure>\n</snip_context>\n\n<extracted_text>\n{THE OCR TEXT GOES HERE}\n</extracted_text>"
    }
  ]
}
```

**For the spike, hard-code what you don't have yet:**
- `source_app`: hard-code `"unknown"` (getting the active window name requires platform API work — that's a Week 3 task)
- `window_title`: hard-code `"unknown"`
- `ocr_confidence`: hard-code `0.95` (Apple Vision doesn't return a global confidence score in `.fast` mode — we'll derive this properly later)
- `has_table_structure`: run the table detection heuristic from LLM PRD Section 3 on the OCR text (check for consistent tabs or pipes). This is 10 lines of code and meaningfully improves classification accuracy.
- `has_code_structure`: run the code detection heuristic from LLM PRD Section 3 (check for import/function/class keywords, brackets, etc.)

**Rust HTTP client:** Use `reqwest` with async. You're already in a Tauri async command context.

```toml
# Cargo.toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

**Response parsing:**

```rust
// The response body contains:
// {
//   "content": [
//     { "type": "text", "text": "{...the ActionMenu JSON...}" }
//   ],
//   "usage": { "input_tokens": N, "output_tokens": N }
// }

// Extract the text field from the first content block
// Parse it as ActionMenu JSON
// If parsing fails → return FALLBACK_ACTIONS
```

**The FALLBACK_ACTIONS constant** (hard-code this — it's the safety net for malformed LLM responses):

```rust
const FALLBACK_ACTIONS: &str = r#"{
  "contentType": "unknown",
  "confidence": 0,
  "summary": "Could not analyze content",
  "detectedLanguage": null,
  "actions": [
    {"id": "copy_text", "label": "Copy Text", "icon": "clipboard", "priority": 1, "description": "Copy the extracted text to clipboard", "requiresExecution": false},
    {"id": "explain", "label": "Explain This", "icon": "lightbulb", "priority": 2, "description": "Explain what this content means", "requiresExecution": true},
    {"id": "search_web", "label": "Search Web", "icon": "search", "priority": 3, "description": "Search for this text online", "requiresExecution": false}
  ]
}"#;
```

**What to measure and log:**

```
[LLM] Provider: anthropic
[LLM] Model: claude-sonnet-4-5-20250929
[LLM] Input tokens: {N}
[LLM] Output tokens: {N}
[LLM] API latency: {N}ms  ← time from request sent to response received
[LLM] Parse result: success | fallback
[LLM] Content type: {contentType}
[LLM] Actions: {number of actions returned}
[LLM] Estimated cost: ${N.NNNN}
```

**API key:** Read from `std::env::var("ANTHROPIC_API_KEY")`. If missing, skip the LLM call entirely and return FALLBACK_ACTIONS. No crash, no panic. The settings UI doesn't exist yet — the engineer runs `export ANTHROPIC_API_KEY=sk-ant-...` in their terminal before `cargo tauri dev`.

---

### Stage 4: Action Menu UI

A new Tauri webview window that appears near the bounding box and displays the ActionMenu JSON as clickable buttons.

**File to create:** `src/action-menu.html` (or `.ts` — whatever the frontend engineer prefers)

**Behavior:**

1. The Rust backend emits an event (`tauri::Emitter`) with the ActionMenu JSON payload after Stage 3 completes.
2. A new webview window opens, positioned at the bottom-left corner of the bounding box.
3. The window renders the action menu.
4. Clicking an action button logs to console: `[ACTION] User clicked: {action.id}`. That's it. No execution yet.
5. Pressing Escape or clicking outside the menu dismisses it and closes the overlay.

**Visual spec (minimal):**

```
┌─────────────────────────────────────┐
│  📝 Python ModuleNotFoundError      │  ← summary (from LLM)
│  ─────────────────────────────────  │
│  🔧  Fix Error                      │  ← priority 1 action
│  💡  Explain Error                   │  ← priority 2
│  📋  Copy Text                       │  ← priority 3
│  🔍  Search Web                      │  ← priority 4
└─────────────────────────────────────┘

Width: 280px fixed
Background: #1a1a2e (dark)
Text: #ffffff
Action rows: 36px height, hover highlight #0f3460
Border-radius: 8px
Box-shadow: 0 4px 12px rgba(0,0,0,0.3)
Font: system-ui, 14px
Icons: Use emoji for the spike (replace with Lucide icons later)
```

No CSS framework. Inline styles are fine. This is a spike UI — it needs to be readable, not beautiful.

**Window positioning:**

The action menu window should appear at approximately:
- x: bounding box left edge
- y: bounding box bottom edge + 8px gap

The bounding box coordinates are known from Stage 1. Pass them through the pipeline.

If positioning is complex due to Tauri's window coordinate system, just center the menu window on screen for now. Positioning is a polish task, not a functional requirement.

---

## The End-to-End Timing Log

When the full pipeline runs, the terminal output should look like this:

```
[CAPTURE] Tray clicked at 1771558842645
[CAPTURE] xcap_capture: 287ms
[CAPTURE] png_save: 428ms
[CAPTURE] window_create: 82ms
[CAPTURE] Overlay visible

... user drags rectangle ...

[CAPTURE] Bounding box received: {x: 120, y: 340, w: 800, h: 200}
[CAPTURE] Region crop: 11ms
[CAPTURE] PNG encode: 85ms (282114 bytes)
[OCR] Recognition level: fast
[OCR] Extracted 854 chars in 87ms
[OCR] has_table_structure: true
[OCR] has_code_structure: false
[RENDER] Skeleton menu window created in 0ms                    ← MENU VISIBLE
[PIPELINE] Local processing: 184ms (crop=11 + encode=85 + ocr=87 + window=0)
[LLM] Provider: anthropic (streaming)
[LLM] Model: claude-haiku-4-5-20251001
[LLM] TTFB: 495ms
[LLM] Input tokens: 1414
[LLM] TTFT: 495ms
[LLM] Skeleton emitted at 793ms                                ← SUMMARY VISIBLE
[LLM] Output tokens: 414
[LLM] Estimated cost: $0.002
[LLM] Stream complete: 2857ms
[LLM] Parse result: success
[LLM] Content type: mixed
[LLM] Actions: 5                                               ← ALL ACTIONS VISIBLE
[PIPELINE] Total (mouse-up to actions complete): 2983ms
[PIPELINE] Perceived latency (mouse-up to skeleton): ~184ms + TTFT
```

There are now **three numbers that matter**:
1. `[RENDER] Skeleton menu window created` — how fast the user sees *something* (~184ms)
2. `[LLM] Skeleton emitted` — how fast the user sees the summary + content type (~793ms from LLM start)
3. `[PIPELINE] Total` — how fast all actions are clickable (~2983ms)

Everything before the bounding box (tray click → overlay) is setup time the user expects.

---

## End-of-Week-2 Deliverables

1. **Working demo:** Snip a screen region → action menu appears with contextually correct actions. Record a screen capture video.

2. **Latency benchmark:** Run the pipeline against 3 live snips. Record both streaming milestones:

   | Snip | Content | Local (ms) | Skeleton (ms) | Full Actions (ms) | Skeleton < 1.5s? | Full < 4s? |
   |------|---------|-----------|--------------|-------------------|------------------|-----------|
   | 1 | Code (1078 chars) | 251 | 893 | 3346 | PASS | PASS |
   | 2 | Mixed (854 chars) | 126 | 793 | 2983 | PASS | PASS |
   | 3 | Code (599 chars) | 72 | 802 | 2737 | PASS | PASS |

3. **Classification accuracy:** For each of the 3 images, verify that:
   - `contentType` is correct
   - The top action (priority 1) is the most useful action for that content
   - The action list makes sense (no "Export CSV" for a single sentence, etc.)

4. **Edge case log:** Document anything weird that happened. LLM returned invalid JSON? OCR missed text? Menu appeared in the wrong place? Write it down. These become Week 3 tickets.

---

## What NOT to Build This Week

| Don't | Why |
|-------|-----|
| Action execution (clicking buttons does things) | Week 4. Buttons log to console only. |
| Provider abstraction (OpenAI, Gemini) | Week 3. Hard-code Anthropic only. |
| Settings UI | Week 3. Use environment variable for API key. |
| Sensitive data redaction | Week 4. |
| Source app / window title detection | Week 3. Hard-code "unknown". |
| Error handling UI (pretty error messages) | Later. Errors go to terminal logs. |
| Any visual polish beyond basic readability | Later. |

---

## File Structure (Expected After Week 2)

```
src-tauri/
  src/
    lib.rs
    main.rs
    tray.rs                    ← existing (Stage 1)
    capture/
      mod.rs                   ← existing
      screenshot.rs            ← existing
      region.rs                ← existing
    llm/
      mod.rs                   ← NEW: LLM provider call
      classify.rs              ← NEW: builds CLASSIFY request, parses response
      prompts.rs               ← NEW: system prompt constants (copy from LLM PRD)
      types.rs                 ← NEW: ActionMenu, Action structs + serde
    ocr/
      mod.rs                   ← NEW or refactored: wraps swift-bridge FFI
      heuristics.rs            ← NEW: table_structure + code_structure detection
src/
  overlay.ts                   ← existing (Stage 1 overlay)
  action-menu.ts               ← NEW (Stage 4 menu UI)
  styles.css                   ← existing
tools/
  ocr-bench/                   ← existing (can stay as standalone for benchmarking)
```

This structure maps directly to the 4-layer architecture in the PRD: Shell (`tray.rs`, `capture/`), Eyes (`ocr/`), Brain (`llm/`), Hands (not yet — Week 4).

---

## The Moment of Truth — Updated for Streaming Architecture

The original question was: **"Under 2 seconds from mouse-up to action menu?"**

The streaming architecture reframes this into **two moments that matter**:

1. **Snip-to-skeleton:** How long until the user sees a menu window with a summary and a clickable Copy Text button?
2. **Snip-to-full-actions:** How long until all LLM-generated action buttons are visible and clickable?

### Architecture Change: Why Streaming

The non-streaming pipeline required the entire LLM response before showing anything. With Haiku's ~500ms TTFT and ~2.5s total generation, the user stared at nothing for 2.5+ seconds after the local processing completed.

The streaming architecture (`"stream": true` on the Anthropic Messages API) changed this:
- Action menu window opens **immediately** after OCR (before the LLM call starts)
- Skeleton UI shows shimmer placeholders + a clickable **Copy Text** button
- `"action-menu-skeleton"` Tauri event fires when `contentType` + `summary` are parsed from partial JSON (~800ms into stream)
- `"action-menu-complete"` Tauri event fires when the full ActionMenu JSON is parsed (stream end)

### Revised Performance Targets

| Metric | Target | Haiku Actual | Sonnet Actual | Status |
|--------|--------|-------------|---------------|--------|
| Snip-to-skeleton (window visible + Copy Text clickable) | < 1,500ms | 72–251ms | 72–251ms | **PASS** |
| Snip-to-summary (skeleton + contentType + summary text) | < 1,500ms | 565–1,144ms | 1,046–6,462ms | **Haiku PASS / Sonnet FAIL** |
| Snip-to-full-actions (all buttons rendered) | < 4,000ms | 2,737–3,346ms | 7,388–7,962ms | **Haiku PASS / Sonnet FAIL** |
| Copy Text available | < 1,500ms | 72–251ms | 72–251ms | **PASS** |

### Haiku Benchmark (3 snips, `claude-haiku-4-5-20251001`)

| Snip | Content | Local (ms) | TTFT (ms) | Skeleton (ms) | Complete (ms) | Total (ms) | Actions | Cost |
|------|---------|-----------|-----------|--------------|--------------|-----------|---------|------|
| 1 | Code (1078 chars) | 251 | 587 | 893 | 3095 | 3346 | 5 | ~$0.002 |
| 2 | Mixed (854 chars) | 126 | 495 | 793 | 2857 | 2983 | 5 | ~$0.002 |
| 3 | Code (599 chars) | 72 | 496 | 802 | 2665 | 2737 | 5 | ~$0.002 |

**Haiku summary:** Skeleton visible in < 300ms. Summary populated by ~900ms. Full actions by ~3s. Cost: ~$0.002/snip.

### Sonnet Benchmark (3 snips, `claude-sonnet-4-5-20250929`)

| Snip | Content | Local (ms) | TTFT (ms) | Skeleton (ms) | Complete (ms) | Total (ms) | Actions | Cost |
|------|---------|-----------|-----------|--------------|--------------|-----------|---------|------|
| 1 | Code (1078 chars) | 253 | 836 | 1465 | 6940 | 7178 | 6 | $0.012 |
| 2 | Mixed (854 chars) | 184 | 1559 | 2323 | 6960 | 7145 | 5 | $0.010 |
| 3 | KV pairs (599 chars) | 210 | 5556 | 6252 | 7541 | 7752 | 5 | $0.010 |

**Sonnet summary:** TTFT variance is high (836ms–5.5s). Total 7–8 seconds. Cost: ~$0.011/snip (5.5x Haiku). **Disqualified as default model.** Sonnet will be offered as an optional "quality" mode in the settings panel (Week 3).

### Model Decision

| | Haiku | Sonnet |
|--|-------|--------|
| Role | **Default** | Optional quality mode |
| Skeleton < 1.5s | Yes | No (Snip 3: 6.2s) |
| Full actions < 4s | Yes (marginal) | No (7–8s) |
| Cost per snip | ~$0.002 | ~$0.011 |
| Classification accuracy | Good (all 3 correct) | Good (all 3 correct) |

### What "< 2 seconds" Means Now

The original target of "< 2 seconds from mouse-up to action menu" is **met in spirit** by the streaming architecture:
- The user sees a **usable menu** (Copy Text clickable) in **72–251ms** — far under 2 seconds
- The user sees the **summary** (what was snipped) in **565–1,144ms** — under 2 seconds
- The user sees **all LLM-generated actions** in **2.7–3.3s** — over 2 seconds, but the UX is smooth because the menu is already visible and the content fills in progressively

The perceived latency is a **pass**. The user never stares at a blank screen.
