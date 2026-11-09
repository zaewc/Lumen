import type { ConfidenceView, ItemKindView } from "@lumen/api-client";
import type { Messages } from "./en";
import { countFormat } from "./format";
import type { RuleId } from "./rules";

const count = countFormat("zh-CN");

/** “1 个项目”、“1,204 个项目”。 */
const items = (n: number) => `${count(n)} 个项目`;

/** “Spotify 和 Discord”；超过三个时显示“A、B、C 等另外 2 个”。 */
function appList(apps: readonly string[]): string {
  if (apps.length > 3) {
    return `${apps.slice(0, 3).join("、")}等另外 ${count(apps.length - 3)} 个`;
  }
  return new Intl.ListFormat("zh-CN", { type: "conjunction" }).format(apps);
}

/** 中文目录。结构与其他语言目录一致。 */
export const zhCN: Messages = {
  language: "语言",
  nav: {
    label: "主导航",
    overview: "存储空间",
    cleanUp: "探索",
    quarantine: "隔离区",
    apps: "应用",
    settings: "设置",
    held: (n: number) => `隔离区中有 ${items(n)}`,
  },
  common: {
    loading: "正在加载…",
    close: "关闭",
    details: "详情",
    noDetails: "Lumen 未能获取此错误的详细信息。",
    tryAgain: "重试",
    scanDisk: "扫描磁盘",
    scanning: "正在扫描…",
    scanAgain: "重新扫描",
    scanOther: "扫描其他文件夹",
    items: (n: number) => items(n),
  },
  overview: {
    title: "存储空间",
    disk: {
      title: "启动磁盘",
      meter: "磁盘使用情况",
      used: (size: string) => `已使用 ${size}`,
      total: (size: string) => `总计 ${size}`,
      percentUsed: (percent: string) => `已使用 ${percent}%`,
      assessed: (size: string) => `已评估：${size}`,
      low: "空间即将用满：可用空间不足 10%。",
      loadFailed: "Lumen 无法读取磁盘使用情况。",
    },
    session: {
      title: "上次扫描",
      quarantine: "隔离区中",
      atLeast: (size: string) => `至少 ${size}`,
      found: (n: string) => `已读取 ${n} 项`,
      openExplore: "打开“探索”",
      showProgress: "显示进度",
      cleaned: (n: number, size: string) => `${items(n)}（${size}）已移至隔离区。`,
      none: "尚未扫描。扫描磁盘以探索其中的内容，并查看每项决策背后的证据。",
    },
    tools: {
      title: "开发工具",
      intro: "询问 Xcode、Docker 和包管理器各自占用多少空间。Lumen 只读取它们的回答。",
      check: "检查开发工具",
      checking: "正在检查…",
      checkAgain: "再次检查",
      loadFailed: "Lumen 无法询问开发工具。",
      notInstalled: "未安装",
      unsupported: "此电脑上无法检查",
      unavailable: "无法检查",
      none: "没有报告任何项目",
      inUse: (detail: string) => `使用中 ${detail}`,
      reclaimable: (size: string) => `Docker 估计 ${size} 未使用`,
      cache: "缓存",
      sharedStore: "共享存储",
      ownCommand: "专用命令",
      xcodeManage: "在 Xcode › Settings › Components 中管理运行时。",
      dockerManage:
        "Docker 认为未使用的空间包括卷和已停止的容器，其中可能有你需要的数据。Docker 清理只会在“探索”中确认完整命令后运行，并保留卷。",
      sharedStoreNote: "项目会链接到此存储。它的专用命令会保留仍在使用的包。",
      cacheNote: "需要时会重新下载。",
    },
    copies: {
      title: "相同的文件",
      intro:
        "选择文件夹后，Lumen 会比较其中的文件，找出内容相同的文件。Lumen 只读取你选择的文件夹中的文件。",
      choose: "选择要比较的文件夹…",
      chooseAgain: "比较其他文件夹…",
      stop: "停止",
      stopping: "正在停止…",
      stopped: "已停止比较。不会显示这次比较的结果。",
      comparing: "正在比较…",
      stage: {
        head: "正在比较大小相同的文件的开头",
        tail: "正在比较仍然相同的文件的结尾",
        full: "正在完整比较仍然相同的文件",
      },
      filesRead: (n: string) => `已读取 ${count(n)} 个文件`,
      folders: "已比较的文件夹",
      group: (n: number, size: string) => `${count(n)} 个副本 · 每个 ${size}`,
      none: "在能够比较的文件中，没有内容相同的文件。",
      more: (n: number) => `另有 ${count(n)} 组较小的文件未显示。`,
      hardLinks: (n: number) =>
        `${count(n)} 个文件有多个名称（硬链接）。它们各是一个文件，不是副本。`,
      skipped: (n: number) =>
        `${count(n)} 个文件未比较：它们已更改、无法读取、仅在云端，或受系统保护。`,
      protectedFiles: (n: number) => `受保护位置中的 ${count(n)} 个文件未读取。`,
      incomplete: "部分文件夹未能完整读取，其中的文件未比较。",
      unsupported: "这台电脑暂时还不能比较文件。",
      tooMany: (limit: string) => `这些文件夹中的文件超过 ${count(limit)} 个。请选择更小的文件夹。`,
      failed: "Lumen 无法比较这些文件夹。",
      advice:
        "副本可能是你想保留的备份。Lumen 不会选择或删除任何副本。删除一个副本释放的空间可能小于它的大小。",
    },
  },
  workspace: {
    available: "可用空间",
    assessed: "已评估项目 · 按大小排列",
    assessment: (n: number) => `已评估 ${items(n)}`,
    unknownOrigin: "来源信息未知",
    workflow: "从扫描到清理",
    scanStage: "扫描",
    resultsStage: "查看结果",
    cleanUpStage: "清理",
    activeCommand: "命令正在运行",
    quitHeld: "Lumen 正在移动或移除项目。操作完成后即可关闭。",
  },
  start: {
    title: "查看磁盘上的内容",
    promise: "扫描只会读取数据，耗时几分钟。移动任何内容前，Lumen 都会征求你的同意。",
    wholeDiskHint: "电脑可能会询问你是否允许访问某些文件夹。",
    chooseFolders: "仅扫描部分文件夹",
    foldersLabel: "要扫描的文件夹，每行一个",
    foldersEmpty: "请至少输入一个文件夹。",
    scanFolders: "扫描这些文件夹",
    failed: "扫描未能完成。你的文件没有变化。",
    aiOn: (service: string) =>
      `AI 检查已开启：Lumen 会将建议处理项目和部分待检查项目的详细信息（绝不会发送其内容）发送到 ${service} 服务器。你可以在“设置”中关闭此功能。`,
  },
  ai: {
    title: "AI 检查",
    offIntro:
      "AI 模型会再次检查 Lumen 建议清理的内容，并说明待检查项目可能是什么。它只能让 Lumen 的判断更加谨慎。",
    onIntro: (key: string) =>
      `已开启，使用 ${key}。Lumen 会发送每个建议处理项目以及最多 20 个待检查项目的详细信息，绝不会发送其内容。AI 只能让 Lumen 的判断更加谨慎。你可以随时关闭此功能。 还会发送已安装应用的名称、发布者和版本，不含路径。`,
    keyInUse: {
      builtin: (service: string) => `${service}（内置密钥）`,
      stored: (service: string) => `${service}（你的密钥）`,
    },
    checking: "正在检查…",
    short: {
      on: "AI 检查已开启",
      off: "AI 检查已关闭",
    },
    loadFailed: "Lumen 无法加载 AI 检查设置。",
    changeFailed: "Lumen 无法更改 AI 检查设置。",
    consent: {
      title: "开启 AI 检查？",
      what: "对于每个建议处理项目以及最多 20 个体积最大的待检查项目，Lumen 会发送其路径（主文件夹会显示为 ~）、大小、存放时间以及决策依据。绝不会发送文件内容。 为了应用页面，还会发送已安装应用的名称、发布者、版本及与其关联的后台项目，不含路径。",
      whereJeb:
        "Lumen 会使用你的 API 密钥将这些信息发送到你学校的 Jeb 服务器（jeb.https.gsmsv.site）。服务器运营者可能会保留请求。",
      whereJebBuiltin:
        "Lumen 会使用内置密钥将这些信息发送到某所学校的 Jeb 服务器（jeb.https.gsmsv.site）。服务器运营者可能会保留请求。",
      whereOpenRouter:
        "Lumen 会使用你的 API 密钥，通过 OpenRouter 将这些信息发送给 TypeSafe 的 Jev。",
      cautious:
        "AI 只能让决策更加谨慎。对于待检查项目，AI 只会补充说明每项可能是什么。AI 绝不会选择要移动或移除的内容。",
      revocable: "之后每次扫描都会运行 AI 检查。你可以随时在“设置”中关闭此功能。",
      keyLabel: "Jeb API 密钥",
      keyHint: "Lumen 会将密钥保存在系统的安全存储中，之后不会再次显示。",
      keyInvalid: "请粘贴完整密钥：1–256 个字符，不能包含空格。",
      ownKey: "使用自己的密钥",
      ownKeyHint:
        "可选。Lumen 会使用你的密钥而非内置密钥，将其保存在系统的安全存储中，之后不会再次显示。",
      confirm: "开启 AI 检查",
      turningOn: "正在开启…",
    },
    firstLaunch: {
      title: "使用 AI 检查？",
      intro:
        "Lumen 会依据自身的安全规则进行判断。如果你愿意，AI 模型可以再检查一次。只有在你选择后，才会发送信息。",
      on: "开启 AI 检查",
      off: "不使用 AI 检查，继续",
    },
  },
  scanning: {
    title: "正在扫描…",
    deciding: "正在判断…",
    decidingHint: "Lumen 正在确认每个项目是什么以及该如何处理，不会更改任何内容。",
    stages: {
      inventory: "正在检查已安装的应用…",
      markers: "正在检查项目的来源…",
      evidence: "正在识别缓存和工具文件夹…",
      graph: "正在关联项目及其应用和工具…",
      deciding: "正在应用安全规则…",
      describing: "AI 检查正在描述待检查项目…",
    },
    stagePercent: (n: number) => `当前阶段完成 ${n}%`,
    processed: (done: string, total: string) => `已检查 ${done} / ${total} 个项目`,
    readOnly: "Lumen 只读取，不会更改任何内容。",
    progress: "扫描进度",
    items: "已读取的条目",
    unreadable: "无法读取的文件夹",
    reading: "正在读取",
    readSoFar: (size: string) => `已读取：${size}`,
    failed: "Lumen 无法判断找到的内容。你的文件没有变化。",
  },
  results: {
    map: {
      label: "已评估项目分布图，按大小排列",
      all: "所有类型",
      smaller: (n: number) => `${count(n)} 个较小项目`,
      notRead: "未读取",
      notReadYet: "尚未读取",
      entries: (n: number) => `${count(n)} 条`,
    },
    canFree: (size: string) => `可以清理 ${size}`,
    nothingTitle: "没有可清理的内容",
    nothing: "Lumen 只建议处理安全规则允许的项目，例如应用可以重新生成的缓存。",
    filters: {
      all: "全部",
    },
    groups: {
      command: "使用对应工具的命令清理",
      developer: "开发者工具",
      app_cache: "应用缓存",
      app_data: "应用数据",
      other: "其他",
    },
    otherTools: "其他工具",
    commandNote:
      "这些项目只能由对应工具移除。打开某项即可运行其命令；运行前请检查移除的内容，因为操作无法撤销。",
    status: {
      quarantine: "建议处理",
      command: "专用命令",
      review: "需要检查",
      keep: "保留",
    },
    columns: {
      name: "名称",
      location: "位置",
      status: "状态",
      size: "大小",
    },
    listHint: "使用向上和向下方向键在项目间移动。",
    emptyFilter: "没有符合此状态的项目。",
    partial: "扫描提前停止。以下是停止前找到的内容。",
    whenClosed: (size: string, ...apps: string[]) =>
      apps.length === 0
        ? `退出正在使用部分保留项目的应用，然后重新扫描以再清理 ${size}。`
        : `退出 ${appList(apps)}，然后重新扫描以再清理 ${size}。`,
    outOfSpace:
      "由于磁盘空间即将用满，扫描已停止（Lumen 始终会保留 2 GB 可用空间）。以下是停止前找到的内容。请先释放一些空间，然后重新扫描。",
  },
  action: {
    cleanUp: "清理…",
    preparing: "正在准备…",
    nothing: "没有建议清理的内容。",
    planFailed: "Lumen 无法准备清理操作。你的文件没有变化。",
  },
  detail: {
    size: "大小",
    location: "位置",
    decision: "决策",
    reasons: "证据",
    aiNoteHint: "这是 AI 检查的推测，不会改变 Lumen 的决策。",
    whenClosed: (...apps: string[]) =>
      apps.length === 0
        ? "退出正在使用此项目的应用，然后重新扫描以清理它。"
        : `退出 ${appList(apps)}，然后重新扫描以清理它。`,
    relatedNone: "没有记录显示此项目与其他项目有关联。",
    sources: {
      rule: "规则",
      location: "位置",
      time: "时间",
      relation: "关联",
      process: "进程",
      ai: "AI 检查",
    },
    owner: (app: string, present: boolean | null) =>
      present === null
        ? `属于 ${app}。是否已安装尚不清楚。`
        : `属于 ${app}，该应用${present ? "已安装" : "未安装"}。`,
    appRunning: (app: string, running: boolean) =>
      running ? `${app} 正在运行。` : `${app} 未在运行。`,
    appOpened: (app: string, date: string) =>
      `${app} 上次打开于 ${date}；系统可能不会记录每次启动。`,
    projectChanged: (date: string) => `项目文件上次更改于 ${date}；只看清单和锁定文件这类文件。`,
    inUse: (inUse: boolean): string =>
      inUse ? "有应用正在使用此项目。" : "没有应用正在使用此项目。",
    unseen: (unseen: number | null) =>
      unseen === null
        ? "没有可见的应用正在使用此项目。"
        : `没有可见的应用正在使用此项目；有 ${count(unseen)} 个进程无法检查。`,
    changed: (date: string, folder: boolean) =>
      folder
        ? `文件夹条目上次更改于 ${date}；其中的内容之后可能有过变化。`
        : `上次更改时间：${date}。`,
    opened: (date: string, folder: boolean) =>
      `${folder ? "文件夹上次读取于" : "上次打开于"} ${date} 或之后；系统可能不会记录每次使用。`,
    notRecorded: (which: "modified" | "accessed"): string =>
      which === "modified" ? "未记录上次更改时间。" : "未记录上次使用时间。",
    notRead: (which: "modified" | "accessed"): string =>
      which === "modified" ? "上次更改时间未知：未读取。" : "上次使用时间未知：未读取。",
    means: {
      keep: "Lumen 会保留此项目，不会移动任何内容。",
      review: "Lumen 会将此项目留给你判断，不会自行移动。",
      command: "应由对应工具自行清理。只有在你要求时，Lumen 才会运行命令。",
      quarantine: "Lumen 建议将此项目移至隔离区。之后可以从隔离区恢复。",
    },
    recheck: {
      unknown: "Lumen 判断时不知道：",
      gaps: {
        in_use: "是否有应用在使用它",
        unread: "其中的全部内容",
        stale: "它现在的状态",
        owner: "它属于哪个应用，以及该应用是否已安装",
      },
      action: "重新检查",
      checking: "正在检查…",
      note: "检查只读取，不会移动任何内容。",
      settled: "没有需要检查的：这个判断不依赖 Lumen 不知道的信息。",
      changed: "扫描后它已改变，因此 Lumen 没有重新判断。请重新扫描。",
      still: (status: string) => `已重新检查 · 仍为${status}`,
      now: (before: string, after: string) => `已重新检查 · ${before} → ${after}`,
      failed: "无法重新检查。",
    },
    lens: {
      tool: (n: number, tool: string, size: string) => `${tool} 的另外 ${items(n)} · ${size}`,
      processes: (n: number, size: string, ...apps: string[]) =>
        `${items(n)} 也由 ${appList(apps)} 使用 · ${size}`,
      evidence: (n: number, size: string) => `扫描记录关联的另外 ${items(n)} · ${size}`,
      kind: (n: number, group: string, size: string) => `${group}中的另外 ${items(n)} · ${size}`,
    },
  },
  aiNote: {
    likely: (kind: string, confidence: string) => `AI：可能是${kind}（${confidence}）`,
    unknown: "AI：无法判断这是什么",
    kinds: {
      app_settings: "应用设置或数据",
      user_documents: "你的文档",
      cache: "缓存",
      logs: "日志",
      downloads_or_installers: "下载文件或安装程序",
      developer_tool_data: "开发者工具数据",
      backup_or_archive: "备份或存档",
      unknown: "无法判断的内容",
    } satisfies { readonly [K in ItemKindView]: string },
    confidence: {
      low: "置信度低",
      medium: "置信度中",
      high: "置信度高",
    } satisfies { readonly [C in ConfidenceView]: string },
  },
  toolCommand: {
    homebrewDownloads:
      "Homebrew 会清理旧版已安装程序和近期下载缓存。之后安装时可能需要重新下载这些文件。",
    title: (tool: string) => `${tool} 可以自行清理此项目`,
    copy: "复制",
    copied: "已复制",
    copyFailed: "Lumen 无法复制。请自行选择并复制命令。",
    note: "Lumen 不会运行此命令。运行前请检查它会移除什么。",
    run: "运行命令…",
    runConfirm: "运行命令",
    runWarning:
      "此命令会立即删除文件。文件不会进入隔离区，因此 Lumen 无法恢复。运行前请检查它会移除什么。",
    removesData: "它会移除 Docker 中已停止的容器、未使用的网络、悬空镜像和未使用的构建缓存。",
    running: "正在运行…可能需要几分钟。",
    succeeded: "命令已完成。请重新扫描以查看剩余内容。",
    failed: (code: number | null) =>
      code === null ? "命令执行失败。" : `命令执行失败（退出代码 ${code}）。`,
    timedOut: "命令运行超过 15 分钟仍未结束，Lumen 已将其停止。",
    output: "显示输出",
    runFailed: "Lumen 无法运行此命令。",
    runNote: "运行后无法撤销。运行前请检查它会移除什么。",
  },
  spaceChange: {
    estimate: (size: string) => `所选项目：预计 ${size}。`,
    increased: (size: string) => `观察到可用空间增加了 ${size}。`,
    decreased: (size: string) => `观察到可用空间减少了 ${size}。`,
    unchanged: "读数显示可用空间没有变化。",
    unknown: "Lumen 无法测量可用空间的变化。",
    multiple: "已分别测量每个磁盘的可用空间。",
    shared: "卷可能共用可用空间，因此这些读数会分别显示。",
    details: "查看测量数据",
    disk: (index: number) => `磁盘 ${index}`,
    readings: (before: string, after: string) => `可用空间：${before} → ${after}`,
    note: "其他应用也会影响这些读数。磁盘空间可能需要一些时间才会释放。",
  },
  coverage: {
    incomplete: "Lumen 无法读取部分文件夹，因此未处理其中的内容。显示的大小可能偏大。",
    howTo: "如何授予“完全磁盘访问权限”",
  },
  details: {
    title: "扫描详情",
    entries: (entries: string) => `已读取 ${count(entries)} 条`,
    files: "文件",
    fileCount: (files: string) => count(files),
    size: "大小",
    sizeValue: (logical: string, allocated: string) => `${logical}（磁盘占用 ${allocated}）`,
    reclaimable: "可释放空间",
    atLeast: (size: string) => `至少 ${size}`,
    complete: "Lumen 已读取全部内容。",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `Lumen 未能读取全部内容：${count(denied)} 项被拒绝，${count(failed)} 项失败，${count(notScanned)} 项未扫描。`,
  },
  confirm: {
    title: (n: number, size: string) => `将 ${items(n)}（${size}）移至隔离区？`,
    checked: "Lumen 会先重新检查每个项目；自扫描后发生变化的项目会留在原处。",
    restorable: "至少 30 天内，你都可以从隔离区恢复所有内容，直到将其永久移除。",
    nothingLeft: "没有剩余内容可移动。请重新扫描以查看可以清理的内容。",
    items: "待移动项目",
    move: "移至隔离区",
    moving: "正在移动…",
    failed: "Lumen 无法执行清理操作。",
    planMade: (createdAt: string) => `Lumen 于 ${createdAt} 创建了此计划。你可以在 24 小时内使用。`,
    planHash: "计划哈希值",
  },
  done: {
    title: (n: number, size: string) => `已将 ${items(n)}（${size}）移至隔离区。`,
    noneMoved: "Lumen 没有移动任何内容。",
    freedNote: "从隔离区永久移除这些项目后，才会释放相应空间。",
    openQuarantine: "打开隔离区",
    notMoved: (n: number) => `${items(n)}未移动`,
    left: (n: number) => `本次扫描剩余 ${items(n)}`,
    scanAgainToMove: "要移动更多项目，请重新扫描。",
  },
  item: {
    quarantined: "已移至隔离区。你可以恢复。",
    skipped: (why: string) => `未移动：${why}。`,
    failed: (reason: string, where: string) => `移动失败：${reason}；${where}。`,
    failIdentityMismatch: "扫描后发生了变化",
    failInUse: "有程序正在使用",
    failVolumeUnsupported: "此驱动器无法存放隔离区",
    failMoveFailed: "移动时发生错误",
    failVerificationFailed: "移动后与计划不符",
    failOther: "原因未知（此版本无法识别）",
    skipChanged: "已发生变化或无法再检查",
    skipOutsideRoots: "位于已扫描文件夹之外",
    skipNowKeep: "Lumen 现在决定保留此项目",
    skipNowReview: "Lumen 现在要求检查此项目",
    skipReasonsChanged: "现在建议处理此项目的理由有所不同",
    skipNotPrepared: "Lumen 无法记录此项目以便安全移动",
    skipManifest: "Lumen 无法写入其记录",
    leftSource: "原位置未受影响",
    leftQuarantine: "位于隔离区，可以恢复",
    leftGone: "已不存在",
  },
  quarantine: {
    title: "隔离区",
    atLeast: (size: string) => `至少 ${size}`,
    columns: {
      name: "名称",
      location: "原始位置",
      size: "大小",
      held: "存放时间",
    },
    loadFailed: "Lumen 无法加载隔离区。",
    empty: "隔离区为空",
    emptyHint: "清理的项目会暂存在这里，改变主意时可以恢复。",
    goCleanUp: "打开“探索”",
    interrupted: (n: number) =>
      `清理操作中途停止，Lumen 无法确定 ${n === 1 ? "此项目" : `这 ${count(n)} 个项目`}的位置。Lumen 没有移除${n === 1 ? "它" : "它们"}。请检查${n === 1 ? "它原来" : "它们原来"}的位置：`,
    selection: {
      all: "全选",
      item: (name: string) => `选择 ${name}`,
      count: (n: number, size: string) => `已选择 ${items(n)} · ${size}`,
      clear: "清除选择",
      restore: "恢复",
      removeAsk: "永久移除…",
      removeWarning: (n: number) =>
        `这会永久删除 ${items(n)}，且无法撤销。Lumen 会先检查每个项目是否正在被使用。`,
      removeSkips: (n: number) => `${items(n)}只能恢复，仍会留在隔离区。`,
      remove: "永久移除",
      working: (done: number, total: number) => `正在处理…${count(done)} / ${count(total)}`,
      restored: (well: number, total: number) =>
        well === total
          ? `已恢复 ${items(total)}。`
          : `已恢复 ${count(well)} / ${items(total)}。其余项目仍在隔离区。`,
      removed: (well: number, total: number) =>
        well === total
          ? `已永久移除 ${items(total)}。`
          : `已永久移除 ${count(well)} / ${items(total)}。Lumen 没有移除其他内容。`,
      restoredStopped: (well: number, total: number, untried: number) =>
        [
          `已恢复 ${count(well)} / ${items(total)}，随后 Lumen 因错误而停止。`,
          untried > 0 ? `其余 ${count(untried)} 个项目尚未尝试恢复。` : null,
          "仍在列表中的内容会留在隔离区。",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
      unverified: (n: number) =>
        n === 1
          ? "有 1 个恢复的项目与记录不完全一致，请检查。"
          : `${count(n)} 个恢复的项目与记录不完全一致，请检查。`,
      removedStopped: (well: number, total: number, untried: number) =>
        [
          `已永久移除 ${count(well)} / ${items(total)}，随后 Lumen 因错误而停止。`,
          untried > 0 ? `其余 ${count(untried)} 个项目尚未尝试移除。` : null,
          "仍在列表中的内容会留在隔离区，你可以恢复。",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
    },
  },
  held: {
    unknownPath: "（位置未知）",
    days: (days: number) => (days === 0 ? "今天" : `${count(days)} 天`),
    restoreOnly: "仅可恢复",
    afterFailure: "移动时发生错误，因此 Lumen 已将此项目安全存放于此。你只能恢复它。",
    restore: "恢复",
    removeForGoodAsk: "永久移除…",
    removeForGoodWarning: "这会永久删除此项目，且无法撤销。Lumen 会先检查它是否正在被使用。",
    removeForGood: "永久移除",
    failed: "Lumen 无法完成操作。请重试。",
  },
  restore: {
    restored: "已恢复到原位置。",
    parentReplaced: "其文件夹后来已重新创建。",
    unverified: "此项目与记录不完全一致，请检查。",
    notRestored: (why: string) => `Lumen 无法恢复此项目。它仍在隔离区。${why}。`,
    nameTaken: "原位置现在已有同名内容。请先将其移开，然后重试",
    originUnreachable: "Lumen 无法安全访问原始文件夹",
    itemChanged: "此项目在隔离区期间发生了变化",
    moveFailed: "移回操作失败",
    notRestorable: "此项目目前无法由 Lumen 恢复。",
  },
  finalize: {
    finalized: "已永久移除。",
    foundAtSource: "此项目已回到原位置。Lumen 没有移除任何内容。",
    notFound:
      "Lumen 无法在隔离区中找到此项目。如果它存放在另一块驱动器上，请连接该驱动器后重试。Lumen 没有移除任何内容。",
    failed: (what: string) => `Lumen 无法彻底移除此项目。${what}。`,
    removedNothing: "没有移除任何内容",
    removedPartly: "部分内容已移除。请重试以完成操作",
    removedItem: "此项目已移除；仅剩其记录",
    maybeInUse:
      "此项目可能正在被使用，或者 Lumen 无法检查，因此没有移除任何内容。请关闭正在使用它的应用后重试。",
    notFinalizable: "此项目只能恢复。",
  },
  apps: {
    title: "应用",
    desktopOnly: "已安装的应用列表可在桌面应用中查看。",
    reading: "正在读取已安装的应用…",
    readAgain: "重新读取",
    loadFailed: "无法读取已安装的应用。",
    search: "搜索应用",
    showAll: (n: number) => `同时显示 ${n} 个依赖项和隐藏项`,
    none: "没有匹配的应用。",
    protected: "系统组件",
    protectedTip: "Lumen 绝不会删除此项。",
    partial: (names: string) => `部分列表未能完整读取，可能缺少一些应用：${names}。`,
    readOnly: "此列表从你的电脑读取并保留在本机。只有确认后才会移动应用。",
    readOnlyAi:
      "启用 AI 审查时，会向其发送这些应用的名称、发布者和版本，不含路径。只有确认后才会移动应用。",
    move: {
      action: "移至隔离区…",
      title: (name: string) => `将“${name}”移至隔离区？`,
      moving: (name: string) => `正在将“${name}”移至隔离区…`,
      checking: "正在检查应用…",
      failed: "无法检查或移动应用。重试前请先查看隔离区。",
      refused: "这个应用无法移至隔离区",
      notFound: "列表中已找不到这个应用。请重新读取列表。",
      ambiguous: "多个应用使用同一个标识符，Lumen 无法选择。",
      unsupported: "将应用移至隔离区的功能仅适用于 macOS。",
      size: "应用大小",
      atLeast: (size: string) => `至少 ${size}`,
      expires: (date: string) => `此预览在创建 24 小时后的 ${date} 到期。`,
      moved: (name: string) => `已将“${name}”移至隔离区。`,
      noSpace: "只有从隔离区永久删除应用后，才会释放空间。",
      changed: "应用已发生变化。请关闭预览并重新检查。",
      failedMove: "未能完成应用移动。",
      quit: "退出",
      quitHint: "像应用自己的“退出”菜单一样请求它退出。如有未保存的工作，应用会先询问你。",
      quitAsked: "已请求应用退出。如果它询问是否保存，请在应用中回答。",
      stillRunning: "应用仍在运行。请退出后重新检查。",
      quitItself: "Lumen 无法在这里退出自身。",
      quitFailed: "无法请求应用退出。请自行退出后重新检查。",
      recheck: "重新检查",
      notes: {
        goes_to_quarantine: "永久删除前可以恢复应用。Lumen 不会自动删除它。",
        library_data_stays: "~/Library 中的应用数据会保留。重新扫描后，可以在探索中查看。",
      },
      locations: {
        source: "应用仍在原来的位置。",
        quarantine: "应用在隔离区中。请打开隔离区查看。",
        gone: "无法确认应用的位置。重试前请先查看隔离区。",
      },
      reasons: {
        not_in_applications: "应用不直接位于 /Applications 或 ~/Applications 中。",
        protected: "它是 macOS 的一部分，Lumen 会保留它。",
        not_a_bundle: "它不是应用文件夹，或者是一个链接。",
        owner_unknown: "无法确认应用的所有者。",
        not_owned_by_user: "应用属于其他账户。请使用其安装程序或 App Store 删除。",
        services_unknown: "无法检查所有后台项目。",
        service_linked: "有后台项目与这个应用关联。请使用应用自己的卸载程序。",
        helpers_unknown: "无法检查应用的后台辅助程序。",
        has_helpers: "应用带有后台辅助程序。请使用它自己的卸载程序。",
        running_unknown: "无法确认应用是否正在运行。",
        running: "应用正在运行。请退出后重新检查。",
        store_unknown: "无法确认此驱动器是否有隔离区存储位置。",
        no_store_on_volume: "此驱动器没有隔离区存储位置。",
      },
    },
    ai: {
      explaining: "AI 检查正在说明这些应用…",
      note: (purpose: string, confidence: string, impact: string) =>
        `AI：很可能是${purpose}（${confidence}）。${impact}`,
      purposes: {
        web_browser: "网页浏览器",
        communication: "通讯应用",
        media: "媒体应用",
        office_or_productivity: "办公或效率应用",
        development_tool: "开发工具",
        design_or_creative: "设计或创意应用",
        game: "游戏",
        security_or_banking: "安全或银行软件",
        system_or_driver: "系统组件或驱动程序",
        cloud_sync_or_backup: "云同步或备份应用",
        utility: "实用工具",
        runtime_or_library: "运行时或库",
        unknown: "无法判断的项目",
      },
      impacts: {
        only_this_app: "删除它应该不会影响其他内容。",
        other_software_depends: "其他软件很可能依赖它。",
        stops_background_activity: "删除后其后台活动很可能会停止。",
        security_or_access: "删除后安全、银行或登录功能可能无法正常工作。",
        system_function: "删除后某项系统功能可能无法正常工作。",
        unknown: "无法判断删除它会有什么影响。",
      },
    },
    origins: {
      mac_app: "应用",
      mac_installer: "安装包",
      homebrew: "Homebrew",
      homebrew_cask: "Homebrew Cask",
      windows_program: "程序",
      msix: "商店包",
      dpkg: "dpkg",
      rpm: "RPM",
      pacman: "pacman",
      flatpak: "Flatpak",
      snap: "Snap",
      other: "其他",
    },
  },
  settings: {
    title: "设置",
    appearance: {
      title: "主题",
      system: "跟随系统",
      light: "浅色",
      dark: "深色",
      failed: "Lumen 无法更改窗口外观。",
    },
    permissions: {
      title: "权限",
      fda: "完全磁盘访问权限",
      fdaIntro:
        "你可以在 macOS“系统设置”中检查访问权限。如果已允许 Lumen 访问，可以跳过本指南。即使已授予权限，部分受保护的文件夹仍可能无法读取。",
      steps: "显示设置步骤",
      step1: "打开“系统设置”。",
      step2: "前往“隐私与安全性”›“完全磁盘访问权限”。",
      step3: "开启 Lumen。如果已开启，请保持开启。",
      step4: "如果更改了设置，请完全退出 Lumen，然后重新打开并开始新的扫描。",
      open: "打开“系统设置”",
      openFailed: "Lumen 无法打开“系统设置”。请改为从 Apple 菜单打开。",
      access: "文件夹访问权限",
      accessIntro: "电脑可能会禁止 Lumen 访问某些文件夹。Lumen 仍会读取所有获准访问的文件夹。",
      unread:
        "Lumen 会将无法读取的文件夹标记为未读取，而不会视为空文件夹，并且不会建议处理其中的内容。",
    },
    about: {
      title: "关于",
      version: "版本",
      local: "此设备上的数据",
      localBody:
        "扫描、决策和隔离区内容都保留在此电脑上。文件内容绝不会发送到任何地方。开启 AI 检查时，只会发送路径和大小等详细信息。 还会发送已安装应用的名称、发布者和版本。",
      update: {
        title: "更新",
        checking: "正在检查新版本…",
        upToDate: "Lumen 已是最新版本。",
        available: (version: string) => `有新版本 ${version} 可用。`,
        packageOnly: "此版本通过软件包安装：请前往 zaewc.github.io/Lumen 获取新版本。",
        checkFailed: "Lumen 无法检查新版本。",
        check: "立即检查",
        install: "更新并重新启动",
        installing: "正在更新…",
        installingNote: "Lumen 会下载新版本、检查其签名、完成安装并重新启动。",
        installFailed: "更新未能安装。当前版本未发生变化。",
        waitForItems: "Lumen 完成项目移动或移除后，即可更新。",
        sidebar: (version: string) => `有新版本 ${version}`,
        notice: (version: string) => `Lumen ${version} 已发布。`,
        noticeAction: "查看更新",
        later: "稍后",
      },
    },
  },
  whatIs: (term: string) => `“${term}”是什么意思？`,
  glossary: {
    verdicts: {
      title: "Lumen 如何决策",
      body: "Lumen 使用固定的安全规则判断每个项目，而不是猜测。“建议处理”：例如应用或工具可以重新生成的缓存。“使用专用命令清理”：只能由对应工具移除的项目；Lumen 会显示工具命令供你运行。“需要检查”：Lumen 无法确定的大型项目，或未安装应用留下的内容；绝不会自动移动。“保留”：Lumen 不会触碰的项目，例如系统文件、正在使用的文件、应用自身数据或无法识别的小型项目。如有疑问，Lumen 会让项目留在原处。",
    },
    quarantine: {
      title: "隔离区",
      body: "隔离区是安全的暂存位置。清理时，项目会移到这里而不是被删除；在永久移除之前，你可以将其恢复到原处。隔离区中的项目仍会占用磁盘空间。",
    },
  },
  reviewReasons: {
    unknownOwner: "无法确认它属于哪个应用或工具。",
    sharedResource: "它可能与其他文件共享存储空间，或在此项目之外还有链接。",
    staleEvidence: "部分依据已过时，请重新扫描后再决定。",
    uncertainSize: "永久删除后能释放多少空间尚不确定。",
  },
  reasons: {
    location: (name: string) => `保留：此项目位于受保护的位置（${name}）。`,
    rules: {
      "default.review": "需要检查：Lumen 无法识别此项目。",
      "default.keep-small": "保留：项目太小，不值得检查。",
      "protect.os-protected": "保留：操作系统对此项目提供保护。",
      "protect.system-file": "保留：Windows 将此项目标记为系统文件。",
      "protect.cloud-backed":
        "保留：此项目与云服务同步，在此处移除可能会导致它在所有位置都被删除。",
      "protect.in-use": "保留：正在运行的应用正在使用此项目。",
      "protect.app-running": "保留：此项目所属的应用正在运行。退出该应用后重新扫描即可清理。",
      "protect.in-use-partial": "保留：Lumen 可见的应用都未使用此项目，但 Lumen 无法检查所有应用。",
      "protect.in-use-unknown": "保留：Lumen 尚未检查是否有程序正在使用此项目。",
      "protect.not-observed": "保留：扫描尚未观察到此项目。",
      "protect.incomplete-scan": "保留：Lumen 无法读取此项目的部分内容，因此其内容未知。",
      "protect.location-data": "保留：Lumen 无法检查受保护位置列表。",
      "protect.location-context": "保留：Lumen 无法检查受保护位置列表。",
      "protect.contains-system": "保留：此项目包含操作系统的一部分。",
      "protect.contains-sensitive":
        "保留：此项目包含密码、密钥或网页浏览器登录信息，因此 Lumen 不会触碰。",
      "protect.contains-cloud":
        "保留：此项目包含云同步文件夹，在此处移除可能会导致其中的文件在所有位置都被删除。",
      "eligible.quarantine-unsupported": "需要检查：此设备无法以可逆方式将此项目移至隔离区。",
      "eligible.frees-nothing": "保留：移除此项目不会释放空间。",
      "eligible.object-kind": "需要检查：此项目是链接或特殊文件，并非数据。",
      "eligible.in-use-partial":
        "Lumen 可见的应用都未使用此项目。Lumen 无法检查系统应用，因此会在移动前再次检查。",
      "eligible.in-use-partial-single-user":
        "Lumen 可见的应用都未使用此项目。你是此电脑的唯一用户，因此 Lumen 无法检查的对象属于系统。移动前会再次检查。",
      "classify.devkb-regenerable": "建议处理：创建此项目的工具可以重新生成它。",
      "classify.devkb-tool-action":
        "需要检查：只能通过该工具自带的清理命令移除此项目，而且操作无法撤销。",
      "classify.devkb-not-regenerable": "需要检查：无法重新生成此项目。",
      "classify.devkb-self-managed": "保留：创建此项目的工具会自动清理它。",
      "classify.devkb-unknown": "需要检查：Lumen 无法加载与此项目匹配的工具信息。",
      "classify.vendor-declared-cache": "需要检查：其制造商表示可以删除此项目。",
      "classify.browser-profile":
        "保留：此项目包含网页浏览器的登录信息、Cookie 或网站数据，因此 Lumen 不会触碰。",
      "classify.app-cache": "建议处理：此项目是应用缓存，应用可以重新生成。",
      "classify.app-state": "保留：此项目包含应用自身的数据、设置或文档，因此 Lumen 不会触碰。",
      "classify.orphan": "需要检查：此项目所属的应用似乎已卸载。",
      "jev.demote": "需要检查：AI 检查建议谨慎处理。",
    } satisfies { readonly [R in RuleId]: string },
  },
};
