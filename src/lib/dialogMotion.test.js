import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const appStyles = readFileSync(
  fileURLToPath(new URL("../index.css", import.meta.url)),
  "utf8",
);

describe("dialog motion", () => {
  it("keeps the dialog centered throughout its opening animation", () => {
    const keyframes = appStyles.match(
      /@keyframes motion-dialog-content-in\s*{([\s\S]*?)}\s*\.motion-dialog-overlay/,
    )?.[1];

    expect(keyframes).toBeDefined();
    expect(keyframes).not.toContain("transform:");
    expect(keyframes?.match(/scale:/g)).toHaveLength(2);
  });
});
