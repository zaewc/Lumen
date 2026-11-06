import type { ConfidenceView, ItemKindView } from "@lumen/api-client";
import type { Messages } from "./en";
import { countFormat } from "./format";
import type { RuleId } from "./rules";

const count = countFormat("ja");

/** 「1件」「1,204件」。 */
const items = (n: number) => `${count(n)}件`;

/** 「その他1件」「その他3件」。 */
const others = (n: number) => `その他${count(n)}件`;

/** 「Spotify、Discord」または「A、B、C、ほか2件」。 */
function appList(apps: readonly string[]): string {
  if (apps.length > 3) {
    return `${apps.slice(0, 3).join("、")}、ほか${count(apps.length - 3)}件`;
  }
  return new Intl.ListFormat("ja", { type: "conjunction" }).format(apps);
}

/** 日本語カタログ。すべてのカタログで `en` と同じ形を使います。 */
export const ja: Messages = {
  language: "言語",
  nav: {
    label: "メイン",
    overview: "ストレージ",
    cleanUp: "探索",
    quarantine: "隔離",
    settings: "設定",
    held: (n: number) => `隔離中：${items(n)}`,
  },
  common: {
    loading: "読み込み中…",
    close: "閉じる",
    details: "詳細",
    noDetails: "このエラーの詳細情報はありません。",
    tryAgain: "再試行",
    scanDisk: "ディスクをスキャン",
    scanning: "スキャン中…",
    scanAgain: "もう一度スキャン",
    scanOther: "別のフォルダーをスキャン",
    items: (n: number) => items(n),
  },
  overview: {
    title: "ストレージ",
    disk: {
      title: "起動ディスク",
      meter: "ディスク使用量",
      used: (size: string) => `${size}使用中`,
      total: (size: string) => `合計${size}`,
      percentUsed: (percent: string) => `${percent}%使用中`,
      assessed: (size: string) => `確認済み：${size}`,
      low: "空き容量が10%未満です。",
      loadFailed: "ディスク使用量を読み取れませんでした。",
    },
    session: {
      title: "前回のスキャン",
      quarantine: "隔離中",
      atLeast: (size: string) => `${size}以上`,
      found: (n: string) => `${n}読み取り済み`,
      openExplore: "「探索」を開く",
      showProgress: "進行状況を表示",
      cleaned: (n: number, size: string) => `${items(n)}（${size}）を隔離に移動しました。`,
      none: "まだスキャンしていません。ディスクをスキャンして、内容と判断の根拠を確認しましょう。",
    },
    tools: {
      title: "開発ツール",
      intro:
        "Xcode、Docker、パッケージマネージャーに、使っている容量を問い合わせます。Lumen は回答を読むだけです。",
      check: "開発ツールを確認",
      checking: "確認中…",
      checkAgain: "もう一度確認",
      loadFailed: "開発ツールに問い合わせできませんでした。",
      notInstalled: "インストールされていません",
      unsupported: "このコンピュータでは確認できません",
      unavailable: "確認できませんでした",
      none: "報告された項目はありません",
      inUse: (detail: string) => `使用中 ${detail}`,
      reclaimable: (size: string) => `Docker の見積もりでは ${size} が未使用`,
      cache: "キャッシュ",
      sharedStore: "共有ストア",
      ownCommand: "専用コマンド",
      xcodeManage: "ランタイムは Xcode › Settings › Components で管理します。",
      dockerManage:
        "停止したコンテナや未使用のボリュームにも必要なデータがあるかもしれません。Lumen は prune を実行しません。",
      sharedStoreNote:
        "プロジェクトはこのストアにリンクしています。専用コマンドは使用中のパッケージを残します。",
      cacheNote: "必要に応じて再ダウンロードされます。",
    },
    copies: {
      title: "同じ内容のファイル",
      intro:
        "フォルダを選ぶと、その中のファイルを比べて内容が同じものを探します。Lumen が読むのは選んだフォルダのファイルだけです。",
      choose: "比べるフォルダを選ぶ…",
      chooseAgain: "ほかのフォルダを比べる…",
      comparing: "比べています…",
      stage: {
        head: "同じサイズのファイルの先頭を比べています",
        tail: "まだ一致するファイルの末尾を比べています",
        full: "まだ一致するファイルを最後まで比べています",
      },
      filesRead: (n: string) => `${count(n)}個のファイルを読みました`,
      folders: "比べたフォルダ",
      group: (n: number, size: string) => `${count(n)}個のコピー · 各 ${size}`,
      none: "比べられたファイルの中に、同じ内容のファイルはありません。",
      more: (n: number) => `小さいグループ ${count(n)}個は表示していません。`,
      hardLinks: (n: number) =>
        `${count(n)}個のファイルには複数の名前(ハードリンク)があります。コピーではなく 1 つのファイルです。`,
      skipped: (n: number) =>
        `${count(n)}個のファイルは比べていません。変更された、読めない、クラウドにしかない、またはシステムが保護しているファイルです。`,
      protectedFiles: (n: number) =>
        `保護された場所にある ${count(n)}個のファイルは読んでいません。`,
      incomplete: "一部のフォルダを最後まで読めませんでした。そこにあるファイルは比べていません。",
      unsupported: "このコンピュータでは、まだファイルを比べられません。",
      tooMany: (limit: string) =>
        `ファイルが ${count(limit)}個を超えています。もっと小さいフォルダを選んでください。`,
      failed: "フォルダを比べられませんでした。",
      advice:
        "コピーは残しておきたいバックアップかもしれません。Lumen はどれも選んだり削除したりしません。コピーを削除しても、そのサイズ分の空きが増えないことがあります。",
    },
  },
  workspace: {
    available: "空き容量",
    assessed: "確認済み項目 · サイズ順",
    assessment: (n: number) => `${items(n)}を確認`,
    unknownOrigin: "出所の情報なし",
    workflow: "スキャンから整理まで",
    scanStage: "スキャン",
    resultsStage: "結果を確認",
    cleanUpStage: "整理",
    activeCommand: "コマンド実行中",
    quitHeld: "項目を移動または削除しています。完了後にLumenを閉じられます。",
  },
  start: {
    title: "ディスクの中身を確認する",
    promise: "スキャンは読み取りのみで、数分で終わります。項目を移動する前にLumenが確認します。",
    wholeDiskHint: "一部のフォルダーへのアクセス許可を求められることがあります。",
    chooseFolders: "一部のフォルダーだけをスキャン",
    foldersLabel: "スキャンするフォルダー（1行に1つ）",
    foldersEmpty: "フォルダーを1つ以上入力してください。",
    scanFolders: "指定したフォルダーをスキャン",
    failed: "スキャンを完了できませんでした。ファイルは変更されていません。",
    aiOn: (service: string) =>
      `AIレビューは有効です。整理を提案する項目と、一部の確認が必要な項目の詳細情報を、ファイルの内容は含めずに${service}サーバーへ送信します。設定から無効にできます。`,
  },
  ai: {
    title: "AIレビュー",
    offIntro:
      "AIモデルが整理候補をもう一度確認し、確認が必要な項目が何かを推測します。Lumenの判断をより慎重にすることしかできません。",
    onIntro: (key: string) =>
      `有効（${key}）。整理候補と、確認が必要な項目のうち最大20件について、内容は含めずに詳細情報を送信します。Lumenの判断をより慎重にすることしかできません。いつでも無効にできます。`,
    keyInUse: {
      builtin: (service: string) => `${service}（組み込みキー）`,
      stored: (service: string) => `${service}（自分のキー）`,
    },
    checking: "確認中…",
    short: {
      on: "AIレビュー：有効",
      off: "AIレビュー：無効",
    },
    loadFailed: "AIレビューの設定を読み込めませんでした。",
    changeFailed: "AIレビューの設定を変更できませんでした。",
    consent: {
      title: "AIレビューを有効にしますか？",
      what: "整理候補と、確認が必要な項目のうちサイズが大きい最大20件について、パス（ホームフォルダーは「~」と表示）、サイズ、経過期間、判断の根拠を送信します。ファイルの内容は送信しません。",
      whereJeb:
        "APIキーとともに学校のJebサーバー（jeb.https.gsmsv.site）へ送信します。サーバーの運営者がリクエストを保存する場合があります。",
      whereJebBuiltin:
        "組み込みキーを使って学校のJebサーバー（jeb.https.gsmsv.site）へ送信します。サーバーの運営者がリクエストを保存する場合があります。",
      whereOpenRouter: "APIキーとともに、OpenRouter経由でTypeSafeのJevへ送信します。",
      cautious:
        "AIができるのは判断をより慎重にすることだけです。確認が必要な項目には、何である可能性が高いかをメモとして追加します。移動または削除する項目を選ぶことはありません。",
      revocable: "以降のすべてのスキャンでAIレビューを実行します。設定からいつでも無効にできます。",
      keyLabel: "Jeb APIキー",
      keyHint: "システムの安全なストレージに保存し、再表示することはありません。",
      keyInvalid: "キー全体を貼り付けてください（1～256文字、空白なし）。",
      ownKey: "自分のキーを使う",
      ownKeyHint:
        "任意です。組み込みキーの代わりに使用し、システムの安全なストレージに保存します。再表示することはありません。",
      confirm: "AIレビューを有効にする",
      turningOn: "有効にしています…",
    },
    firstLaunch: {
      title: "AIレビューを使いますか？",
      intro:
        "Lumenは独自の安全ルールで判断します。必要に応じてAIモデルがもう一度確認できます。選択するまでは何も送信されません。",
      on: "AIレビューを有効にする",
      off: "AIレビューを使わずに続ける",
    },
  },
  scanning: {
    title: "スキャン中…",
    deciding: "判断中…",
    decidingHint: "各項目が何で、どう扱うべきかを確認しています。何も変更しません。",
    stages: {
      inventory: "インストール済みアプリを確認中…",
      markers: "項目の出所を確認中…",
      evidence: "キャッシュとツールのフォルダーを識別中…",
      graph: "項目と関連するアプリやツールを紐付け中…",
      deciding: "安全ルールを適用中…",
      describing: "AIレビューが確認対象の説明を作成中…",
    },
    stagePercent: (n: number) => `この段階の${n}%`,
    processed: (done: string, total: string) => `${total}件中${done}件を確認`,
    readOnly: "読み取りのみで、何も変更しません。",
    progress: "スキャンの進行状況",
    items: "読み取った項目",
    unreadable: "読み取れないフォルダー",
    reading: "読み取り中",
    readSoFar: (size: string) => `読み取り済み：${size}`,
    failed: "見つかった項目を判断できませんでした。ファイルは変更されていません。",
  },
  results: {
    map: {
      label: "確認済み項目のサイズ別マップ",
      all: "すべての種類",
      smaller: (n: number) => `小さい項目${count(n)}件`,
      notRead: "未読み取り",
      notReadYet: "未読み取り",
      entries: (n: number) => `${count(n)}件`,
    },
    canFree: (size: string) => `${size}を整理できます`,
    nothingTitle: "整理できる項目はありません",
    nothing: "アプリが再作成できるキャッシュなど、安全ルールで許可された項目だけを提案します。",
    filters: {
      all: "すべて",
    },
    groups: {
      command: "専用コマンドで整理",
      developer: "開発者ツール",
      app_cache: "アプリのキャッシュ",
      app_data: "アプリデータ",
      other: "その他",
    },
    otherTools: "その他のツール",
    commandNote:
      "これらは各ツールでのみ削除してください。項目を開いてコマンドを実行できます。元に戻せないため、実行前に削除内容を確認してください。",
    status: {
      quarantine: "整理候補",
      command: "専用コマンド",
      review: "要確認",
      keep: "維持",
    },
    columns: {
      name: "名前",
      location: "場所",
      status: "状態",
      size: "サイズ",
    },
    listHint: "上下の矢印キーで項目を移動します。",
    emptyFilter: "この状態の項目はありません。",
    partial: "スキャンは途中で停止しました。停止するまでに見つかった項目を表示しています。",
    whenClosed: (size: string, ...apps: string[]) =>
      apps.length === 0
        ? `維持対象の一部を使用中のアプリを終了し、もう一度スキャンすると、さらに${size}を整理できます。`
        : `${appList(apps)}を終了し、もう一度スキャンすると、さらに${size}を整理できます。`,
    outOfSpace:
      "ディスクの空き容量が少なくなったため、スキャンを停止しました（Lumenは常に2 GBを空けておきます）。停止するまでに見つかった項目を表示しています。空き容量を確保して再度スキャンしてください。",
  },
  action: {
    cleanUp: "整理…",
    preparing: "準備中…",
    nothing: "整理候補はありません。",
    planFailed: "整理を準備できませんでした。ファイルは変更されていません。",
  },
  detail: {
    size: "サイズ",
    location: "場所",
    decision: "判断",
    reasons: "根拠",
    aiNoteHint: "AIレビューによる推測です。Lumenの判断は変わりません。",
    whenClosed: (...apps: string[]) =>
      apps.length === 0
        ? "使用中のアプリを終了し、もう一度スキャンして整理してください。"
        : `${appList(apps)}を終了し、もう一度スキャンして整理してください。`,
    relatedNone: "ほかの項目との関連は記録されていません。",
    sources: {
      rule: "ルール",
      location: "場所",
      time: "日時",
      relation: "関連",
      process: "プロセス",
      ai: "AIレビュー",
    },
    owner: (app: string, present: boolean | null) =>
      present === null
        ? `${app}に属します。インストールされているかは不明です。`
        : `${app}に属します。${present ? "インストールされています" : "インストールされていません"}。`,
    inUse: (inUse: boolean): string =>
      inUse ? "アプリが使用中です。" : "使用中のアプリはありません。",
    unseen: (unseen: number | null) =>
      unseen === null
        ? "確認できるアプリで使用されていません。"
        : `確認できるアプリで使用されていません。確認できなかったプロセスが${count(unseen)}件あります。`,
    changed: (date: string, folder: boolean) =>
      folder
        ? `フォルダー項目の最終変更日は${date}です。中身はその後に変更された可能性があります。`
        : `最終変更日：${date}。`,
    opened: (date: string, folder: boolean) =>
      `${folder ? "フォルダーの最終読み取り：" : "最終使用："}${date}以降。システムがすべての使用状況を記録するとは限りません。`,
    notRecorded: (which: "modified" | "accessed"): string =>
      which === "modified"
        ? "最終変更日時は記録されていません。"
        : "最終使用日時は記録されていません。",
    notRead: (which: "modified" | "accessed"): string =>
      which === "modified"
        ? "最終変更日時は不明です（未読み取り）。"
        : "最終使用日時は不明です（未読み取り）。",
    means: {
      keep: "維持します。何も移動しません。",
      review: "判断はお任せします。Lumenが自動で移動することはありません。",
      command: "専用ツールで整理してください。依頼された場合にのみ、Lumenがコマンドを実行します。",
      quarantine: "隔離への移動を提案しています。隔離から復元できます。",
    },
    lens: {
      tool: (n: number, tool: string, size: string) => `${tool}由来の${others(n)} · ${size}`,
      processes: (n: number, size: string, ...apps: string[]) =>
        `${appList(apps)}も使用中の${others(n)} · ${size}`,
      evidence: (n: number, size: string) => `スキャン記録で関連付けられた${others(n)} · ${size}`,
      kind: (n: number, group: string, size: string) => `${group}の${others(n)} · ${size}`,
    },
  },
  aiNote: {
    likely: (kind: string, confidence: string) => `AI：${kind}の可能性（${confidence}）`,
    unknown: "AI：種類を判別できませんでした",
    kinds: {
      app_settings: "アプリの設定またはデータ",
      user_documents: "ユーザーの書類",
      cache: "キャッシュ",
      logs: "ログ",
      downloads_or_installers: "ダウンロードまたはインストーラー",
      developer_tool_data: "開発者ツールのデータ",
      backup_or_archive: "バックアップまたはアーカイブ",
      unknown: "判別できない項目",
    } satisfies { readonly [K in ItemKindView]: string },
    confidence: {
      low: "確信度：低",
      medium: "確信度：中",
      high: "確信度：高",
    } satisfies { readonly [C in ConfidenceView]: string },
  },
  toolCommand: {
    homebrewDownloads:
      "Homebrewは古いインストール済みバージョンと最近のダウンロードキャッシュを整理します。今後のインストールで、これらのファイルを再ダウンロードする場合があります。",
    title: (tool: string) => `${tool}で整理できます`,
    copy: "コピー",
    copied: "コピーしました",
    copyFailed: "コピーできませんでした。コマンドを選択して手動でコピーしてください。",
    note: "Lumenはこのコマンドを実行しません。実行前に削除内容を確認してください。",
    run: "コマンドを実行…",
    runConfirm: "コマンドを実行",
    runWarning:
      "このコマンドはファイルをすぐに削除します。隔離を経由しないため、Lumenで復元できません。実行前に削除内容を確認してください。",
    removesData:
      "Dockerの停止中コンテナー、未使用ネットワーク、danglingイメージ、未使用のビルドキャッシュを削除します。",
    running: "実行中…数分かかることがあります。",
    succeeded: "コマンドが完了しました。もう一度スキャンして残りを確認してください。",
    failed: (code: number | null) =>
      code === null ? "コマンドに失敗しました。" : `コマンドに失敗しました（終了コード${code}）。`,
    timedOut: "15分経っても実行中だったため、Lumenが停止しました。",
    output: "出力を表示",
    runFailed: "コマンドを実行できませんでした。",
    runNote: "実行すると元に戻せません。実行前に削除内容を確認してください。",
  },
  spaceChange: {
    estimate: (size: string) => `選択した項目：推定${size}。`,
    increased: (size: string) => `確認された空き容量が${size}増えました。`,
    decreased: (size: string) => `確認された空き容量が${size}減りました。`,
    unchanged: "空き容量に変化はありません。",
    unknown: "空き容量の変化を測定できませんでした。",
    multiple: "各ディスクの空き容量を測定しました。",
    shared:
      "複数のボリュームで空き容量を共有している場合があるため、測定値を個別に表示しています。",
    details: "測定値を見る",
    disk: (index: number) => `ディスク${index}`,
    readings: (before: string, after: string) => `${before} → ${after}利用可能`,
    note: "ほかのアプリも測定値に影響します。空き容量が回復するまで時間がかかる場合があります。",
  },
  coverage: {
    incomplete:
      "一部のフォルダーを読み取れず、そのままにしました。実際のサイズは表示より大きい場合があります。",
    howTo: "フルディスクアクセスを許可する方法",
  },
  details: {
    title: "スキャンの詳細",
    entries: (entries: string) => `${count(entries)}件を読み取り`,
    files: "ファイル",
    fileCount: (files: string) => count(files),
    size: "サイズ",
    sizeValue: (logical: string, allocated: string) => `${logical}（ディスク上では${allocated}）`,
    reclaimable: "空けられる容量",
    atLeast: (size: string) => `${size}以上`,
    complete: "すべて読み取りました。",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `すべてを読み取れませんでした：アクセス拒否${count(denied)}件、失敗${count(failed)}件、未スキャン${count(notScanned)}件。`,
  },
  confirm: {
    title: (n: number, size: string) => `${items(n)}（${size}）を隔離に移動しますか？`,
    checked: "各項目をもう一度確認し、スキャン後に変更された項目はそのまま残します。",
    restorable: "隔離から少なくとも30日間は復元できます。完全に削除すると復元できなくなります。",
    nothingLeft: "移動する項目はありません。もう一度スキャンして整理できる項目を確認してください。",
    items: "移動する項目",
    move: "隔離に移動",
    moving: "移動中…",
    failed: "整理を実行できませんでした。",
    planMade: (createdAt: string) => `この計画は${createdAt}に作成されました。24時間利用できます。`,
    planHash: "計画のハッシュ",
  },
  done: {
    title: (n: number, size: string) => `${items(n)}（${size}）を隔離に移動しました。`,
    noneMoved: "何も移動しませんでした。",
    freedNote: "隔離から完全に削除すると、空き容量が戻ります。",
    openQuarantine: "隔離を開く",
    notMoved: (n: number) => `移動されなかった項目：${items(n)}`,
  },
  item: {
    quarantined: "隔離に移動しました。復元できます。",
    skipped: (why: string) => `移動しませんでした：${why}。`,
    failed: (reason: string, where: string) => `移動に失敗しました：${reason}；${where}。`,
    failIdentityMismatch: "スキャン後に変更されたため",
    failInUse: "プログラムが使用中のため",
    failVolumeUnsupported: "このドライブでは隔離を保持できないため",
    failMoveFailed: "移動中に問題が発生したため",
    failVerificationFailed: "移動後の状態が計画と一致しなかったため",
    failOther: "このバージョンでは原因を特定できないため",
    skipChanged: "変更されたか、確認できなくなったため",
    skipOutsideRoots: "スキャン対象のフォルダー外にあるため",
    skipNowKeep: "現在は維持する判断となったため",
    skipNowReview: "現在は確認が必要な判断となったため",
    skipReasonsChanged: "現在は別の根拠で提案されるため",
    skipNotPrepared: "安全に移動する記録を作成できなかったため",
    skipManifest: "記録を書き込めなかったため",
    leftSource: "元の場所にそのまま残っています",
    leftQuarantine: "隔離にあり、復元できます",
    leftGone: "なくなっています",
  },
  quarantine: {
    title: "隔離",
    atLeast: (size: string) => `${size}以上`,
    columns: {
      name: "名前",
      location: "元の場所",
      size: "サイズ",
      held: "隔離期間",
    },
    loadFailed: "隔離を読み込めませんでした。",
    empty: "隔離は空です",
    emptyHint: "整理した項目はここに保管されるため、考えが変わっても元に戻せます。",
    goCleanUp: "「探索」を開く",
    interrupted: (n: number) =>
      `整理が途中で停止し、${n === 1 ? "この項目" : `この${count(n)}件の項目`}がどこにあるか確認できません。削除はしていません。${n === 1 ? "項目" : "項目"}の場所を確認してください：`,
    selection: {
      all: "すべて選択",
      item: (name: string) => `${name}を選択`,
      count: (n: number, size: string) => `${items(n)}を選択 · ${size}`,
      clear: "選択を解除",
      restore: "復元",
      removeAsk: "完全に削除…",
      removeWarning: (n: number) =>
        `${items(n)}を完全に削除します。元に戻せません。各項目が使用中でないことを最初に確認します。`,
      removeSkips: (n: number) => `復元のみ可能な${items(n)}は隔離に残ります。`,
      remove: "完全に削除",
      working: (done: number, total: number) => `処理中…${total}件中${count(done)}件`,
      restored: (well: number, total: number) =>
        well === total
          ? `${items(total)}を復元しました。`
          : `${total}件中${count(well)}件を復元しました。残りは引き続き隔離にあります。`,
      removed: (well: number, total: number) =>
        well === total
          ? `${items(total)}を完全に削除しました。`
          : `${total}件中${count(well)}件を完全に削除しました。それ以外は削除していません。`,
      restoredStopped: (well: number, total: number, untried: number) =>
        [
          `${total}件中${count(well)}件を復元した後、エラーで停止しました。`,
          untried > 0 ? `残り${count(untried)}件は処理していません。` : null,
          "一覧に残っている項目は隔離に保管されています。",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
      unverified: (n: number) =>
        n === 1
          ? "復元した項目が記録と完全には一致しません。確認してください。"
          : `復元した${count(n)}件の項目が記録と完全には一致しません。確認してください。`,
      removedStopped: (well: number, total: number, untried: number) =>
        [
          `${total}件中${count(well)}件を完全に削除した後、エラーで停止しました。`,
          untried > 0 ? `残り${count(untried)}件は処理していません。` : null,
          "一覧に残っている項目は隔離に保管され、復元できます。",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
    },
  },
  held: {
    unknownPath: "（場所不明）",
    days: (days: number) => (days === 0 ? "今日" : `${count(days)}日`),
    restoreOnly: "復元のみ",
    afterFailure: "移動中に問題が発生したため、安全のためここに保管しました。復元のみ可能です。",
    restore: "復元",
    removeForGoodAsk: "完全に削除…",
    removeForGoodWarning: "完全に削除します。元に戻せません。使用中でないことを最初に確認します。",
    removeForGood: "完全に削除",
    failed: "処理できませんでした。もう一度お試しください。",
  },
  restore: {
    restored: "元の場所に復元しました。",
    parentReplaced: "元のフォルダーはその後に作り直されています。",
    unverified: "記録と完全には一致しません。確認してください。",
    notRestored: (why: string) => `復元できませんでした。隔離に残っています。${why}。`,
    nameTaken: "元の場所に同じ名前の項目があります。そちらを移動してから、もう一度お試しください。",
    originUnreachable: "元のフォルダーに安全にアクセスできません。",
    itemChanged: "隔離中に変更されました。",
    moveFailed: "元の場所への移動に失敗しました。",
    notRestorable: "現在の状態では、この項目を復元できません。",
  },
  finalize: {
    finalized: "完全に削除しました。",
    foundAtSource: "すでに元の場所に戻っています。削除していません。",
    notFound:
      "隔離内に見つかりませんでした。別のドライブに保管されていた場合は、そのドライブを接続して再試行してください。削除していません。",
    failed: (what: string) => `完全に削除できませんでした。${what}。`,
    removedNothing: "何も削除していません",
    removedPartly: "一部が削除されています。完了するにはもう一度お試しください",
    removedItem: "項目は削除され、記録だけが残っています",
    maybeInUse:
      "使用中か、使用中かどうかを確認できないため、削除していません。使用中のアプリを終了して再試行してください。",
    notFinalizable: "この項目は復元のみ可能です。",
  },
  settings: {
    title: "設定",
    appearance: {
      title: "テーマ",
      system: "システム設定に合わせる",
      light: "ライト",
      dark: "ダーク",
      failed: "ウィンドウの外観を変更できませんでした。",
    },
    permissions: {
      title: "アクセス許可",
      fda: "フルディスクアクセス",
      fdaIntro:
        "macOSの「システム設定」でアクセス許可を確認できます。Lumenに許可済みなら、この手順は省略できます。許可があっても、一部の保護されたフォルダーは読み取れない場合があります。",
      steps: "設定手順を表示",
      step1: "「システム設定」を開きます。",
      step2: "「プライバシーとセキュリティ › フルディスクアクセス」に進みます。",
      step3: "Lumenをオンにします。すでにオンの場合はそのままにしてください。",
      step4: "設定を変更した場合は、Lumenを完全に終了して開き直し、新しいスキャンを開始します。",
      open: "「システム設定」を開く",
      openFailed: "「システム設定」を開けませんでした。Appleメニューから開いてください。",
      access: "フォルダーへのアクセス",
      accessIntro:
        "コンピューターが一部のフォルダーへのアクセスをブロックすることがあります。許可されたフォルダーはすべて読み取ります。",
      unread:
        "読み取れないフォルダーは「未読み取り」と表示し、空として扱いません。その中の項目は提案しません。",
    },
    about: {
      title: "Lumenについて",
      version: "バージョン",
      local: "このデバイス内",
      localBody:
        "スキャン、判断、隔離データはこのコンピューター内に保管されます。ファイルの内容を外部に送信することはありません。AIレビューが有効な場合も、パスやサイズなどの詳細情報のみを送信します。",
      update: {
        title: "アップデート",
        checking: "新しいバージョンを確認中…",
        upToDate: "Lumenは最新です。",
        available: (version: string) => `バージョン${version}を利用できます。`,
        packageOnly:
          "パッケージからインストールされたアプリです。zaewc.github.io/Lumenから新しいバージョンを入手してください。",
        checkFailed: "新しいバージョンを確認できませんでした。",
        check: "今すぐ確認",
        install: "アップデートして再起動",
        installing: "アップデート中…",
        installingNote:
          "新しいバージョンをダウンロードし、署名を確認してインストール後、再起動します。",
        installFailed:
          "アップデートをインストールできませんでした。現在のバージョンに変更はありません。",
        waitForItems: "項目の移動または削除が完了するとアップデートできます。",
        sidebar: (version: string) => `バージョン${version}を利用できます`,
        notice: (version: string) => `Lumen ${version}を利用できます。`,
        noticeAction: "アップデートを確認",
        later: "後で",
      },
    },
  },
  whatIs: (term: string) => `「${term}」とは？`,
  glossary: {
    verdicts: {
      title: "Lumenの判断方法",
      body: "各項目は推測ではなく、固定された安全ルールで判断します。「整理候補」：アプリやツールが再作成できるキャッシュなど。「専用コマンドで整理」：ツール自身でのみ削除すべき項目。実行するコマンドを表示します。「要確認」：確信が持てない大きな項目や、アンインストール済みアプリの残り。自動で移動することはありません。「維持」：システムファイル、使用中のファイル、アプリ固有のデータ、認識できない小さな項目など、Lumenが触れない項目。判断に迷う場合は、そのまま残します。",
    },
    quarantine: {
      title: "隔離",
      body: "隔離は項目を安全に保管する場所です。整理では削除せずにここへ移動し、完全に削除するまでは元の場所に復元できます。隔離中もディスク容量を使用します。",
    },
  },
  reviewReasons: {
    unknownOwner: "どのアプリやツールの項目か確認できませんでした。",
    sharedResource:
      "ほかのファイルと領域を共有しているか、この項目の外にもリンクがある可能性があります。",
    staleEvidence: "確認した根拠が古くなっています。もう一度スキャンしてから判断してください。",
    uncertainSize: "完全に削除したあとに空く領域は確かではありません。",
  },
  reasons: {
    location: (name: string) => `維持：保護された場所（${name}）にあります。`,
    rules: {
      "default.review": "要確認：この項目を識別できません。",
      "default.keep-small": "維持：確認するほどのサイズではありません。",
      "protect.os-protected": "維持：オペレーティングシステムによって保護されています。",
      "protect.system-file": "維持：Windowsがシステムファイルとして識別しています。",
      "protect.cloud-backed":
        "維持：クラウドサービスと同期されています。ここで削除すると、ほかの場所からも削除される可能性があります。",
      "protect.in-use": "維持：実行中のアプリが使用しています。",
      "protect.in-use-partial":
        "維持：確認できるアプリは使用していませんが、すべてのアプリを確認できませんでした。",
      "protect.in-use-unknown": "維持：使用中かどうかを確認していません。",
      "protect.not-observed": "維持：スキャンでこの項目を確認できませんでした。",
      "protect.incomplete-scan": "維持：一部を読み取れず、中身が不明です。",
      "protect.location-data": "維持：保護された場所の一覧を確認できませんでした。",
      "protect.location-context": "維持：保護された場所の一覧を確認できませんでした。",
      "protect.contains-system": "維持：オペレーティングシステムの一部が含まれています。",
      "protect.contains-sensitive":
        "維持：パスワード、キー、またはWebブラウザーのサインイン情報が含まれているため、触れません。",
      "protect.contains-cloud":
        "維持：クラウド同期フォルダーが含まれています。ここで削除すると、中のファイルがすべての場所から削除される可能性があります。",
      "eligible.quarantine-unsupported":
        "要確認：このデバイスでは、元に戻せる形で隔離に移動できません。",
      "eligible.frees-nothing": "維持：削除しても空き容量は増えません。",
      "eligible.object-kind": "要確認：データではなく、リンクまたは特殊ファイルです。",
      "eligible.in-use-partial":
        "確認できるアプリは使用していません。システムアプリを確認できなかったため、移動直前にもう一度確認します。",
      "eligible.in-use-partial-single-user":
        "確認できるアプリは使用していません。このコンピューターのユーザーはあなたのみのため、確認できなかった対象はシステムアプリです。移動直前にもう一度確認します。",
      "classify.devkb-regenerable": "整理候補：作成元のツールで再作成できます。",
      "classify.devkb-tool-action":
        "要確認：ツール専用の整理コマンドでのみ削除できます。実行すると元に戻せません。",
      "classify.devkb-not-regenerable": "要確認：再作成できません。",
      "classify.devkb-self-managed": "維持：作成元のツールが自動的に整理します。",
      "classify.devkb-unknown": "要確認：該当するツール情報を読み込めませんでした。",
      "classify.vendor-declared-cache": "要確認：提供元は削除可能としています。",
      "classify.browser-profile":
        "維持：Webブラウザーのサインイン情報、Cookie、サイトデータが含まれているため、触れません。",
      "classify.app-cache": "整理候補：アプリのキャッシュであり、アプリが再作成できます。",
      "classify.app-state": "維持：アプリ固有のデータ、設定、書類が含まれているため、触れません。",
      "classify.orphan": "要確認：この項目に関連するアプリはアンインストール済みのようです。",
      "jev.demote": "要確認：AIレビューが慎重な判断を勧めました。",
    } satisfies { readonly [R in RuleId]: string },
  },
};
