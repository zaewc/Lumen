/** A desktop Mac, by its user agent (WKWebView and Safari say "Macintosh"). */
export function isMac(userAgent: string): boolean {
  return /Macintosh|Mac OS X/.test(userAgent) && !/iPhone|iPad|iPod/.test(userAgent);
}

/** This browser's user agent, or "" where there is none (tests). */
export function userAgent(): string {
  return typeof navigator === "undefined" ? "" : navigator.userAgent;
}
