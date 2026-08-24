import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { EngineStatuses } from "../types";
import { EnginePicker } from "./EnginePicker";

const statuses: EngineStatuses = {
  parakeet: { available: true, modelReady: true },
  whisper: { available: true, modelReady: true },
  "qwen-0.6b": { available: true, modelReady: false },
  "qwen-1.7b": {
    available: false,
    modelReady: false,
    unavailableReason: "Нужна macOS 14 или новее.",
  },
};

function renderPicker(
  overrides: Partial<React.ComponentProps<typeof EnginePicker>> = {},
) {
  return renderToStaticMarkup(
    <EnginePicker
      value="parakeet"
      statuses={statuses}
      onChange={vi.fn()}
      onPrepare={vi.fn()}
      onDelete={vi.fn()}
      {...overrides}
    />,
  );
}

describe("EnginePicker", () => {
  it("renders the four models in the intended user-facing order", () => {
    const html = renderPicker();
    const positions = [
      "Parakeet V3",
      "Whisper Large-v3 turbo",
      "Qwen3-ASR 0.6B MLX",
      "Qwen3-ASR 1.7B MLX",
    ].map((name) => html.indexOf(name));

    expect(positions.every((position) => position >= 0)).toBe(true);
    expect(positions).toEqual([...positions].sort((a, b) => a - b));
  });

  it("shows explicit actions for selected, ready, downloadable and unavailable models", () => {
    const html = renderPicker();

    expect(html).toContain(">Выбрана</button>");
    expect(html).toContain(">Выбрать</button>");
    expect(html).toContain(">Скачать и выбрать</button>");
    expect(html).toContain(">Недоступна</button>");
    expect(html).toContain("Нужна macOS 14 или новее.");
  });

  it("shows installation progress and locks model actions while preparation is running", () => {
    const html = renderPicker({
      busyEngine: "qwen-0.6b",
      stage: "installing",
      progress: 7,
    });

    expect(html).toContain("Устанавливаю…");
    expect(html).toContain("Подготавливаю окружение для модели");
    expect(html.match(/disabled=""/g)?.length).toBeGreaterThanOrEqual(4);
  });

  it("restores a retry action after a failed download and blocks preparation during an active job", () => {
    const retryHtml = renderPicker({ failedEngine: "qwen-0.6b" });
    const lockedHtml = renderPicker({ hasActiveJob: true });

    expect(retryHtml).toContain(">Повторить</button>");
    expect(lockedHtml).toMatch(
      /<button[^>]*disabled=""[^>]*aria-label="Скачать и выбрать[^>]*>/,
    );
  });
});
