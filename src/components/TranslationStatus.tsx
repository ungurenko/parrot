import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { formatErrorDescription } from "@/lib/userErrors";
import type { TranslationState } from "../types";

interface Props {
  state?: TranslationState;
  onCancel: () => void;
}

export function TranslationStatus({ state, onCancel }: Props) {
  if (state?.status === "generating") {
    return (
      <div className="translation-progress" aria-live="polite">
        <div className="translation-progress-head">
          <span>
            {state.stage === "saving"
              ? "Сохраняю перевод…"
              : `Перевожу часть ${state.currentPart} из ${state.totalParts}…`}
          </span>
          {state.stage !== "saving" && (
            <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
              Отменить
            </Button>
          )}
        </div>
        <Progress value={Math.max(state.percent, 2)} />
      </div>
    );
  }

  if (state?.status === "error") {
    return (
      <Alert variant="destructive" className="translation-error">
        <AlertTitle>Не удалось перевести текст</AlertTitle>
        <AlertDescription className="whitespace-pre-wrap break-words">
          {formatErrorDescription(state.message)}
        </AlertDescription>
      </Alert>
    );
  }

  return null;
}
