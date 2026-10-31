import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

export const zhCN: Messages = {
  lang: "zh-CN",
  language: "语言",
  description:
    "适用于各种设备的存储空间管理。安全规则和可选的 AI 检查可帮助你了解存储空间，再决定要清理什么。Lumen 目前处于预发布阶段。",
  ogDescription: "文件安全比释放空间更重要。本地优先、开源，目前处于预发布阶段。",
  skip: "跳到正文",
  nav: { label: "网站", docs: "文档" },
  hero: {
    headline: ["Storage intelligence", "for every device."],
    lead: "哪些该保留，哪些该清理？AI 可以帮你判断。",
    download: "查看发行版本",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  showcase: {
    main: "Lumen「探索」页面示例：按决策分类的存储空间、开发工具和应用缓存中建议清理的 9.9 GB，以及由工具自身清理的项目和对应命令。",
  },
  features: {
    title: "Lumen 的功能",
    items: [
      {
        title: "找出占用空间的内容",
        text: "从 Xcode 构建数据、Gradle 缓存，到应用缓存和旧安装包。扫描时不会打开任何文件。",
      },
      {
        title: "解释每项决策",
        text: "项目由谁创建、能否重新生成、是否仍在使用：Lumen 根据这些证据决定保留、检查还是清理。",
      },
      {
        title: "AI 再检查一次",
        text: "启用 AI 检查后，项目的路径、大小和时间信息会发送到学校的 Jeb 服务器，服务器运营者可能会保存这些信息（不会发送文件内容）。AI 会重新检查建议清理的项目，并备注待检查项目可能是什么。",
      },
      {
        title: "经你确认后再清理",
        text: "文件会移到隔离区，你可以随时恢复。对于 pnpm 存储目录或 Xcode 模拟器，请检查并运行对应工具自己的清理命令。工具命令会直接删除数据，无法撤销。",
      },
    ],
  },
  trust: {
    items: [
      {
        headline: ["先保护你的文件。", "再考虑释放空间。"],
        text: "Lumen 不会自动删除任何内容。隔离的文件在彻底删除前都可以恢复。",
      },
      {
        headline: ["查看证据。", "不靠猜测。"],
        text: "了解文件是什么、是否仍在使用。不确定时，文件会留在原处。",
      },
    ],
    more: "查看架构设计",
  },
  download: {
    title: "下载与安装",
    noScript: ["文件可从", { link: "GitHub 发行版本", href: `${REPO_URL}/releases` }, "获取。"],
    checking: "正在检查发行版本…",
    thisComputer: "<-- 需要下载",
    installer: "安装程序",
    requirements: {
      mac: "macOS 13 或更高版本 · Apple silicon 和 Intel",
      windows: "Windows 10 22H2 或更高版本、Windows 11 · 64 位 · 无需管理员权限",
      linux: "x86_64 · Ubuntu 24.04 或更高版本，或同类发行版",
    },
  },
  files: {
    summary: "其他文件：命令行工具、校验和、SBOM",
    intro: [
      "命令行工具 ",
      { code: "lumen" },
      " 提供 ",
      { code: "lumen-<target>.tar.gz" },
      " (Apple silicon Mac、Linux) 和 ",
      { code: ".zip" },
      " (Windows) 格式。",
      { code: ".cdx.json" },
      " 文件列出了每个构建版本的组件 (SBOM)。",
    ],
  },
  verify: {
    summary: "验证下载文件",
    introBefore: ["将"],
    introAfter: [
      "与下载的文件放在同一文件夹中，然后在该文件夹运行下面的命令。对应文件的结果应显示 ",
      { code: "OK" },
      "。",
    ],
    windowsNote: " · 将结果与 SHA256SUMS 中该文件对应的行进行比较；字母大小写不影响结果。",
    provenance: "构建来源证明",
    provenanceNote: [
      " · 用于确认文件由此仓库的发行工作流构建。需要安装 ",
      { link: "GitHub CLI", href: "https://cli.github.com/" },
      "。",
    ],
  },
  install: {
    title: "首次打开 Lumen",
    intro:
      "Lumen 尚未进行代码签名，因此 macOS 和 Windows 会阻止首次启动。允许一次后，之后即可正常打开。",
    mac: {
      steps: [
        ["打开 ", { code: ".dmg" }, "，将 Lumen 拖到", { ui: "应用程序" }, "文件夹。"],
        ["打开 Lumen。如果 macOS 提示无法打开，请点击", { ui: "完成" }, "。"],
        [
          "打开",
          { ui: "系统设置 › 隐私与安全性" },
          "，向下滚动到",
          { ui: "安全性" },
          "，点击",
          { ui: "“仍要打开”" },
          "，然后使用密码或 Touch ID 确认。",
        ],
        ["再次打开 Lumen 并点击", { ui: "仍要打开" }, "。之后即可正常打开。"],
      ],
      aside: [
        "macOS 13 和 14 也可以按住 Control 并点按应用，然后选择",
        { ui: "打开" },
        "。macOS 15 及更高版本请使用上述步骤。",
      ],
    },
    fda: {
      summary: "macOS 完全磁盘访问权限",
      intro:
        "获得此权限后，Lumen 还可以扫描邮件、信息和其他应用数据等受保护文件夹。未获得权限时，这些文件夹无法读取，Lumen 会保留其中的所有内容。",
      steps: [
        [
          "打开",
          { ui: "系统设置 › 隐私与安全性 › 完全磁盘访问权限" },
          "，也可以使用 Lumen 设置中的按钮。",
        ],
        ["启用 Lumen。如果列表中没有它，请点击", { ui: "+" }, "，从“应用程序”文件夹添加。"],
        ["退出并重新打开 Lumen，然后再次扫描。"],
      ],
      aside: [
        "由于 Lumen 尚未签名，macOS 可能会将更新后的版本视为另一个应用。如果更新后文件夹无法读取，请点击",
        { ui: "−" },
        "从列表中移除 Lumen，再重新添加。",
      ],
    },
    windows: {
      steps: [
        ["如果浏览器对下载发出警告，请在下载菜单中选择", { ui: "保留" }, "。"],
        [
          "运行安装程序。在",
          { ui: "Windows 已保护你的电脑" },
          "窗口中，点击",
          { ui: "更多信息" },
          "，再点击",
          { ui: "仍要运行" },
          "。",
        ],
        ["从开始菜单以普通用户身份打开 Lumen。以管理员身份运行时，Lumen 不会移动文件。"],
      ],
    },
    linux: {
      aside: [
        "如果 AppImage 无法启动，可能需要 FUSE 2 (Ubuntu 24.04：",
        { code: "sudo apt install libfuse2t64" },
        ")。请以普通用户身份运行 Lumen；以 root 运行时不会移动文件。",
      ],
    },
  },
  footer: { source: "GitHub 仓库", releases: "所有发行版本", contact: "联系" },
  copy: "复制",
  client: {
    dateLocale: "zh-CN",
    download: "选择桌面版下载",
    downloadFor: "下载 {platform} 版本",
    viewOnGitHub: "在 GitHub 上关注开发进度",
    downloadFromGitHub: "查看 GitHub 发行版本",
    getFromGitHub: "查看 GitHub 发行版本",
    comingSoonShort: "暂时还没有公开发行版本。 ",
    loadFailed: "无法获取发行版本信息，请直接访问 GitHub。",
    checkFailed: "无法获取发行版本信息。 ",
    rateLimited: "GitHub 请求过多，无法检查最新版本。 ",
    releaseNotes: "发行说明",
    githubReleases: "GitHub 发行版本",
    allFiles: "在 GitHub 上查看所有文件",
    fileHeader: "文件",
    sizeHeader: "大小",
    copied: "已复制",
  },
  notFound: { title: "找不到页面", home: "前往 Lumen 下载页面" },
};
