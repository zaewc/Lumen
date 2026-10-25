// Where Lumen's releases live and which file belongs to which platform.
// Shared by the pages (links that work without JavaScript) and the release
// lookup in the browser (src/scripts/site.ts).

export const REPO = "zaewc/Lumen";
export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases`;
export const LATEST_URL = `${RELEASES_URL}/latest`;
export const SUMS_URL = `${LATEST_URL}/download/SHA256SUMS`;
export const API_URL = `https://api.github.com/repos/${REPO}/releases/latest`;

export type PlatformId = "mac" | "windows" | "linux";

export interface PlatformFile {
  /** End of the release file's name, such as `_universal.dmg`. */
  readonly suffix: string;
  /** Button text; without one the button says "Installer" in the page's language. */
  readonly label?: string;
}

export interface Platform {
  readonly id: PlatformId;
  readonly name: string;
  /** Desktop files; the first one is the primary download. */
  readonly files: readonly [PlatformFile, ...PlatformFile[]];
}

export const PLATFORMS: readonly Platform[] = [
  { id: "mac", name: "macOS", files: [{ suffix: "_universal.dmg", label: ".dmg" }] },
  { id: "windows", name: "Windows", files: [{ suffix: "_x64-setup.exe" }] },
  {
    id: "linux",
    name: "Linux",
    files: [
      { suffix: "_amd64.deb", label: ".deb" },
      { suffix: "_amd64.AppImage", label: "AppImage" },
    ],
  },
];
