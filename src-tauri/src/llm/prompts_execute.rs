//! EXECUTE pipeline prompts — per-action system and user message templates.
//!
//! The EXECUTE call is the second LLM interaction. The user has already
//! seen the action menu (from CLASSIFY) and clicked a specific action.
//! Now we send the OCR text + action-specific instructions and get
//! a structured result back.

pub const EXECUTE_MAX_TOKENS: u32 = 1024;

/// EXECUTE system prompt — instructs the LLM to perform a specific action
/// on the extracted text and return a structured JSON result.
pub const EXECUTE_SYSTEM_PROMPT: &str = r#"You are the action executor for Omni-Glass, a desktop AI utility. The user snipped a region of their screen, the OCR layer extracted text, and the user selected a specific action to perform on that text. Your job is to execute that action and return a structured JSON result.

<role>
You execute actions on extracted screen text. You return structured JSON results. You do NOT make up information — if you can't perform the action, say so in the result.
</role>

<rules>
1. ALWAYS respond with valid JSON matching the ActionResult schema below.
2. For "text" results: provide clear, concise, actionable explanations (3-8 sentences).
3. For "command" results: ALWAYS set status to "needs_confirmation". Never assume commands should auto-execute.
4. For "file" results: provide the complete file content in the text field.
5. For "command" results: suggest the simplest, safest command. Prefer package managers over manual installs.
6. NEVER suggest destructive commands (rm -rf, format, dd, etc.).
7. NEVER include API keys, credentials, or sensitive data in your response.
8. If the extracted text is insufficient to perform the action, return status "error" with an explanation.
</rules>

<response_format>
{
  "status": "success" | "error" | "needs_confirmation",
  "actionId": "<the action that was requested>",
  "result": {
    "type": "text" | "file" | "command" | "clipboard",
    "text": "<explanation text or file content>",
    "filePath": "<suggested filename for file results>",
    "command": "<shell command for command results>",
    "mimeType": "<MIME type for file results>"
  },
  "metadata": {
    "processingNote": "<optional note about the result>"
  }
}
</response_format>"#;

// ── Per-action user message templates ──────────────────────────────

pub const PROMPT_EXPLAIN_ERROR: &str = r#"Action: explain_error

Analyze this error message or stack trace and explain:
1. What the error means in plain English
2. Why it likely occurred
3. The most common cause

Keep the explanation concise (3-5 sentences). A developer is reading this.

Return result type "text" with your explanation.

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_EXPLAIN: &str = r#"Action: explain

Explain this content clearly and concisely:
1. What this content is
2. Key information or meaning
3. Any important context

Keep the explanation concise (3-5 sentences). Be helpful, not verbose.

Return result type "text" with your explanation.

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_SUGGEST_FIX: &str = r#"Action: suggest_fix

Analyze this error and suggest a fix command.

Platform: {platform}
Shell: {detected_shell}

Requirements:
- Suggest ONE command that is most likely to fix the issue
- Prefer package manager commands (pip install, npm install, brew install, cargo add, etc.)
- The command must be safe and non-destructive
- Set status to "needs_confirmation" — the user must approve before execution
- Include a brief explanation of what the command does and why

Return result type "command" with the fix command.

<extracted_text>
{extracted_text}
</extracted_text>"#;

pub const PROMPT_EXPORT_CSV: &str = r#"Action: export_csv

Extract the tabular data from this text and format it as a valid CSV file.

Requirements:
- Detect column headers and data rows
- Use comma as delimiter, double-quote fields that contain commas
- Include a header row
- If the data isn't clearly tabular, do your best to extract structured rows
- Suggest a descriptive filename (e.g., "sales_data_export.csv")

Return result type "file" with mimeType "text/csv".
Put the CSV content in the "text" field.
Put the suggested filename in the "filePath" field.

<extracted_text>
{extracted_text}
</extracted_text>"#;

/// Build the user message for an EXECUTE call by selecting the
/// appropriate action template and filling in placeholders.
pub fn build_execute_message(
    action_id: &str,
    extracted_text: &str,
    platform: &str,
) -> String {
    let template = match action_id {
        "explain_error" | "explain_script" | "explain_code" => PROMPT_EXPLAIN_ERROR,
        "explain" | "explain_this" | "review_ocr" => PROMPT_EXPLAIN,
        "suggest_fix" | "fix_error" | "fix_syntax" | "fix_code" => PROMPT_SUGGEST_FIX,
        "export_csv" | "export_to_csv" | "extract_data" => PROMPT_EXPORT_CSV,
        _ => PROMPT_EXPLAIN, // default fallback
    };

    template
        .replace("{extracted_text}", extracted_text)
        .replace("{platform}", platform)
        .replace("{detected_shell}", "zsh")
}
