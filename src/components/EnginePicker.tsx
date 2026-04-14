import { Badge } from "@/components/ui/badge";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import type { Engine } from "../types";

interface Props {
  value: Engine;
  onChange: (engine: Engine) => void;
}

const OPTIONS: Array<{
  id: Engine;
  title: string;
  hint: string;
  badge?: string;
}> = [
  {
    id: "qwen-0.6b",
    title: "Qwen3-ASR 0.6B MLX",
    hint: "Лучший баланс скорости и качества. ~2.7 ГБ RAM, подходит для Mac с 16 ГБ.",
    badge: "рекомендуем",
  },
  {
    id: "qwen-1.7b",
    title: "Qwen3-ASR 1.7B MLX",
    hint: "Максимальное качество на шумном/акцентном аудио. Нужен Mac с 32+ ГБ RAM для длинных файлов.",
    badge: "качество",
  },
  {
    id: "parakeet",
    title: "Parakeet V3",
    hint: "Максимальная скорость. 25 языков включая русский. Без галлюцинаций.",
  },
  {
    id: "whisper",
    title: "Whisper Large-v3 turbo",
    hint: "Медленнее, но 100+ языков (включая азиатские и арабский).",
  },
];

export function EnginePicker({ value, onChange }: Props) {
  return (
    <ToggleGroup
      type="single"
      value={value}
      onValueChange={(next) => {
        if (next) onChange(next as Engine);
      }}
      orientation="vertical"
      spacing={2}
      className="w-full min-w-0 flex-col items-stretch gap-2 overflow-x-hidden"
    >
      {OPTIONS.map((opt) => {
        const active = value === opt.id;
        return (
          <ToggleGroupItem
            key={opt.id}
            value={opt.id}
            variant="outline"
            size="lg"
            aria-label={opt.title}
            className={cn(
              "h-auto w-full min-w-0 items-start justify-start whitespace-normal border-transparent bg-muted/25 px-3 py-3 text-left hover:bg-background",
              active && "border-border bg-background",
            )}
          >
            <div className="grid w-full min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-2">
              <div className="min-w-0">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="font-medium leading-snug">{opt.title}</span>
                  {opt.badge && <Badge variant="secondary">{opt.badge}</Badge>}
                </div>
                <div className="mt-1 whitespace-normal break-words text-xs leading-relaxed text-muted-foreground">
                  {opt.hint}
                </div>
              </div>
              <div className="flex justify-end">
                {active && (
                  <span className="text-primary" aria-hidden="true">
                    ✓
                  </span>
                )}
              </div>
            </div>
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}
