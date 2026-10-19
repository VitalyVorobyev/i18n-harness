// Whether the start screen offers the bundled sample, per viewer. Browser
// storage can be unavailable; the offer is then simply shown.

const KEY = "i18n-harness:hide-sample";

export function sampleHidden(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

export function hideSample(): void {
  try {
    localStorage.setItem(KEY, "1");
  } catch {
    // The offer comes back next time; harmless.
  }
}
