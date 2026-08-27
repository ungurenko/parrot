import { renderToStaticMarkup } from "react-dom/server";
import type { Update } from "@tauri-apps/plugin-updater";
import { describe, expect, it, vi } from "vitest";
import type { AutoUpdate } from "../hooks/useAutoUpdate";
import { UpdateBannerView } from "./UpdateBanner";

const RELEASE_BODY = [
  "## Что нового",
  "",
  "Транскрибация стала заметно быстрее.",
  "",
  "Длинные записи теперь обрабатываются стабильнее.",
].join("\n");

function makeUpdater(overrides: Partial<AutoUpdate> = {}): AutoUpdate {
  return {
    available: {
      version: "0.4.27",
      body: RELEASE_BODY,
    } as unknown as Update,
    status: "idle",
    progress: 0,
    errorDetails: null,
    errorScope: null,
    runCheck: vi.fn(),
    install: vi.fn(),
    ...overrides,
  };
}

function renderBanner(
  overrides: Partial<React.ComponentProps<typeof UpdateBannerView>> = {},
) {
  return renderToStaticMarkup(
    <UpdateBannerView
      updater={makeUpdater()}
      expanded={false}
      onToggleExpanded={vi.fn()}
      onDismiss={vi.fn()}
      onOpenSettings={vi.fn()}
      {...overrides}
    />,
  );
}

describe("UpdateBanner", () => {
  it("keeps the default banner compact", () => {
    const html = renderBanner();

    expect(html).toContain("Доступно обновление Parrot");
    expect(html).toContain("v0.4.27");
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("Транскрибация стала заметно быстрее.");
    expect(html).not.toContain("Длинные записи теперь обрабатываются стабильнее.");
  });

  it("reveals release notes and installation details on demand", () => {
    const html = renderBanner({ expanded: true });

    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain("Транскрибация стала заметно быстрее.");
    expect(html).toContain("Длинные записи теперь обрабатываются стабильнее.");
    expect(html).toContain("Меньше минуты · Перезапустится автоматически");
  });

  it("shows installation progress and prevents dismissal", () => {
    const html = renderBanner({
      updater: makeUpdater({ status: "installing", progress: 42 }),
    });

    expect(html).toContain("Обновляю… 42%");
    expect(html).toMatch(/aria-label="Скрыть подсказку"[^>]*disabled=""/);
  });

  it("offers retry and settings details after an install error", () => {
    const html = renderBanner({
      updater: makeUpdater({ status: "error", errorScope: "install" }),
    });

    expect(html).toContain("Не удалось установить обновление");
    expect(html).toContain("Подробнее — в Настройках");
    expect(html).toContain("Повторить");
  });
});
