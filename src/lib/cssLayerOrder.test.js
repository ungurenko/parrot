import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const processingStyles = readFileSync(
  fileURLToPath(new URL("../components/ProcessingView.css", import.meta.url)),
  "utf8",
);

describe("CSS layer order", () => {
  it("keeps the base reset below standalone component styles", () => {
    expect(processingStyles.trimStart()).toMatch(
      /^@layer properties, theme, base, components, utilities;/,
    );
  });
});
