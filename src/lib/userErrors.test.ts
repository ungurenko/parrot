import { describe, expect, it } from "vitest";
import { formatErrorDescription, userErrorFrom } from "./userErrors";

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

  it("explains audio extraction failures without ffmpeg details", () => {
    const error = userErrorFrom("raw ffmpeg failure with many internal details");

    expect(error.title).toBe("Аудио не удалось прочитать");
    expect(error.message).toBe("Parrot не смог извлечь звук из файла или видео.");
    expect(error.technical).toContain("raw ffmpeg failure");
  });

  it("turns YouTube anti-bot and private video errors into clear advice", () => {
    const error = userErrorFrom(
      "yt-dlp failed with status Some(1): ERROR: [youtube] Sign in to confirm you're not a bot",
    );

    expect(error.title).toBe("YouTube не отдал видео");
    expect(error.message).toContain("ограничил скачивание");
    expect(error.action).toContain("скачайте видео вручную");
  });

  it("explains failed model downloads without leaking HTTP internals", () => {
    const description = formatErrorDescription(
      "HTTP GET failed: https://huggingface.co/model error sending request",
    );

    expect(description).toContain("Parrot не смог скачать модель");
    expect(description).toContain("интернет");
    expect(description).not.toContain("HTTP GET");
  });

  it("explains local summary environment install failures", () => {
    const error = userErrorFrom(
      "pip install mlx-lm/mlx-vlm завершился с ошибкой: No matching distribution found",
    );

    expect(error.title).toBe("Конспекты не подготовились");
    expect(error.action).toContain("нажмите «Установить окружение» ещё раз");
  });

  it("explains dictation access problems in normal macOS words", () => {
    const error = userErrorFrom(
      "Не удалось найти активное поле ввода (AX error -25205).",
    );

    expect(error.title).toBe("Диктовка не вставила текст");
    expect(error.message).toContain("поле для текста");
    expect(error.action).toContain("Универсальный доступ");
  });

  it("explains stale macOS accessibility grants for dictation", () => {
    const error = userErrorFrom(
      "macOS не разрешает Parrot вставлять текст автоматически. Разрешите Parrot в Системные настройки → Конфиденциальность и безопасность → Универсальный доступ.",
    );

    expect(error.title).toBe("Диктовка не вставила текст");
    expect(error.action).toContain("выключите");
    expect(error.action).toContain("добавьте заново");
  });

  it("explains unavailable Qwen ASR without setup scripts", () => {
    const description = formatErrorDescription(
      "Qwen MLX не установлен. Запустите tools/setup_qwen_mlx.sh или укажите PARROT_QWEN_BIN.",
    );

    expect(description).toContain("Режим качества сейчас недоступен");
    expect(description).toContain("Выберите быстрый режим");
    expect(description).not.toContain("tools/setup_qwen_mlx.sh");
  });

  it("explains invalid saved settings", () => {
    const error = userErrorFrom("Неизвестная модель распознавания: old-engine");

    expect(error.title).toBe("Настройку не удалось сохранить");
    expect(error.action).toContain("выберите значение из списка");
  });

  it("explains empty transcripts before summary", () => {
    const error = userErrorFrom("Пустой транскрипт.");

    expect(error.title).toBe("В тексте нечего конспектировать");
    expect(error.action).toContain("сначала сделайте транскрипцию");
  });

  it("explains missing history entries", () => {
    const error = userErrorFrom("Запись не найдена");

    expect(error.title).toBe("Запись недоступна");
    expect(error.message).toContain("истории");
  });

  it("treats lowercase cancellation as a user cancellation", () => {
    const error = userErrorFrom("cancelled");

    expect(error.title).toBe("Действие отменено");
  });
});
