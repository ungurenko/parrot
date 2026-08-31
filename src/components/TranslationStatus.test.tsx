import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { TranslationStatus } from "./TranslationStatus";

describe("TranslationStatus", () => {
  it("shows the current part and cancellation while translating", () => {
    const html = renderToStaticMarkup(
      <TranslationStatus
        state={{
          status: "generating",
          stage: "translating",
          percent: 45,
          currentPart: 2,
          totalParts: 4,
        }}
        onCancel={vi.fn()}
      />,
    );

    expect(html).toContain("Перевожу часть 2 из 4");
    expect(html).toContain("Отменить");
    expect(html).toContain('aria-valuenow="45"');
  });
});
