# Добавление Gemma 4 E2B-it для конспектов Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить Gemma 4 E2B-it как выбираемую локальную модель для генерации конспектов, сохранив текущий Qwen 3-4B Instruct как стабильный вариант и дав пользователю понятный выбор в настройках.

**Architecture:** Вынести метаданные моделей конспекта в отдельный слой, добавить настройку `summary_model`, а текущий backend конспекта научить запускать выбранную модель. Qwen продолжает работать через `mlx-lm`; Gemma 4 E2B-it добавляется через проверенный MLX-путь с `mlx-vlm.server` для теплого OpenAI-совместимого сервера и отдельным fallback/probe перед включением в UI.

**Tech Stack:** Tauri 2, Rust, React 19, TypeScript, MLX, `mlx-lm`, `mlx-vlm`, Hugging Face cache через `HF_HOME`, локальный Python venv в Application Support.

## Global Constraints

- Не удалять `src-tauri/binaries/` и не менять sidecar-названия `ffmpeg-aarch64-apple-darwin` / `yt-dlp-aarch64-apple-darwin`.
- Все конспекты остаются offline-first: сеть нужна только для установки Python-окружения и первой загрузки модели.
- Не добавлять аналитику, телеметрию, cloud API или внешнюю отправку транскриптов.
- Целевое устройство для Gemma 4 E2B-it: MacBook M2 16 GB; модель должна запускаться без ручной установки системного Python.
- Текущий Qwen 3-4B Instruct должен остаться рабочим, чтобы у пользователя был стабильный fallback.
- Gemma repo: `mlx-community/gemma-4-e2b-it-4bit`; ожидаемый размер весов по HF index: `3_580_765_126` байт (~3.6 GB).
- Текущий Qwen repo: `mlx-community/Qwen3-4B-Instruct-2507-4bit`; ожидаемый размер весов по HF index: `2_262_920_192` байта (~2.3 GB).
- Для Gemma использовать instruction-вариант `-it`; base-модель `gemma-4-e2b` не подходит для пользовательских конспектов.
- Проверка после реализации: `npm run check` плюс живой smoke на генерацию конспекта для Qwen и Gemma.

---

## Source Evidence

- Google MLX integration: `mlx_lm.generate --model mlx-community/gemma-4-e2b-it-4bit --prompt ...`, `mlx_vlm.server --model mlx-community/gemma-4-e2b-it-4bit`, OpenAI-compatible endpoint at `/v1`.
- Gemma 4 E2B-it MLX card: converted from `google/gemma-4-e2b-it` using `mlx-vlm 0.4.3`.
- Gemma 4 E2B-it model index: `total_size = 3_580_765_126`.
- Qwen 3-4B Instruct model index: `total_size = 2_262_920_192`, converted with `mlx-lm 0.26.2`.
- Current code path: `src-tauri/src/summarizer_qwen3.rs` owns env setup, model readiness, warm server, generation, delete, and tests.

## File Structure

- Create `src-tauri/src/summarizer_models.rs`: Rust source of truth for summary model ids, labels, repo ids, expected bytes, runtime kind, ready marker names, and supported model validation.
- Modify `src-tauri/src/lib.rs`: register the new module, compare `summary_model` changes in `set_settings`, and keep preload/cleanup behavior correct.
- Modify `src-tauri/src/settings.rs`: add persisted `summary_model` with default `qwen3-4b`, normalization, validation, and tests.
- Modify `src-tauri/src/summarizer_qwen3.rs`: keep the existing file for now, but make it model-aware through `summarizer_models`; stop using global `SUMMARY_MODEL_REPO` as the only model.
- Modify `src-tauri/src/commands/summary.rs`: use selected model metadata for status, download progress, delete, and preload.
- Modify `src/types.ts`: add frontend `SummaryModel` type and model metadata.
- Modify `src/components/SettingsModal.tsx`: add a compact model selector inside the "Конспект" section.
- Modify `src/components/SummaryPanel.tsx` and `src/components/ResultView.tsx`: show the selected model label/size instead of hardcoded Qwen text.
- Create `docs/local-summary-models.md` and modify `docs/qwen-mlx.md`: document Qwen/Gemma local summary models, install path, cache, and test command.

### Task 1: Add Summary Model Metadata

**Files:**
- Create: `src-tauri/src/summarizer_models.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/summarizer_models.rs`

**Interfaces:**
- Produces: `SummaryModelId`, `SummaryRuntime`, `SummaryModelSpec`, `DEFAULT_SUMMARY_MODEL`, `SUPPORTED_SUMMARY_MODELS`, `summary_model_spec(id: &str) -> Option<&'static SummaryModelSpec>`, `normalize_summary_model(id: &str) -> &'static str`, `is_supported_summary_model(id: &str) -> bool`.
- Consumes: no earlier task.

- [ ] **Step 1: Create metadata file**

