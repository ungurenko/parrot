export function displayShortcut(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => (part.trim() === "Alt" ? "Option" : part.trim()))
    .join("+");
}

export const SHORTCUT_GLYPH: Record<string, string> = {
  Cmd: "⌘",
  Command: "⌘",
  Meta: "⌘",
  Shift: "⇧",
  Option: "⌥",
  Alt: "⌥",
  Ctrl: "⌃",
  Control: "⌃",
};
