//! Integration tests for the local LLM feature.
//!
//! Tests model registry, model manager paths, prompt formatting,
//! and GBNF grammar structure. Only compiled with `--features local-llm`.

#![cfg(feature = "local-llm")]

use omni_glass_lib::llm::model_manager;
use omni_glass_lib::llm::model_registry;
use omni_glass_lib::llm::prompts_execute_local;
use omni_glass_lib::llm::prompts_local;

// ── Model Registry ──────────────────────────────────────────────────

#[test]
fn registry_has_at_least_two_models() {
    let models = model_registry::available_models();
    assert!(models.len() >= 2, "Expected at least 2 models");
}

#[test]
fn default_model_is_3b() {
    let m = model_registry::default_model();
    assert_eq!(m.id, "qwen2.5-3b-q4km");
    assert!(m.size_bytes > 1_000_000_000, "3B model should be >1 GB");
}

#[test]
fn find_model_returns_correct_info() {
    let m = model_registry::find_model("qwen2.5-1.5b-q4km");
    assert!(m.is_some(), "Should find 1.5B model");
    let m = m.unwrap();
    assert_eq!(m.name, "Qwen 2.5 1.5B (Faster)");
    assert!(m.ram_required_gb < 4.0);
}

#[test]
fn find_model_returns_none_for_unknown() {
    assert!(model_registry::find_model("nonexistent").is_none());
}

#[test]
fn all_model_urls_are_huggingface_gguf() {
    for m in model_registry::available_models() {
        assert!(m.url.starts_with("https://huggingface.co/"), "URL should be HuggingFace: {}", m.url);
        assert!(m.url.ends_with(".gguf"), "URL should end with .gguf: {}", m.url);
    }
}

// ── Model Manager Paths ─────────────────────────────────────────────

#[test]
fn models_dir_is_under_omni_glass() {
    let dir = model_manager::models_dir();
    let s = dir.to_string_lossy();
    assert!(s.contains("omni-glass"), "Dir should contain 'omni-glass': {}", s);
    assert!(s.ends_with("models"), "Dir should end with 'models': {}", s);
}

#[test]
fn model_path_includes_gguf_filename() {
    let m = model_registry::default_model();
    let path = model_manager::model_path(m);
    let s = path.to_string_lossy();
    assert!(s.ends_with(".gguf"), "Path should end with .gguf: {}", s);
    assert!(s.contains("omni-glass"), "Path should be under omni-glass: {}", s);
}

// ── ChatML Prompt Format ─────────────────────────────────────────────

#[test]
fn classify_prompt_uses_chatml_format() {
    let prompt = prompts_local::build_local_classify_prompt(
        "Hello world",
        0.95,
        false,
        false,
        "",
    );
    assert!(prompt.starts_with("<|im_start|>system"), "Should start with ChatML system tag");
    assert!(prompt.contains("<|im_end|>"), "Should contain ChatML end tag");
    assert!(prompt.contains("<|im_start|>user"), "Should contain ChatML user tag");
    assert!(prompt.ends_with("<|im_start|>assistant\n"), "Should end with assistant prompt");
}

#[test]
fn classify_prompt_includes_text() {
    let prompt = prompts_local::build_local_classify_prompt(
        "error: undefined variable 'foo'",
        0.85,
        false,
        true,
        "",
    );
    assert!(prompt.contains("error: undefined variable 'foo'"));
    assert!(prompt.contains("0.85"));
    assert!(prompt.contains("Has code structure: true"));
}

#[test]
fn classify_prompt_includes_plugin_tools() {
    let tools = "- Calculator (calc::compute): Does math\n- Translator (translate::run): Translates text";
    let prompt = prompts_local::build_local_classify_prompt(
        "test text",
        0.9,
        false,
        false,
        tools,
    );
    assert!(prompt.contains("Calculator"), "Should include plugin tool");
    assert!(prompt.contains("Translator"), "Should include plugin tool");
}

#[test]
fn execute_prompt_uses_chatml_format() {
    let prompt = prompts_execute_local::build_local_execute_prompt(
        "explain",
        "Hello world",
        "macos",
    );
    assert!(prompt.starts_with("<|im_start|>system"));
    assert!(prompt.contains("<|im_start|>user"));
    assert!(prompt.ends_with("<|im_start|>assistant\n"));
}

#[test]
fn execute_prompt_includes_action_and_text() {
    let prompt = prompts_execute_local::build_local_execute_prompt(
        "suggest_fix",
        "let x = ;",
        "macos",
    );
    assert!(prompt.contains("suggest_fix"));
    assert!(prompt.contains("let x = ;"));
}

#[test]
fn text_command_prompt_includes_tools() {
    let prompt = prompts_execute_local::build_local_text_command_prompt(
        "What is 2+2?",
        "- Calculator: Does math",
    );
    assert!(prompt.starts_with("<|im_start|>system"));
    assert!(prompt.contains("What is 2+2?"));
    assert!(prompt.contains("Calculator"));
}

#[test]
fn args_prompt_includes_schema() {
    let prompt = prompts_execute_local::build_local_args_prompt(
        "translate",
        "Translates text between languages",
        r#"{"type":"object","properties":{"text":{"type":"string"},"target_lang":{"type":"string"}}}"#,
        "Hello world",
    );
    assert!(prompt.contains("translate"));
    assert!(prompt.contains("target_lang"));
    assert!(prompt.contains("Hello world"));
}

// ── GBNF Grammar Structure ──────────────────────────────────────────

#[test]
fn action_menu_grammar_has_root_rule() {
    let g = prompts_local::ACTION_MENU_GRAMMAR;
    assert!(g.contains("root"), "Grammar should define root rule");
    assert!(g.contains("contentType") || g.contains("content_type"),
        "Grammar should reference content type field");
}

#[test]
fn action_result_grammar_has_root_rule() {
    let g = prompts_execute_local::ACTION_RESULT_GRAMMAR;
    assert!(g.contains("root"), "Grammar should define root rule");
    assert!(g.contains("status"), "Grammar should reference status field");
}

#[test]
fn route_decision_grammar_has_root_rule() {
    let g = prompts_execute_local::ROUTE_DECISION_GRAMMAR;
    assert!(g.contains("root"), "Grammar should define root rule");
}

#[test]
fn grammars_use_safe_string_rule() {
    // The wildcard `\\ .` pattern crashes llama.cpp. All grammars must use
    // explicit escape chars: `["\\/bfnrt]` instead of `.` (wildcard).
    let grammars = [
        ("ACTION_MENU", prompts_local::ACTION_MENU_GRAMMAR),
        ("ACTION_RESULT", prompts_execute_local::ACTION_RESULT_GRAMMAR),
        ("ROUTE_DECISION", prompts_execute_local::ROUTE_DECISION_GRAMMAR),
    ];
    for (name, g) in &grammars {
        assert!(!g.contains(r#""\\" .)"#),
            "{} grammar still uses wildcard `.` in string rule — will crash llama.cpp", name);
        assert!(g.contains("bfnrt"),
            "{} grammar should use explicit escape chars [bfnrt]", name);
    }
}

// ── Provider Config ─────────────────────────────────────────────────

#[test]
fn provider_list_includes_local() {
    let providers = omni_glass_lib::llm::provider::all_providers();
    let local = providers.iter().find(|p| p.id == "local");
    assert!(local.is_some(), "Provider list should include 'local'");
    let local = local.unwrap();
    assert_eq!(local.cost_per_snip, "Free");
    assert!(local.env_key.is_empty(), "Local provider needs no API key");
}
