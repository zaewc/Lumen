import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

// English copy: sentence case, active and plain, Lumen's glossary.
export const en: Messages = {
  lang: "en",
  otherLanguage: "한국어",
  description:
    "Storage intelligence for every device. Safety rules, with AI review (on by default; you can turn it off), help you understand your storage before deciding what to remove. Lumen is pre-alpha software.",
  ogDescription:
    "Storage intelligence for every device. Safety over reclaimed space. Local-first, open source, pre-alpha.",
  skip: "Skip to content",
  nav: { label: "Site", docs: "Docs" },
  hero: {
    headline: ["Storage intelligence", "for every device."],
    lead: "What to keep and what to clear: AI helps you work it out.",
    download: "View releases",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  showcase: {
    main: "Lumen's Clean up page, proposing 9.9 GB across developer tools and app caches, and listing items that only their own tool should clean with the command to run.",
    review: {
      alt: "Items that need review, each with an AI note such as: likely an app's settings or data.",
      caption: "AI notes say what each item to review likely is.",
    },
    quarantine: {
      alt: "Quarantine, with one item selected to restore or remove for good.",
      caption: "Restore from Quarantine, or remove several items for good at once.",
    },
  },
  features: {
    title: "What Lumen does",
    items: [
      {
        title: "Finds what takes up space",
        text: "From Xcode's build data and Gradle caches to app caches and old installers. Scanning never opens a file.",
      },
      {
        title: "Explains every decision",
        text: "Who made an item, whether it can be rebuilt, whether anything still uses it: the evidence decides between keep, review and clean up.",
      },
      {
        title: "Never deletes on its own",
        text: "Cleaned-up items move to a quarantine on the same drive. Restore them any time; they are removed for good only when you say so.",
      },
      {
        title: "Shows a tool's own command",
        text: "For the pnpm store, Xcode simulators and the like, Lumen leaves the files alone and shows the command that cleans them.",
      },
    ],
  },
  trust: {
    items: [
      {
        headline: ["Your files come first.", "Free space comes next."],
        text: "Nothing is removed automatically. Restore quarantined files until you remove them for good.",
      },
      {
        headline: ["Look at the evidence.", "Leave out the guesswork."],
        text: "What a file is. What still uses it. When the answer is uncertain, it stays put.",
      },
      {
        headline: ["Safety rules decide.", "AI takes a second look."],
        text: "AI review is on by default, and you can turn it off in Settings. It rechecks what Lumen proposes and tells you what unclear items are. It can only make a decision more careful, and file contents never leave your device.",
      },
    ],
    more: "Read the architecture",
  },
  download: {
    title: "All downloads and installation",
    noScript: [
      "Files come from the ",
      { link: "GitHub releases", href: `${REPO_URL}/releases` },
      ".",
    ],
    checking: "Checking releases…",
    thisComputer: "<-- You need to download",
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
  footer: {
    source: "Source",
    releases: "All releases",
  },
  copy: "Copy",
  client: {
    dateLocale: "en-US",
    download: "Choose a desktop download",
    downloadFor: "Download for {platform}",
    viewOnGitHub: "Follow development on GitHub",
    downloadFromGitHub: "Check GitHub releases",
    getFromGitHub: "Check GitHub releases",
    comingSoonShort: "No public release yet. ",
    loadFailed: "Release information is unavailable. Check GitHub directly.",
    checkFailed: "Release information is unavailable. ",
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
