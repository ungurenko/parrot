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
    <div className="space-y-2">
      {OPTIONS.map((opt) => {
        const active = value === opt.id;
        return (
          <button
            key={opt.id}
            type="button"
            onClick={() => onChange(opt.id)}
            className={`w-full text-left px-3 py-3 rounded-md border transition-colors ${
              active
                ? "border-[var(--color-accent)] bg-[var(--color-accent)]/10"
                : "border-[var(--color-border)] bg-[var(--color-panel)] hover:bg-[var(--color-panel-hover)]"
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="font-medium">{opt.title}</span>
              {opt.badge && (
                <span className="text-xs px-1.5 py-0.5 rounded bg-[var(--color-accent)] text-white">
                  {opt.badge}
                </span>
              )}
              {active && <span className="ml-auto text-[var(--color-accent)]">✓</span>}
            </div>
            <div className="text-xs text-[var(--color-muted)] mt-1">{opt.hint}</div>
          </button>
        );
      })}
    </div>
  );
}
