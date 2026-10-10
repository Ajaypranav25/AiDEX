/** True on macOS and iOS, where ⌘ (Meta) is the command modifier instead of Ctrl. */
export function isApplePlatform(): boolean {
  return /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent);
}
