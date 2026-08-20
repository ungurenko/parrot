#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryRuntime {
    MlxLm,
    MlxVlm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub repo: &'static str,
    pub expected_bytes: u64,
    pub runtime: SummaryRuntime,
    pub ready_marker: &'static str,
}

pub const DEFAULT_SUMMARY_MODEL: &str = "qwen3-4b";

pub const QWEN3_4B_SUMMARY: SummaryModelSpec = SummaryModelSpec {
    id: "qwen3-4b",
    label: "Qwen 3-4B Instruct",
    repo: "mlx-community/Qwen3-4B-Instruct-2507-4bit",
    expected_bytes: 2_262_920_192,
    runtime: SummaryRuntime::MlxLm,
    ready_marker: ".parrot-ready-summary-qwen3-4b",
};

pub const GEMMA4_E2B_SUMMARY: SummaryModelSpec = SummaryModelSpec {
    id: "gemma4-e2b",
    label: "Gemma 4 E2B-it",
    repo: "mlx-community/gemma-4-e2b-it-4bit",
    expected_bytes: 3_580_765_126,
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
