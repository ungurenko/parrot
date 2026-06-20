import { describe, expect, it } from "vitest";
import { userErrorFrom } from "./userErrors";

describe("userErrors", () => {
  it("explains unsupported files with a next action", () => {
    expect(userErrorFrom("Неподдерживаемый формат файла")).toEqual({
      title: "Файл не подходит",
      message: "Parrot понимает аудио и видео: MP3, M4A, WAV, MP4, MOV и похожие форматы.",
      action: "Выберите другой файл или конвертируйте запись в MP3.",
    });
  });

  it("explains YouTube URL problems", () => {
    const error = userErrorFrom("URL не похож на YouTube-ссылку");

    expect(error.title).toBe("Ссылка не похожа на YouTube");
    expect(error.action).toContain("youtube.com");
  });

  it("keeps unknown errors short but useful", () => {
    const error = userErrorFrom("raw ffmpeg failure with many internal details");

    expect(error.title).toBe("Что-то пошло не так");
    expect(error.message).toBe("Parrot не смог завершить действие.");
    expect(error.technical).toContain("raw ffmpeg failure");
  });
});
