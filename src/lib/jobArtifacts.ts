import type {
  GeneratedArtifact,
  Job,
  SummaryState,
  TranslationState,
} from "../types";

export function summaryResult(
  state: SummaryState | undefined,
): GeneratedArtifact | undefined {
  return state?.status === "done" ? state.result : undefined;
}

export function translationResult(
  state: TranslationState | undefined,
): GeneratedArtifact | undefined {
  if (state?.status === "done") return state.result;
  if (state?.status === "generating" || state?.status === "error") {
    return state.previous;
  }
  return undefined;
}

export function localModelBusy(job: Job): boolean {
  return (
    job.summary?.status === "generating" ||
    job.translation?.status === "generating"
  );
}
