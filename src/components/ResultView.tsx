import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Textarea } from "@/components/ui/textarea";
import type { Job } from "../types";

interface Props {
  job: Job | null;
}

export function ResultView({ job }: Props) {
  if (!job) {
    return (
      <Empty className="h-full border bg-muted/20">
        <EmptyHeader>
          <EmptyMedia className="text-2xl">📄</EmptyMedia>
          <EmptyTitle>Результат появится здесь</EmptyTitle>
          <EmptyDescription>
            Выберите готовую задачу, чтобы увидеть текст.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  if (job.status === "error") {
    return (
      <Alert variant="destructive">
        <AlertTitle>Ошибка</AlertTitle>
        <AlertDescription className="whitespace-pre-wrap">
          {job.error}
        </AlertDescription>
      </Alert>
    );
  }

  if (job.status === "canceled") {
    return (
      <Empty className="h-full border bg-muted/20">
        <EmptyHeader>
          <EmptyMedia className="text-2xl">🚫</EmptyMedia>
          <EmptyTitle>Задача отменена</EmptyTitle>
          <EmptyDescription>
            Можно добавить файл заново, если расшифровка всё ещё нужна.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  if (job.status !== "done" || job.text === undefined) {
    return (
      <Empty className="h-full border bg-muted/20">
        <EmptyHeader>
          <EmptyMedia className="text-2xl">⏳</EmptyMedia>
          <EmptyTitle>Задача ещё не завершена</EmptyTitle>
          <EmptyDescription>
            Текст появится здесь, когда расшифровка будет готова.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  const copyText = async () => {
    try {
      await navigator.clipboard.writeText(job.text ?? "");
      toast.success("Текст скопирован");
    } catch (e) {
      toast.error("Не удалось скопировать текст", {
        description: String(e),
      });
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="mb-2 flex shrink-0 items-center justify-between gap-3">
        <div className="min-w-0 truncate text-xs text-muted-foreground">
          {job.outputPath}
        </div>
        <div className="flex shrink-0 gap-2">
          <Button variant="outline" size="sm" onClick={copyText}>
            <span aria-hidden="true">📋</span>
            Копировать
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => invoke("open_in_finder", { path: job.outputPath })}
          >
            <span aria-hidden="true">📂</span>
            В Finder
          </Button>
        </div>
      </div>
      <Textarea
        readOnly
        value={job.text}
        className="min-h-0 flex-1 resize-none bg-background font-mono text-sm shadow-inner"
      />
    </div>
  );
}