Add `src-tauri/src/summarizer_models.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryRuntime {
    MlxLm,
    MlxVlm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub short_label: &'static str,
    pub repo: &'static str,
    pub expected_bytes: u64,
    pub size_label: &'static str,
    pub runtime: SummaryRuntime,
    pub ready_marker: &'static str,
}

pub const DEFAULT_SUMMARY_MODEL: &str = "qwen3-4b";

pub const QWEN3_4B_SUMMARY: SummaryModelSpec = SummaryModelSpec {
    id: "qwen3-4b",
    label: "Qwen 3-4B Instruct",
    short_label: "Qwen",
    repo: "mlx-community/Qwen3-4B-Instruct-2507-4bit",
    expected_bytes: 2_262_920_192,
    size_label: "~2.3 ГБ",
    runtime: SummaryRuntime::MlxLm,
    ready_marker: ".parrot-ready-summary-qwen3-4b",
};

pub const GEMMA4_E2B_SUMMARY: SummaryModelSpec = SummaryModelSpec {
    id: "gemma4-e2b",
    label: "Gemma 4 E2B-it",
    short_label: "Gemma",
    repo: "mlx-community/gemma-4-e2b-it-4bit",
    expected_bytes: 3_580_765_126,
    size_label: "~3.6 ГБ",
    runtime: SummaryRuntime::MlxVlm,
    ready_marker: ".parrot-ready-summary-gemma4-e2b",
};

pub const SUPPORTED_SUMMARY_MODELS: [&SummaryModelSpec; 2] =
    [&QWEN3_4B_SUMMARY, &GEMMA4_E2B_SUMMARY];

pub fn summary_model_spec(id: &str) -> Option<&'static SummaryModelSpec> {
    SUPPORTED_SUMMARY_MODELS
        .iter()
        .copied()
        .find(|model| model.id == id)
}

pub fn normalize_summary_model(id: &str) -> &'static str {
    summary_model_spec(id)
        .map(|model| model.id)
        .unwrap_or(DEFAULT_SUMMARY_MODEL)
}

pub fn is_supported_summary_model(id: &str) -> bool {
    summary_model_spec(id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemma_model_metadata_should_match_hf_repo() {
        let spec = summary_model_spec("gemma4-e2b").expect("gemma spec");
        assert_eq!(spec.repo, "mlx-community/gemma-4-e2b-it-4bit");
        assert_eq!(spec.expected_bytes, 3_580_765_126);
        assert_eq!(spec.runtime, SummaryRuntime::MlxVlm);
    }

    #[test]
    fn unknown_summary_model_should_normalize_to_default() {
        assert_eq!(normalize_summary_model("bad-model"), DEFAULT_SUMMARY_MODEL);
        assert!(is_supported_summary_model("qwen3-4b"));
        assert!(is_supported_summary_model("gemma4-e2b"));
    }
}
```

- [ ] **Step 2: Register the module**

Modify `src-tauri/src/lib.rs` near the current summarizer module declaration:

```rust
mod summarizer_models;
mod summarizer_qwen3;
```

- [ ] **Step 3: Run targeted Rust test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml summarizer_models
```

Expected:

```text
test result: ok
```

- [ ] **Step 4: Commit**

Run:

```bash
git add src-tauri/src/lib.rs src-tauri/src/summarizer_models.rs
git commit -m "feat: add summary model metadata"
```

### Task 2: Persist Selected Summary Model

**Files:**
- Modify: `src-tauri/src/settings.rs`
- Modify: `src/types.ts`
- Test: `src-tauri/src/settings.rs`

**Interfaces:**
- Consumes: `summarizer_models::DEFAULT_SUMMARY_MODEL`, `normalize_summary_model`, `is_supported_summary_model`.
- Produces: Rust `Settings.summary_model: String`; TypeScript `Settings.summary_model: SummaryModel`.

- [ ] **Step 1: Add Rust setting field**

Modify imports in `src-tauri/src/settings.rs`:

```rust
use crate::{paths, summarizer_models};
```

Add the field to `Settings` after `summarizer_enabled`:

```rust
#[serde(default = "default_summary_model")]
pub summary_model: String,
```

Add default helper:

```rust
fn default_summary_model() -> String {
    summarizer_models::DEFAULT_SUMMARY_MODEL.to_string()
}
```

Add the default value in `impl Default for Settings`:

```rust
summary_model: default_summary_model(),
```

Normalize it in `Settings::normalized`:

```rust
self.summary_model = summarizer_models::normalize_summary_model(&self.summary_model).to_string();
```

Validate it in `validate_for_save`:

```rust
if !summarizer_models::is_supported_summary_model(&settings.summary_model) {
    anyhow::bail!("Неизвестная модель конспекта: {}", settings.summary_model);
}
```

- [ ] **Step 2: Update existing settings test fixture**

In `normalized_should_restore_unknown_engine_and_language`, set:

```rust
summary_model: "bad-summary-model".to_string(),
```

Add assertion:

```rust
assert_eq!(settings.summary_model, summarizer_models::DEFAULT_SUMMARY_MODEL);
```

- [ ] **Step 3: Add TypeScript summary model types**

Modify `src/types.ts`:

```ts
export type SummaryModel = "qwen3-4b" | "gemma4-e2b";

