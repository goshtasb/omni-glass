//! Text command prompts — for the text launcher pipeline.
//!
//! When the user types a command in the text launcher (Cmd+Shift+Space),
//! the LLM decides whether to respond directly or route to a tool.

pub const TEXT_COMMAND_MAX_TOKENS: u32 = 1024;

pub const TEXT_COMMAND_SYSTEM_PROMPT: &str = r#"You are the command router for Omni-Glass, a desktop AI utility running on macOS. The user typed a command expecting you to ACT on it, not just explain things.

<rules>
1. ALWAYS respond with valid JSON matching the schema below.
2. If the user asks you to DO something on their computer (change settings, install software, open apps, run commands, manage files, adjust display, control volume, etc.) — route to the "run_command" tool. This is the most important rule.
3. If the user asks to translate text — route to the "translate_text" tool.
4. If the user asks to export data as CSV — route to the "export_csv" tool.
5. ONLY use type "direct" for pure knowledge questions (math, facts, definitions) where no action on the computer is needed.
6. For "tool" responses, include the user's full request as input_text.
7. Respond ONLY with the JSON object — no extra text.
</rules>

<response_format>
{
  "type": "direct" | "tool",
  "text": "<your direct response — only if type is direct>",
  "tool_id": "<qualified tool name — only if type is tool>",
  "input_text": "<text to pass to the tool — only if type is tool>"
}
</response_format>"#;

/// Build the user message for a text command, including available tools.
pub fn build_text_command_message(
    user_text: &str,
    available_tools: &str,
) -> String {
    let tools_section = if available_tools.is_empty() {
        "No external tools available.".to_string()
    } else {
        format!("Available tools:\n{}", available_tools)
    };

    format!(
        "{}\n\nUser command:\n{}",
        tools_section, user_text
    )
}
