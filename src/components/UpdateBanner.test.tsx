import { renderToStaticMarkup } from "react-dom/server";
import type { Update } from "@tauri-apps/plugin-updater";
import { describe, expect, it, vi } from "vitest";
import type { AutoUpdate } from "../hooks/useAutoUpdate";
import { UpdateBannerView, UpdateNotesContent } from "./UpdateBanner";

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
      notesOpen={false}
      onNotesOpenChange={vi.fn()}
      onDismiss={vi.fn()}
      onOpenSettings={vi.fn()}
      {...overrides}
    />,
  );
}

describe("UpdateBanner", () => {
  it("keeps the default banner compact", () => {
    const html = renderBanner();

    expect(html).toContain("Доступна новая версия");
    expect(html).toContain("v0.4.27");
    expect(html).toContain("Транскрибация стала заметно быстрее.");
    expect(html).toContain("Что нового");
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("update-banner-disclosure");
  });

  it("exposes the details dialog without expanding the banner", () => {
    const html = renderBanner({ notesOpen: true });

    expect(html).toContain('aria-expanded="true"');
    expect(html).not.toContain("update-banner-disclosure");
  });

  it("renders three concise highlights in the details content", () => {
    const html = renderToStaticMarkup(
      <UpdateNotesContent
        highlights={[
          "Первое изменение.",
          "Второе изменение.",
          "Третье изменение.",
        ]}
        onInstall={vi.fn()}
      />,
    );

    expect(html).toContain("Первое изменение.");
    expect(html).toContain("Второе изменение.");
    expect(html).toContain("Третье изменение.");
    expect(html).toContain("Меньше минуты · Parrot перезапустится сам");
  });

  it("shows installation progress and prevents dismissal", () => {
    const html = renderBanner({
      updater: makeUpdater({ status: "installing", progress: 42 }),
    });

    expect(html).toContain("Обновляю Parrot");
    expect(html).toContain("Обновляю… 42%");
    expect(html).toMatch(/aria-label="Скрыть подсказку"[^>]*disabled=""/);
  });

  it("offers retry and settings details after an install error", () => {
    const html = renderBanner({
      updater: makeUpdater({ status: "error", errorScope: "install" }),
    });

    expect(html).toContain("Обновление не установлено");
    expect(html).toContain("Подробнее");
    expect(html).toContain("Повторить");
  });
});
