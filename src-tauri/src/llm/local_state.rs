//! Local LLM state management — load, unload, and generate with llama.cpp.
//!
//! The model is loaded once when the user selects "local" provider, and held
//! in memory via `Arc<LlamaModel>`. Each generation creates a fresh context
//! (LlamaContext is !Send, so it lives entirely inside spawn_blocking).

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Global state for the local LLM, managed by Tauri.
pub struct LocalLlmState {
    backend: LlamaBackend,
    model: Mutex<Option<LoadedModel>>,
}

struct LoadedModel {
    inner: Arc<LlamaModel>,
    id: String,
}

// Safety: LlamaBackend is a ZST (zero-sized type) proof token — init'd once,
// never mutated, holds no data. LlamaModel is Send+Sync (declared by crate).
// Mutex guards all mutable access to the loaded model.
unsafe impl Send for LocalLlmState {}
unsafe impl Sync for LocalLlmState {}

/// Wrapper to send &LlamaBackend across thread boundaries.
/// Uses usize to avoid raw-pointer auto-trait issues in closures.
/// Safety: LlamaBackend is a ZST used only as an init proof token.
struct BackendPtr(usize);
unsafe impl Send for BackendPtr {}

impl BackendPtr {
    fn new(backend: &LlamaBackend) -> Self {
        Self(backend as *const LlamaBackend as usize)
    }
    /// Safety: The LlamaBackend must outlive the use of this pointer.
    unsafe fn get(&self) -> &LlamaBackend {
        &*(self.0 as *const LlamaBackend)
    }
}

impl LocalLlmState {
    pub fn new() -> Self {
        let backend = LlamaBackend::init().expect("Failed to init llama.cpp backend");
        Self {
            backend,
            model: Mutex::new(None),
        }
    }

    /// Load a GGUF model from disk. Replaces any previously loaded model.
    pub async fn load(&self, model_path: &Path, model_id: &str) -> Result<(), String> {
        let path = model_path.to_path_buf();
        let id = model_id.to_string();
        let bp = BackendPtr::new(&self.backend);

        let model = tokio::task::spawn_blocking(move || {
            // Safety: backend outlives this task (lives for app lifetime)
            let backend = unsafe { bp.get() };
            let params = LlamaModelParams::default();
            LlamaModel::load_from_file(backend, &path, &params)
                .map_err(|e| format!("Failed to load model: {:?}", e))
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

        let mut guard = self.model.lock().await;
        *guard = Some(LoadedModel {
            inner: Arc::new(model),
            id,
        });

        log::info!("[LOCAL_LLM] Model loaded: {}", model_id);
        Ok(())
    }

    /// Check if a model is currently loaded.
    pub async fn is_loaded(&self) -> bool {
        self.model.lock().await.is_some()
    }

    /// Get the ID of the currently loaded model.
    pub async fn loaded_model_id(&self) -> Option<String> {
        self.model.lock().await.as_ref().map(|m| m.id.clone())
    }

    /// Unload the current model, freeing GPU/RAM.
    pub async fn unload(&self) -> Result<(), String> {
        let mut guard = self.model.lock().await;
        if guard.is_some() {
            *guard = None;
            log::info!("[LOCAL_LLM] Model unloaded");
        }
        Ok(())
    }

    /// Generate text using the loaded model.
    ///
    /// Runs entirely inside `spawn_blocking` because LlamaContext is !Send.
    /// If `grammar` is Some, applies GBNF grammar-guided generation.
    pub async fn generate(
        &self,
        prompt: &str,
        max_tokens: u32,
        grammar: Option<&str>,
    ) -> Result<String, String> {
        let model_arc = {
            let guard = self.model.lock().await;
            guard
                .as_ref()
                .ok_or("No model loaded — download one in Settings")?
                .inner
                .clone()
        };

        let prompt = prompt.to_string();
        let grammar = grammar.map(|g| g.to_string());
        let bp = BackendPtr::new(&self.backend);

        tokio::task::spawn_blocking(move || {
            // Safety: backend outlives this task (lives for app lifetime)
            let backend = unsafe { bp.get() };
            generate_sync(backend, &model_arc, &prompt, max_tokens, grammar.as_deref())
        })
        .await
        .map_err(|e| format!("Generation task failed: {}", e))?
    }
}

/// Synchronous generation — runs inside spawn_blocking.
fn generate_sync(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &str,
    max_tokens: u32,
    grammar: Option<&str>,
) -> Result<String, String> {
    let start = std::time::Instant::now();

    // Create context with reasonable defaults for a 3B model
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(2048))
        .with_n_batch(512);

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| format!("Context creation failed: {:?}", e))?;

    // Tokenize prompt
    let tokens = model
        .str_to_token(prompt, llama_cpp_2::model::AddBos::Always)
        .map_err(|e| format!("Tokenization failed: {:?}", e))?;

    let prompt_len = tokens.len();
    log::info!("[LOCAL_LLM] Prompt: {} tokens", prompt_len);

    // Build initial batch with prompt tokens
    let mut batch = LlamaBatch::new(512, 1);
    for (pos, &token) in tokens.iter().enumerate() {
        let is_last = pos == tokens.len() - 1;
        batch
            .add(token, pos as i32, &[0], is_last)
            .map_err(|e| format!("Batch add failed: {:?}", e))?;
    }

    // Process prompt (prefill)
    ctx.decode(&mut batch)
        .map_err(|e| format!("Prompt decode failed: {:?}", e))?;

    let prefill_ms = start.elapsed().as_millis();
    log::info!("[LOCAL_LLM] Prefill: {}ms", prefill_ms);

    // Build sampler — with GBNF grammar if provided, otherwise plain sampling
    let mut sampler = if let Some(grammar_str) = grammar {
        LlamaSampler::chain_simple([
            LlamaSampler::grammar(model, grammar_str, "root")
                .map_err(|e| format!("Grammar compile failed: {:?}", e))?,
            LlamaSampler::temp(0.7),
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::greedy(),
        ])
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::greedy(),
        ])
    };

    // Generation loop
    let mut output = String::new();
    let mut n_decoded = 0u32;
    let eos = model.token_eos();
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..max_tokens {
        let token = sampler.sample(&ctx, -1);
        sampler.accept(token);

        if token == eos {
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, false, None)
            .unwrap_or_default();
        output.push_str(&piece);

        // Prepare next batch — position is prompt_len + n_decoded (0-indexed)
        batch.clear();
        batch
            .add(
                token,
                (prompt_len + n_decoded as usize) as i32,
                &[0],
                true,
            )
            .map_err(|e| format!("Batch add failed: {:?}", e))?;
        ctx.decode(&mut batch)
            .map_err(|e| format!("Decode failed: {:?}", e))?;
        n_decoded += 1;
    }

    let total_ms = start.elapsed().as_millis();
    let gen_ms = total_ms - prefill_ms;
    let tps = if gen_ms > 0 {
        (n_decoded as f64 / gen_ms as f64) * 1000.0
    } else {
        0.0
    };

    log::info!(
        "[LOCAL_LLM] Generated {} tokens in {}ms ({:.1} tok/s, prefill={}ms)",
        n_decoded,
        total_ms,
        tps,
        prefill_ms
    );

    Ok(output)
}
