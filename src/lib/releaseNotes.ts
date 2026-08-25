export interface ParsedReleaseNotes {
  /** One-sentence teaser shown right in the update banner. */
  summary: string;
  /** Full "what's new" paragraphs revealed behind the toggle. */
  details: string[];
}

// Sections with installation instructions are noise inside the banner.
const INSTALL_SECTION = /как получить|как установить|установит/i;
const SENTENCE_BOUNDARY = /(?<=[.!?…])\s/;
const SUMMARY_MAX_LEN = 140;

function truncate(text: string): string {
  if (text.length <= SUMMARY_MAX_LEN) return text;
  const space = text.lastIndexOf(" ", SUMMARY_MAX_LEN);
  const cutAt = space > 0 ? space : SUMMARY_MAX_LEN;
  return `${text.slice(0, cutAt).trimEnd()}…`;
}

interface Section {
  heading: string;
  lines: string[];
}

export function parseReleaseNotes(
  body: string | undefined | null,
): ParsedReleaseNotes {
  if (!body?.trim()) return { summary: "", details: [] };

  let preamble: string[] = [];
  let current: Section | null = null;
  const sections: Section[] = [];

  for (const line of body.split("\n")) {
    const match = line.match(/^##\s*(.*)$/);
    if (match) {
      current = { heading: match[1], lines: [] };
      sections.push(current);
    } else if (current) {
      current.lines.push(line);
    } else {
      preamble.push(line);
    }
  }

  const sourceLines =
    sections.length === 0
      ? body.split("\n")
      : [
          ...preamble,
          ...sections
            .filter((section) => !INSTALL_SECTION.test(section.heading))
            .flatMap((section) => section.lines),
        ];

  const paragraphs: string[] = [];
  let buffer: string[] = [];
  const flush = () => {
    const text = buffer.join(" ").trim();
    if (text) paragraphs.push(text);
    buffer = [];
  };
  for (const line of sourceLines) {
    if (/^##/.test(line)) {
      flush();
    } else if (line.trim() === "") {
      flush();
    } else {
      buffer.push(line.trim());
    }
  }
  flush();

  const first = paragraphs[0] ?? "";
  const sentence = first.split(SENTENCE_BOUNDARY)[0];
  return { summary: truncate(sentence), details: paragraphs };
}