export const SUMMARY_MODEL_LABEL: Record<SummaryModel, string> = {
  "qwen3-4b": "Qwen 3-4B Instruct",
  "gemma4-e2b": "Gemma 4 E2B-it",
};

export const SUMMARY_MODEL_SIZE: Record<SummaryModel, string> = {
  "qwen3-4b": "~2.3 ГБ",
  "gemma4-e2b": "~3.6 ГБ",
};

export const SUMMARY_MODEL_BADGE: Record<SummaryModel, string> = {
  "qwen3-4b": "стабильная",
  "gemma4-e2b": "новая",
};
```

Replace the old constants:

```ts
export const SUMMARIZER_MODEL_LABEL = "Qwen 3-4B Instruct (конспект)";
export const SUMMARIZER_MODEL_SIZE = "~2.3 ГБ";
```

with compatibility helpers:

```ts
export const DEFAULT_SUMMARY_MODEL: SummaryModel = "qwen3-4b";
export const selectedSummaryModelLabel = (model: SummaryModel) => SUMMARY_MODEL_LABEL[model];
export const selectedSummaryModelSize = (model: SummaryModel) => SUMMARY_MODEL_SIZE[model];
```

Add `summary_model` to the frontend `Settings` interface:

```ts
summary_model: SummaryModel;
```

- [ ] **Step 4: Run checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml settings
npx tsc --noEmit
```

Expected:

```text
test result: ok
```

and `npx tsc --noEmit` exits with code `0`.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/settings.rs src/types.ts
git commit -m "feat: persist summary model selection"
```

### Task 3: Make Summarizer Backend Model-Aware

**Files:**
- Modify: `src-tauri/src/summarizer_qwen3.rs`
- Test: `src-tauri/src/summarizer_qwen3.rs`

**Interfaces:**
- Consumes: `settings::load(app).summary_model`, `summarizer_models::summary_model_spec`.
- Produces: model-aware `model_cache_exists`, `delete_model`, `warmup_model`, `generate_summary`, `preload_server`, and command builders.

- [ ] **Step 1: Replace single-model constants with selected spec**

Keep these generation constants unchanged:

```rust
const SUMMARY_MAX_TOKENS: u32 = 4096;
const SUMMARY_TEMP: f32 = 0.3;
const SUMMARY_TOP_P: f32 = 0.9;
```

Remove or stop using:

```rust
pub const SUMMARY_MODEL_REPO: &str = "mlx-community/Qwen3-4B-Instruct-2507-4bit";
pub const EXPECTED_SUMMARY_BYTES: u64 = 2_300_000_000;
const READY_MARKER: &str = ".parrot-ready-summary";
```

Add imports:

```rust
use crate::{prompts, settings, summarizer_models};
use summarizer_models::{SummaryModelSpec, SummaryRuntime};
```

Add helper:

```rust
fn selected_model(app: &AppHandle) -> &'static SummaryModelSpec {
    let settings = settings::load(app);
    summarizer_models::summary_model_spec(&settings.summary_model)
        .unwrap_or(&summarizer_models::QWEN3_4B_SUMMARY)
}
```

- [ ] **Step 2: Track model in the warm server**

Change `SummaryServer`:

```rust
struct SummaryServer {
    model_id: &'static str,
    runtime: SummaryRuntime,
    port: u16,
    child: Child,
}
```

Update the reuse check inside `ensure_summary_server`:

```rust
let spec = selected_model(app);
if let Some(server) = guard.as_mut() {
    if server.model_id == spec.id && server.child.try_wait()?.is_none() && health_ok(server.port) {
        return Ok((server.url(), server.pid(), true));
    }
}
```

When inserting:

```rust
*guard = Some(SummaryServer {
    model_id: spec.id,
    runtime: spec.runtime,
    port,
    child,
});
```

- [ ] **Step 3: Make cache checks per model**

Change model cache helpers:

```rust
pub fn model_cache_exists(app: &AppHandle) -> bool {
    let Ok(cache_dir) = paths::qwen_cache_dir(app) else {
        return false;
    };
    let spec = selected_model(app);
    if !ready_marker_exists_for(&cache_dir, spec) {
        return false;
    }
    model_cache_exists_in(&cache_dir, spec)
}

