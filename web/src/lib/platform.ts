export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") {
    return false;
  }
  return /Mac|iPhone|iPad|iPod/.test(navigator.platform);
}

export function modKeyLabel(): string {
  return isMacPlatform() ? "⌘" : "Ctrl";
}

export function modKeyCombo(key: string): string {
  return isMacPlatform() ? `⌘${key}` : `Ctrl+${key}`;
}

export function shiftKeyLabel(): string {
  return "⇧";
}
