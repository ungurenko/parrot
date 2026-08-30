import { describe, expect, it } from "vitest";
import { createBrowserPreview } from "./browserPreview";

describe("createBrowserPreview", () => {
  it("stays inert when browser preview is disabled", () => {
    expect(
      createBrowserPreview("?preview=processing&theme=dark", false),
    ).toEqual({ jobs: [], processing: false });
  });

  it("parses and clamps a processing preview", () => {
    const preview = createBrowserPreview(
      "?preview=processing&stage=downloading&percent=140&source=file&queue=1&theme=dark",
      true,
    );

    expect(preview).toMatchObject({ processing: true, theme: "dark" });
    expect(preview.jobs).toHaveLength(2);
    expect(preview.jobs[0]).toMatchObject({
      sourceKind: "localFile",
      stage: "downloading",
      percent: 100,
    });
    expect(preview.jobs[1]).toMatchObject({
      status: "queued",
      stage: null,
      percent: 0,
    });
  });

  it("falls back to safe processing defaults", () => {
    const preview = createBrowserPreview(
      "?preview=processing&stage=unknown&percent=oops",
      true,
    );

    expect(preview.jobs[0]).toMatchObject({
      stage: "transcribing",
      percent: 42,
      status: "running",
    });
  });
});