fn model_cache_exists_in(cache_dir: &Path, spec: &SummaryModelSpec) -> bool {
    let repo_cache_name = format!("models--{}", spec.repo.replace('/', "--"));
    let model_dir = cache_dir.join("hub").join(repo_cache_name);
    model_dir.exists() && dir_size_bytes(&model_dir) >= (spec.expected_bytes as f64 * 0.9) as u64
}
```

Add Qwen legacy marker support so existing users do not redownload Qwen:

```rust
fn ready_marker_exists_for(cache_dir: &Path, spec: &SummaryModelSpec) -> bool {
    if cache_dir.join(spec.ready_marker).is_file() {
        return true;
    }
    spec.id == "qwen3-4b" && cache_dir.join(".parrot-ready-summary").is_file()
}

fn write_ready_marker_for(cache_dir: &Path, spec: &SummaryModelSpec) -> Result<()> {
    std::fs::write(cache_dir.join(spec.ready_marker), b"ok")?;
    Ok(())
}

fn remove_ready_marker_for(cache_dir: &Path, spec: &SummaryModelSpec) {
    let _ = std::fs::remove_file(cache_dir.join(spec.ready_marker));
    if spec.id == "qwen3-4b" {
        let _ = std::fs::remove_file(cache_dir.join(".parrot-ready-summary"));
    }
}
```

- [ ] **Step 4: Make download/delete use selected model**

In `delete_model`:

```rust
let spec = selected_model(app);
remove_ready_marker_for(&cache_dir, spec);
let repo_cache_name = format!("models--{}", spec.repo.replace('/', "--"));
```

In `warmup_model`, use `spec` and write the model-specific marker:

```rust
let spec = selected_model(app);
run_summary_warmup(app, spec, cancel, "Не удалось подготовить модель конспекта")?;
let cache_dir = paths::qwen_cache_dir(app)?;
write_ready_marker_for(&cache_dir, spec)?;
```

- [ ] **Step 5: Add runtime-specific commands**

Replace `build_mlx_lm_generate_command` with a wrapper:

```rust
fn build_summary_generate_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    request: &MlxLmGenerateRequest<'_>,
) -> Command {
    match spec.runtime {
        SummaryRuntime::MlxLm => build_mlx_lm_generate_command(python, cache_dir, spec, request),
        SummaryRuntime::MlxVlm => build_mlx_vlm_generate_command(python, cache_dir, spec, request),
    }
}
```

Add Gemma fallback command for short warmup/probe:

```rust
fn build_mlx_vlm_generate_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    request: &MlxLmGenerateRequest<'_>,
) -> Command {
    let mut command = mlx_vlm_command(python, cache_dir);
    command
        .arg("generate")
        .arg("--model")
        .arg(spec.repo)
        .arg("--max-tokens")
        .arg(request.max_tokens.to_string())
        .arg("--temperature")
        .arg(request.temp.unwrap_or(SUMMARY_TEMP).to_string());
    if let Some(prompt_arg) = request.prompt_arg {
        command.arg("--prompt").arg(prompt_arg);
    } else if request.prompt_stdin.is_some() {
        command.arg("--prompt").arg("-");
        command.stdin(Stdio::piped());
    }
    command
}
```

Replace `build_mlx_lm_server_command` with:

```rust
fn build_summary_server_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    port: u16,
) -> Command {
    match spec.runtime {
        SummaryRuntime::MlxLm => build_mlx_lm_server_command(python, cache_dir, spec, port),
        SummaryRuntime::MlxVlm => build_mlx_vlm_server_command(python, cache_dir, spec, port),
    }
}
```

Add Gemma server command:

```rust
fn build_mlx_vlm_server_command(
    python: &Path,
    cache_dir: &Path,
    spec: &SummaryModelSpec,
    port: u16,
) -> Command {
    let mut command = mlx_vlm_command(python, cache_dir);
    command
        .arg("server")
        .arg("--model")
        .arg(spec.repo)
        .arg("--host")
        .arg(SERVER_HOST)
        .arg("--port")
        .arg(port.to_string())
        .arg("--log-level")
        .arg("ERROR")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
```

Add shared command builder:

```rust
fn mlx_vlm_command(python: &Path, cache_dir: &Path) -> Command {
    let mut cmd = Command::new(python);
    cmd.arg("-m").arg("mlx_vlm");
    cmd.env("HF_HOME", cache_dir)
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_XET", "1")
        .env("PYTHONUNBUFFERED", "1");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}
```

- [ ] **Step 6: Keep OpenAI streaming format**

Keep `post_to_summary_server_inner` payload unchanged:

```rust
POST /v1/chat/completions
messages: [{ role: "system" }, { role: "user" }]
stream: true
```

The Google MLX integration states that `mlx_vlm.server` exposes an OpenAI-compatible `/v1` endpoint, so this preserves the current streaming parser for both models.

- [ ] **Step 7: Update backend tests**

Replace `summary_server_command_uses_same_model_and_local_host` with two tests:

```rust
#[test]
fn qwen_summary_server_command_uses_mlx_lm() {
    let command = build_mlx_lm_server_command(
        Path::new("/tmp/python"),
        Path::new("/tmp/cache"),
        &summarizer_models::QWEN3_4B_SUMMARY,
        18181,
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(args.windows(2).any(|pair| pair == ["-m", "mlx_lm"]));
    assert!(args.iter().any(|arg| arg == "server"));
    assert!(args.windows(2).any(|pair| pair == ["--model", summarizer_models::QWEN3_4B_SUMMARY.repo]));
    assert!(args.windows(2).any(|pair| pair == ["--host", SERVER_HOST]));
    assert!(args.windows(2).any(|pair| pair == ["--port", "18181"]));
}

#[test]
fn gemma_summary_server_command_uses_mlx_vlm() {
    let command = build_mlx_vlm_server_command(
        Path::new("/tmp/python"),
        Path::new("/tmp/cache"),
        &summarizer_models::GEMMA4_E2B_SUMMARY,
        18182,
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(args.windows(2).any(|pair| pair == ["-m", "mlx_vlm"]));
    assert!(args.iter().any(|arg| arg == "server"));
    assert!(args.windows(2).any(|pair| pair == ["--model", summarizer_models::GEMMA4_E2B_SUMMARY.repo]));
    assert!(args.windows(2).any(|pair| pair == ["--host", SERVER_HOST]));
    assert!(args.windows(2).any(|pair| pair == ["--port", "18182"]));
}
```

- [ ] **Step 8: Run targeted Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml summarizer_qwen3
```

Expected:

```text
test result: ok
```

- [ ] **Step 9: Commit**

Run:

```bash
git add src-tauri/src/summarizer_qwen3.rs
git commit -m "feat: support multiple summary runtimes"
```

### Task 4: Install and Verify Gemma Runtime Dependencies

**Files:**
- Modify: `src-tauri/src/summarizer_qwen3.rs`
- Modify: `tools/setup_qwen_mlx.sh`
- Test: manual command output from local venv

**Interfaces:**
- Consumes: `SummaryRuntime::MlxVlm`.
- Produces: user-space venv with both `mlx-lm` and `mlx-vlm`.

- [ ] **Step 1: Update app env installer**

In `install_env`, replace the pip install args:

```rust
".args([
    "-m",
    "pip",
    "install",
    "--disable-pip-version-check",
    "mlx-lm>=0.26.2",
    "mlx-vlm>=0.4.3",
])
```

Update progress copy:

```rust
on_progress("Устанавливаю MLX для Qwen и Gemma…");
```

Update sanity check:

```rust
let out = Command::new(&venv_python)
    .args([
        "-c",
        "import mlx_lm, mlx_vlm; print('mlx_lm ok'); print('mlx_vlm ok')",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .context("Не удалось проверить mlx_lm/mlx_vlm")?;
```

Update error text:

```rust
"mlx_lm или mlx_vlm не импортируется после установки"
```

- [ ] **Step 2: Update dev setup script**

In `tools/setup_qwen_mlx.sh`, replace:

```bash
"$VENV_DIR/bin/python" -m pip install "mlx-lm>=0.24.0"
```

with:

```bash
"$VENV_DIR/bin/python" -m pip install "mlx-lm>=0.26.2" "mlx-vlm>=0.4.3"
```

- [ ] **Step 3: Verify local import**

Run:

```bash
.qwen-mlx/venv/bin/python -c "import mlx_lm, mlx_vlm; print('ok')"
```

Expected:

```text
ok
```

If `.qwen-mlx/venv/bin/python` is missing, run:

```bash
tools/setup_qwen_mlx.sh
```

Then repeat the import command.

- [ ] **Step 4: Commit**

Run:

```bash
git add src-tauri/src/summarizer_qwen3.rs tools/setup_qwen_mlx.sh
git commit -m "feat: install gemma summary runtime"
```

### Task 5: Wire Model-Aware Commands and Preload

**Files:**
- Modify: `src-tauri/src/commands/summary.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: Rust compile through `cargo test`

**Interfaces:**
- Consumes: selected `Settings.summary_model`, `summarizer_qwen3::expected_summary_bytes(app)`.
- Produces: correct download progress, delete, status, and preload for selected summary model.

- [ ] **Step 1: Add expected bytes helper**

In `src-tauri/src/summarizer_qwen3.rs`, add:

```rust
pub fn expected_summary_bytes(app: &AppHandle) -> u64 {
    selected_model(app).expected_bytes
}

pub fn selected_model_label(app: &AppHandle) -> &'static str {
    selected_model(app).label
}
```

- [ ] **Step 2: Use selected expected bytes in download command**

In `src-tauri/src/commands/summary.rs`, replace:

```rust
let expected_bytes = summarizer_qwen3::EXPECTED_SUMMARY_BYTES;
```

with:

```rust
let expected_bytes = summarizer_qwen3::expected_summary_bytes(&app);
```

- [ ] **Step 3: Stop/reload summary server when model changes**

In `src-tauri/src/lib.rs`, change `set_settings` after `settings::save`:

```rust
if old.summary_model != new.summary_model {
    summarizer_qwen3::stop_server();
}
if new.summarizer_enabled && (!old.summarizer_enabled || old.summary_model != new.summary_model) {
    summarizer_qwen3::preload_server(app.clone());
}
```

Keep the existing transcription engine preload block unchanged:

```rust
if old.engine != new.engine {
    preload_active_engine(app.clone());
}
```

- [ ] **Step 4: Run compile-level checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml summary
```

Expected:

```text
test result: ok
```

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/commands/summary.rs src-tauri/src/lib.rs src-tauri/src/summarizer_qwen3.rs
git commit -m "feat: preload selected summary model"
```

### Task 6: Add UI Model Selector

**Files:**
- Modify: `src/components/SettingsModal.tsx`
- Modify: `src/components/SummaryPanel.tsx`
- Modify: `src/components/ResultView.tsx`
- Modify: `src/types.ts`
- Test: `npx tsc --noEmit`

**Interfaces:**
- Consumes: `Settings.summary_model`, `SUMMARY_MODEL_LABEL`, `SUMMARY_MODEL_SIZE`, `SUMMARY_MODEL_BADGE`.
- Produces: a settings selector that lets the user choose Qwen or Gemma before downloading/generating summaries.

- [ ] **Step 1: Import summary model metadata**

In components that currently import `SUMMARIZER_MODEL_LABEL` / `SUMMARIZER_MODEL_SIZE`, replace imports with:

```ts
import {
  DEFAULT_SUMMARY_MODEL,
  SUMMARY_MODEL_BADGE,
  SUMMARY_MODEL_LABEL,
  SUMMARY_MODEL_SIZE,
  type SummaryModel,
} from "@/types";
```

- [ ] **Step 2: Normalize settings loaded from older installs**

Where settings state is initialized in `SettingsModal.tsx`, ensure older settings get Qwen:

```ts
const summaryModel = settings.summary_model ?? DEFAULT_SUMMARY_MODEL;
```

Add `summary_model` to `PREVIEW_SETTINGS`:

```ts
summary_model: DEFAULT_SUMMARY_MODEL,
```

When saving:

```ts
summary_model: summaryModel,
```

- [ ] **Step 3: Add selector inside the enabled summary block**

Add a helper near the existing `toggleSummarizer` function:

```ts
const changeSummaryModel = async (summary_model: SummaryModel) => {
  if (!settings || summary_model === settings.summary_model) return;
  const next = { ...settings, summary_model };
  if (previewMode) {
    setSettings(next);
    setSummarizerStatus({ available: true, modelReady: false });
    setSummaryProgress(0);
    return;
  }
  setSummaryBusy(true);
  setModelError(null);
  try {
    await invoke("set_settings", { new: next });
    setSettings(next);
    setSummarizerStatus(await invoke<SummarizerStatus>("get_summarizer_status"));
    setSummaryProgress(0);
  } catch (e: unknown) {
    setModelError(String(e));
  } finally {
    setSummaryBusy(false);
  }
};
```

In the "Конспект" settings area, render two compact selectable rows:

```tsx
<div className="grid gap-2 sm:grid-cols-2">
  {(["qwen3-4b", "gemma4-e2b"] as SummaryModel[]).map((model) => (
    <button
      key={model}
      type="button"
      className={cn(
        "engine-card min-w-0 text-left",
        summaryModel === model && "selected",
      )}
      onClick={() => changeSummaryModel(model)}
    >
      <span className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="font-medium leading-snug">{SUMMARY_MODEL_LABEL[model]}</span>
        <Badge variant="outline">{SUMMARY_MODEL_SIZE[model]}</Badge>
        <Badge variant={model === "gemma4-e2b" ? "secondary" : "outline"}>
          {SUMMARY_MODEL_BADGE[model]}
        </Badge>
      </span>
      <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">
        {model === "qwen3-4b"
          ? "Проверенная модель для стабильных конспектов."
          : "Свежая Gemma для сравнения качества русского текста."}
      </span>
    </button>
  ))}
</div>
```

- [ ] **Step 4: Update hardcoded copy**

Replace visible Qwen-only copy:

```tsx
Модель Qwen 3-4B Instruct (4-bit MLX), работает полностью оффлайн.
```

with:

```tsx
Выбранная модель конспекта работает полностью оффлайн после первой загрузки.
```

In `SummaryPanel.tsx`, compute selected labels from settings passed down or from a new prop:

```tsx
const summaryModel = settings?.summary_model ?? DEFAULT_SUMMARY_MODEL;
const summaryModelLabel = SUMMARY_MODEL_LABEL[summaryModel];
const summaryModelSize = SUMMARY_MODEL_SIZE[summaryModel];
```

Then use:

```tsx
Локальная модель {summaryModelLabel} ({summaryModelSize}, работает оффлайн)
```

- [ ] **Step 5: Update delete confirmation**

Replace:

```ts
`Удалить модель «${SUMMARIZER_MODEL_LABEL}»?\n\nКонспекты не сохранятся, но транскрипции останутся на месте. При необходимости модель можно скачать заново.`
```

with:

```ts
`Удалить модель «${SUMMARY_MODEL_LABEL[summaryModel]}»?\n\nКонспекты не сохранятся, но транскрипции останутся на месте. При необходимости модель можно скачать заново.`
```

- [ ] **Step 6: Run TypeScript check**

Run:

```bash
npx tsc --noEmit
```

Expected: command exits with code `0`.

- [ ] **Step 7: Commit**

Run:

```bash
git add src/types.ts src/components/SettingsModal.tsx src/components/SummaryPanel.tsx src/components/ResultView.tsx
git commit -m "feat: add summary model selector"
```

### Task 7: Add Probe and Benchmark Script

**Files:**
- Create: `tools/probe_summary_model.py`
- Test: manual run against Qwen and Gemma

**Interfaces:**
- Consumes: local `.qwen-mlx/venv/bin/python` or `PARROT_QWEN_PYTHON`.
- Produces: simple timing output for Qwen and Gemma on the same Russian transcript prompt.

- [ ] **Step 1: Create probe script**

Add `tools/probe_summary_model.py`:

```python
#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

MODELS = {
    "qwen3-4b": {
        "repo": "mlx-community/Qwen3-4B-Instruct-2507-4bit",
        "module": "mlx_lm",
    },
    "gemma4-e2b": {
        "repo": "mlx-community/gemma-4-e2b-it-4bit",
        "module": "mlx_lm",
    },
}

SYSTEM_PROMPT = "Ты создаешь краткий структурированный конспект на русском языке."

def python_bin() -> Path:
    env = os.environ.get("PARROT_QWEN_PYTHON")
    if env:
        return Path(env)
    return Path(".qwen-mlx/venv/bin/python")

def run_model(model_id: str, transcript: str) -> dict:
    spec = MODELS[model_id]
    prompt = (
        "Сделай краткий конспект в Markdown: краткое резюме, темы, тезисы, действия.\n\n"
        f"Транскрипт:\n---\n{transcript}\n---"
    )
    cmd = [
        str(python_bin()),
        "-m",
        spec["module"],
        "generate",
        "--model",
        spec["repo"],
        "--system-prompt",
        SYSTEM_PROMPT,
        "--prompt",
        prompt,
        "--max-tokens",
        "700",
        "--temp",
        "0.3",
        "--top-p",
        "0.9",
        "--verbose",
        "False",
    ]
    started = time.perf_counter()
    proc = subprocess.run(cmd, text=True, capture_output=True)
    elapsed = time.perf_counter() - started
    return {
        "model": model_id,
        "ok": proc.returncode == 0,
        "seconds": round(elapsed, 2),
        "stdout_chars": len(proc.stdout),
        "stderr_tail": "\n".join(proc.stderr.splitlines()[-10:]),
        "preview": proc.stdout.strip()[:700],
    }

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", choices=MODELS.keys(), required=True)
    parser.add_argument("--transcript", type=Path, required=True)
    args = parser.parse_args()

    transcript = args.transcript.read_text(encoding="utf-8")
    result = run_model(args.model, transcript)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ok"] else 1

if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Make it executable**

Run:

```bash
chmod +x tools/probe_summary_model.py
```

- [ ] **Step 3: Run a Qwen probe**

Create a short temporary transcript:

```bash
printf 'Сегодня обсудили запуск приложения, качество русских конспектов и проверку Gemma на MacBook M2. Решили сравнить скорость, память и читаемость результата.' > /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model qwen3-4b --transcript /tmp/parrot-summary-probe.txt
```

Expected JSON:

```json
{
  "model": "qwen3-4b",
  "ok": true
}
```

- [ ] **Step 4: Run a Gemma probe**

Run:

```bash
tools/probe_summary_model.py --model gemma4-e2b --transcript /tmp/parrot-summary-probe.txt
```

Expected JSON:

```json
{
  "model": "gemma4-e2b",
  "ok": true
}
```

If Gemma fails through `mlx_lm generate`, switch only the Gemma probe and backend fallback to `python -m mlx_vlm.generate` with the same prompt and keep `mlx_vlm.server` as the app server path.

- [ ] **Step 5: Commit**

Run:

```bash
git add tools/probe_summary_model.py
git commit -m "test: add summary model probe"
```

### Task 8: Document Local Summary Models

**Files:**
- Create: `docs/local-summary-models.md`
- Modify: `docs/qwen-mlx.md`
- Test: read-only doc review

**Interfaces:**
- Consumes: selected model ids, sizes, setup commands.
- Produces: user/developer documentation for Qwen and Gemma summary models.

- [ ] **Step 1: Create docs page**

Add `docs/local-summary-models.md`:

```markdown
# Локальные модели конспекта

Parrot создаёт конспекты локально. Транскрипт не отправляется в облако.

## Модели

| Модель | ID в настройках | HF repo | Размер | Роль |
| --- | --- | --- | --- | --- |
| Qwen 3-4B Instruct | `qwen3-4b` | `mlx-community/Qwen3-4B-Instruct-2507-4bit` | ~2.3 ГБ | стабильная модель |
| Gemma 4 E2B-it | `gemma4-e2b` | `mlx-community/gemma-4-e2b-it-4bit` | ~3.6 ГБ | новая модель для сравнения русского текста |

## Окружение

Приложение само ставит локальный Python и MLX-пакеты в Application Support.
Для разработки можно подготовить окружение командой:

```bash
tools/setup_qwen_mlx.sh
```

## Проверка

```bash
printf 'Короткий русский текст для проверки конспекта.' > /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model qwen3-4b --transcript /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model gemma4-e2b --transcript /tmp/parrot-summary-probe.txt
```

## Правило выбора по умолчанию

Qwen остаётся дефолтом до живого сравнения на реальных русских расшифровках.
Gemma можно сделать дефолтом после проверки скорости, памяти и качества конспекта на MacBook M2 16 GB.
```

- [ ] **Step 2: Link from Qwen doc**

At the top of `docs/qwen-mlx.md`, add:

```markdown
> Для моделей конспекта см. `docs/local-summary-models.md`.
```

- [ ] **Step 3: Commit**

Run:

```bash
git add docs/local-summary-models.md docs/qwen-mlx.md
git commit -m "docs: describe local summary models"
```

### Task 9: Full Verification and Release Readiness

**Files:**
- No new files.
- Test: project checks, dev app smoke, model probes.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: verified implementation ready for a release decision.

- [ ] **Step 1: Run full project gate**

Run:

```bash
npm run check
```

Expected: frontend build, Rust tests, fmt, and clippy complete successfully.

- [ ] **Step 2: Run both probes**

Run:

```bash
printf 'Сегодня обсудили запуск приложения, качество русских конспектов и проверку Gemma на MacBook M2. Решили сравнить скорость, память и читаемость результата.' > /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model qwen3-4b --transcript /tmp/parrot-summary-probe.txt
tools/probe_summary_model.py --model gemma4-e2b --transcript /tmp/parrot-summary-probe.txt
```

Expected: both commands print `"ok": true`.

- [ ] **Step 3: Run dev app smoke**

Run:

```bash
npm run tauri dev
```

Manual checks:

- Settings -> "Конспект" shows Qwen and Gemma.
- Qwen can be selected and still shows already downloaded if the old Qwen marker/cache exists.
- Gemma can be selected, downloaded, and warmed.
- A short transcript can generate a Gemma summary.
- Switching from Gemma back to Qwen stops the old summary server and starts the selected model on next use.
- Delete model removes only the selected summary model.

- [ ] **Step 4: Inspect diff**

Run:

```bash
git status --short
git diff --stat
```

Expected changed areas:

- `src-tauri/src/summarizer_models.rs`
- `src-tauri/src/summarizer_qwen3.rs`
- `src-tauri/src/settings.rs`
- `src-tauri/src/commands/summary.rs`
- `src-tauri/src/lib.rs`
- `src/types.ts`
- summary UI components
- docs/tools from this plan

- [ ] **Step 5: Final commit**

If all checks pass and there are unstaged changes from the last task:

```bash
git add src-tauri/src src/types.ts src/components docs tools
git commit -m "feat: add gemma summary model"
```

## Rollback Plan

- If Gemma runtime fails on the target Mac, keep the model selector hidden behind a constant `const GEMMA_SUMMARY_ENABLED: bool = false` in `summarizer_models.rs` and ship only backend-safe Qwen changes.
- If `mlx_vlm.server` streams a different SSE shape, keep Gemma generation as one-shot `mlx_lm generate` for the first release and postpone warm server support.
- If memory use is too high on M2 16 GB, keep Gemma marked as experimental in UI and keep Qwen default.
- If existing Qwen users lose readiness state, restore compatibility by accepting `.parrot-ready-summary` for Qwen and writing `.parrot-ready-summary-qwen3-4b` on the next successful warmup.

## Self-Review

- Spec coverage: plan adds Gemma 4 E2B-it, keeps Qwen, adds model selection, runtime install, model download/delete/status, UI copy, docs, probes, and full checks.
- Red-flag scan: no empty future-work step, no undefined file path, no missing model id.
- Type consistency: Rust setting is `summary_model`; TS setting is `summary_model`; model ids are exactly `qwen3-4b` and `gemma4-e2b`; Gemma repo is exactly `mlx-community/gemma-4-e2b-it-4bit`.
