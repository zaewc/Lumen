import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

// English copy: sentence case, active and plain, Lumen's glossary.
export const en: Messages = {
  lang: "en",
  otherLanguage: "한국어",
  description:
    "Finds disk space you can safely free. You can restore anything you clean up until you remove it for good. Beta for macOS, Windows and Linux.",
  ogDescription:
    "Finds disk space you can safely free. You can restore anything you clean up until you remove it for good.",
  skip: "Skip to content",
  nav: { label: "Site", download: "Download", install: "First launch", faq: "FAQ" },
  hero: {
    lead: "Finds disk space you can safely free. You can restore anything you clean up until you remove it for good.",
    download: "Download",
    otherPlatforms: "Download for other platforms",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  how: {
    title: "How it works",
    steps: [
      {
        title: "Scan",
        text: "Read-only. Lumen never opens or changes files, and never downloads files kept only in the cloud.",
      },
      {
        title: "Decide",
        text: "A fixed safety policy marks each item Kept, Needs review or Proposed, and shows why.",
      },
      {
        title: "Confirm",
        text: "You look at the proposed items and confirm them. Nothing moves until you do.",
      },
      {
        title: "Quarantine",
        text: "Lumen moves confirmed items to Quarantine instead of deleting them. You can restore them until you remove them for good, which happens only when you ask. That's when you get the space back.",
      },
    ],
  },
  leftAlone: {
    title: "When in doubt, Lumen leaves it alone",
    items: [
      "Lumen always leaves system files, documents, credentials and keys, browser profiles and databases alone.",
      "If Lumen can't read a folder, it doesn't treat it as empty and leaves everything in it alone.",
      "Lumen leaves files in use and files synced with a cloud service alone.",
      "Lumen checks every item again just before moving it, and restoring never overwrites anything.",
    ],
  },
  privacy: {
    title: "Privacy",
    local:
      "Scan results, decisions, Quarantine and logs stay on this computer. Lumen sends no usage statistics or crash reports and doesn't track you.",
    aiTitle: "AI review (optional).",
    ai: "It's off until you turn it on in Settings. Then Lumen sends only the path, sizes, ages and evidence of each proposed item to the school's Jeb server, with your home folder shown as ~. It never sends file contents. AI can only make a decision more cautious; it can't choose what moves. The Jeb server's operator may keep the requests.",
  },
  download: {
    title: "Download",
    noScript: [
      "Files come from the ",
      { link: "latest GitHub release", href: `${REPO_URL}/releases/latest` },
      ".",
    ],
    checking: "Checking the latest version…",
    thisComputer: "You need to download",
    installer: "Installer",
    requirements: {
      mac: "macOS 13 or later · Apple silicon and Intel",
      windows: "Windows 10 22H2 or later, Windows 11 · 64-bit · no admin rights needed",
      linux: "x86_64 · Ubuntu 24.04 or later, or similar",
    },
  },
  files: {
    summary: "Other files: command line, checksums, SBOMs",
    intro: [
      "The ",
      { code: "lumen" },
      " command line comes as ",
      { code: "lumen-<target>.tar.gz" },
      " (macOS on Apple silicon, Linux) and ",
      { code: ".zip" },
      " (Windows). The ",
      { code: ".cdx.json" },
      " files list each build's components (SBOMs).",
    ],
  },
  verify: {
    summary: "Verify a download",
    introBefore: ["Put "],
    introAfter: [
      " next to your download and run this in that folder. Your file should show ",
      { code: "OK" },
      ".",
    ],
    windowsNote: " · compare the result with that file's line in SHA256SUMS; case doesn't matter.",
    provenance: "Build provenance",
    provenanceNote: [
      " · checks that this repository's release workflow built the file. Needs the ",
      { link: "GitHub CLI", href: "https://cli.github.com/" },
      ".",
    ],
  },
  install: {
    title: "Opening Lumen the first time",
    intro:
      "Lumen isn't code-signed yet, so macOS and Windows block its first launch. Allow it once and it opens normally after that.",
    mac: {
      steps: [
        ["Open the ", { code: ".dmg" }, " and drag Lumen into ", { ui: "Applications" }, "."],
        ["Open Lumen. When macOS says it cannot be opened, click ", { ui: "Done" }, "."],
        [
          "In ",
          { ui: "System Settings › Privacy & Security" },
          ", scroll to ",
          { ui: "Security" },
          ", click ",
          { ui: "“Open Anyway”" },
          " and confirm with your password or Touch ID.",
        ],
        ["Open Lumen again and click ", { ui: "Open Anyway" }, ". After that it opens normally."],
      ],
      aside: [
        "On macOS 13 and 14, Control-click › ",
        { ui: "Open" },
        " also works. From macOS 15, use the steps above.",
      ],
    },
    fda: {
      summary: "macOS Full Disk Access",
      intro:
        "With it, Lumen can also scan protected folders such as Mail, Messages and other apps' data. Without it, they stay unreadable and Lumen leaves everything in them alone.",
      steps: [
        [
          "Open ",
          { ui: "System Settings › Privacy & Security › Full Disk Access" },
          ", or use the button in Lumen's Settings.",
        ],
        ["Turn Lumen on. If it is not listed, add it from Applications with ", { ui: "+" }, "."],
        ["Quit and reopen Lumen, then scan again."],
      ],
      aside: [
        "Without a signature, macOS may see an updated Lumen as a different app. If folders are unreadable after an update, remove Lumen from the list with ",
        { ui: "−" },
        " and add it again.",
      ],
    },
    windows: {
      steps: [
        [
          "If your browser warns about the download, choose ",
          { ui: "Keep" },
          " in the download's menu.",
        ],
        [
          "Run the installer. In ",
          { ui: "Windows protected your PC" },
          ", click ",
          { ui: "More info" },
          ", then ",
          { ui: "Run anyway" },
          ".",
        ],
        [
          "Open Lumen from the Start menu as a regular user. Run as administrator, Lumen won't move files.",
        ],
      ],
    },
    linux: {
      aside: [
        "If the AppImage doesn't start, it may need FUSE 2 (Ubuntu 24.04: ",
        { code: "sudo apt install libfuse2t64" },
        "). Run Lumen as a regular user; run as root, it won't move files.",
      ],
    },
  },
  faq: {
    title: "FAQ",
    items: [
      {
        question: "Why the warning?",
        answer: [
          "Lumen isn't code-signed by Apple (Developer ID, notarization) or Microsoft yet. The warning means your system can't identify the developer, not that it found malware. Signing comes once the accounts exist. Until then you can check files with ",
          { link: "checksums and build provenance", href: "#verify" },
          ".",
        ],
      },
      {
        question: "Is it safe?",
        answer: [
          "It's a beta, so it may have bugs. That's why it's cautious: when unsure it leaves things alone, cleaning up only moves confirmed items to Quarantine, and you can restore them until you remove them for good. The source is public, and GitHub Actions builds each release from it. It's a good idea to keep a backup of anything important.",
        ],
      },
      {
        question: "What does AI review send?",
        answer: [
          "Nothing until you turn it on. Then Lumen sends only the path (home folder shown as ~), sizes, ages and evidence of each proposed item to the school's Jeb server. It needs a key from the school's AI Hub. Its answer can only make a decision more cautious, and Lumen ignores a missing or malformed answer. It stays on until you turn it off in Settings. The Jeb server's operator may keep the requests.",
        ],
      },
      {
        question: "How do I get something back?",
        answer: [
          "In ",
          { ui: "Quarantine" },
          ", select the item and choose ",
          { ui: "Restore" },
          ". It goes back to its original place and name. If something else now has that name, Lumen overwrites nothing and keeps the item in Quarantine.",
        ],
      },
      {
        question: "How do I uninstall it?",
        answer: [
          "First restore or remove for good what's in Quarantine. Uninstalling leaves Quarantine in place, still using space.",
        ],
        list: [
          [
            "macOS: move Lumen from Applications to the Trash. Its data is in ",
            { code: "~/Library/Application Support/dev.lumen.desktop" },
            ", Quarantine in ",
            { code: "~/Library/Application Support/Lumen/Quarantine" },
            ".",
          ],
          [
            "Windows: uninstall in ",
            { ui: "Settings › Apps › Installed apps" },
            ". Quarantine is in ",
            { code: "%LOCALAPPDATA%\\Lumen\\Quarantine" },
            ".",
          ],
          [
            "Linux: run ",
            { code: "sudo apt remove lumen" },
            ", or delete the AppImage. Quarantine is in ",
            { code: "~/.local/share/lumen/quarantine" },
            ".",
          ],
        ],
      },
      {
        question: "How do I update?",
        answer: [
          "There are no automatic updates yet. Download the new version here and install it the same way. Afterwards you may need to choose ",
          { ui: "Open Anyway" },
          " and grant Full Disk Access again.",
        ],
      },
      {
        question: "What is missing in the beta?",
        answer: [
          "During 0.x, features and screens may change, and there's no signing or automatic update yet. Lumen can quarantine items on the disk with your user folder; it leaves items on other disks where they are. There's no phone app yet. Report problems as ",
          { link: "GitHub issues", href: `${REPO_URL}/issues` },
          ", and security issues as described in the ",
          { link: "security policy", href: `${REPO_URL}/blob/main/SECURITY.md` },
          ".",
        ],
      },
    ],
  },
  footer: {
    source: "Source",
    releases: "All releases",
    note: "This site uses no cookies or analytics. Your browser fetches the latest version from the GitHub API directly.",
  },
  copy: "Copy",
  client: {
    dateLocale: "en-US",
    download: "Download",
    downloadFor: "Download for {platform}",
    viewOnGitHub: "View on GitHub",
    downloadFromGitHub: "Download from GitHub",
    getFromGitHub: "Download from GitHub",
    comingSoon: "The first beta is coming soon. You'll be able to download it here.",
    comingSoonShort: "The first beta is coming soon. ",
    loadFailed: "Couldn't load the latest version. You can still download it from GitHub.",
    checkFailed: "Couldn't check the latest version. ",
    rateLimited:
      "GitHub is getting too many requests, so this page couldn't check the latest version. ",
    releaseNotes: "Release notes",
    githubReleases: "GitHub releases",
    allFiles: "See every file on GitHub",
    fileHeader: "File",
    sizeHeader: "Size",
    copied: "Copied",
  },
  notFound: { title: "Page not found", home: "Go to the Lumen download page" },
};
