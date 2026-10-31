import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

export const ja: Messages = {
  lang: "ja",
  language: "言語",
  description:
    "あらゆるデバイスのストレージを把握できます。安全ルールと任意のAIレビューで、削除を決める前にストレージの状態を確認できます。Lumenはプレアルファ版のソフトウェアです。",
  ogDescription:
    "空き容量より、ファイルの安全を優先します。ローカルファースト、オープンソースのプレアルファ版です。",
  skip: "本文へ移動",
  nav: { label: "サイト", docs: "ドキュメント" },
  hero: {
    headline: ["Storage intelligence", "for every device."],
    lead: "残すものと片付けるものを、AIと一緒に確認できます。",
    download: "リリースを見る",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  showcase: {
    main: "サンプルデータを使ったLumenの「探索」画面です。判断別のストレージ内訳、開発ツールとアプリのキャッシュで整理を提案する9.9 GB、ツール自身で整理する項目のコマンドを表示します。",
  },
  features: {
    title: "Lumenでできること",
    items: [
      {
        title: "容量を使っているものを見つける",
        text: "XcodeのビルドデータやGradleのキャッシュ、アプリのキャッシュ、古いインストーラーを見つけます。スキャン中にファイルを開くことはありません。",
      },
      {
        title: "判断の理由を説明する",
        text: "項目を作成したアプリ、再作成できるか、まだ使われているか。根拠をもとに、維持・確認・整理を判断します。",
      },
      {
        title: "AIがもう一度確認する",
        text: "初回起動時にAIレビューを使うか尋ねます。選ぶまでデータは送信しません。有効にすると、項目のパス・サイズ・経過期間が学校のJebサーバーに送信され、運営者が保存する場合があります。ファイルの内容は送信しません。AIは整理を提案した項目を再確認し、確認が必要な項目が何かを推測してメモを付けます。判断をより慎重にすることしかできません。設定からいつでも変更できます。",
      },
      {
        title: "確認後に整理する",
        text: "ファイルは隔離に移動するので、あとから復元できます。pnpmストアやXcodeシミュレーターは、ツール自身の整理コマンドを確認して実行します。ツールのコマンドで削除したデータは元に戻せません。",
      },
    ],
  },
  trust: {
    items: [
      {
        headline: ["ファイルを最優先に。", "空き容量はその次に。"],
        text: "自動で削除することはありません。隔離したファイルは、完全に削除するまで復元できます。",
      },
      {
        headline: ["根拠を確認。", "推測に頼らない。"],
        text: "ファイルの種類と、まだ使っているものを確認します。確信が持てない場合は、そのまま残します。",
      },
    ],
    more: "アーキテクチャを見る",
  },
  download: {
    title: "ダウンロードとインストール",
    noScript: [
      "ファイルは",
      { link: "GitHubのリリース", href: `${REPO_URL}/releases` },
      "から入手できます。",
    ],
    checking: "リリースを確認中…",
    thisComputer: "<-- ダウンロードが必要です",
    installer: "インストーラー",
    requirements: {
      mac: "macOS 13以降 · Apple siliconとIntel",
      windows: "Windows 10 22H2以降、Windows 11 · 64ビット · 管理者権限は不要",
      linux: "x86_64 · Ubuntu 24.04以降、または同等のディストリビューション",
    },
  },
  files: {
    summary: "その他のファイル: コマンドライン、チェックサム、SBOM",
    intro: [
      "コマンドラインツール ",
      { code: "lumen" },
      " は ",
      { code: "lumen-<target>.tar.gz" },
      " (Apple silicon搭載Mac、Linux) と ",
      { code: ".zip" },
      " (Windows) で配布しています。",
      { code: ".cdx.json" },
      " ファイルには各ビルドの構成要素 (SBOM) が記載されています。",
    ],
  },
  verify: {
    summary: "ダウンロードを検証する",
    introBefore: ["ダウンロードしたファイルと同じフォルダーに"],
    introAfter: [
      "を置き、そのフォルダーで次のコマンドを実行してください。ファイルの結果が",
      { code: "OK" },
      "になれば検証完了です。",
    ],
    windowsNote:
      " · 結果をSHA256SUMS内の同じファイルの行と比較してください。大文字と小文字は区別されません。",
    provenance: "ビルドの来歴",
    provenanceNote: [
      " · このリポジトリのリリースワークフローでファイルがビルドされたことを確認します。",
      { link: "GitHub CLI", href: "https://cli.github.com/" },
      " が必要です。",
    ],
  },
  install: {
    title: "Lumenを初めて開く",
    intro:
      "Lumenはまだコード署名されていないため、macOSとWindowsでは初回起動がブロックされます。一度許可すると、次からは通常どおり開けます。",
    mac: {
      steps: [
        [{ code: ".dmg" }, "を開き、Lumenを", { ui: "アプリケーション" }, "にドラッグします。"],
        ["Lumenを開きます。macOSに開けないと表示されたら、", { ui: "完了" }, "をクリックします。"],
        [
          { ui: "システム設定 › プライバシーとセキュリティ" },
          "を開き、下部の",
          { ui: "セキュリティ" },
          "で",
          { ui: "「このまま開く」" },
          "をクリックし、パスワードまたはTouch IDで確認します。",
        ],
        [
          "Lumenをもう一度開き、",
          { ui: "このまま開く" },
          "をクリックします。次からは通常どおり開けます。",
        ],
      ],
      aside: [
        "macOS 13と14では、Controlキーを押しながらクリックして",
        { ui: "開く" },
        "を選ぶ方法もあります。macOS 15以降では上記の手順を使ってください。",
      ],
    },
    fda: {
      summary: "macOSのフルディスクアクセス",
      intro:
        "許可すると、メール、メッセージ、ほかのアプリのデータなど、保護されたフォルダーもスキャンできます。許可しない場合、それらは読み取れないままになり、Lumenは中の項目に触れません。",
      steps: [
        [
          "「Lumen」の設定ボタン、または",
          { ui: "システム設定 › プライバシーとセキュリティ › フルディスクアクセス" },
          "を開きます。",
        ],
        [
          "Lumenをオンにします。一覧にない場合は、",
          { ui: "+" },
          "から「アプリケーション」にあるLumenを追加します。",
        ],
        ["Lumenを終了して開き直し、もう一度スキャンします。"],
      ],
      aside: [
        "署名がないため、macOSがアップデート後のLumenを別のアプリと認識することがあります。アップデート後にフォルダーを読み取れない場合は、一覧から",
        { ui: "−" },
        "でLumenを削除して、もう一度追加してください。",
      ],
    },
    windows: {
      steps: [
        [
          "ブラウザーがダウンロードに警告を表示したら、ダウンロードメニューで",
          { ui: "保持" },
          "を選びます。",
        ],
        [
          "インストーラーを実行します。",
          { ui: "WindowsによってPCが保護されました" },
          "と表示されたら、",
          { ui: "詳細情報" },
          "をクリックし、",
          { ui: "実行" },
          "を選びます。",
        ],
        [
          "スタートメニューから通常のユーザーとしてLumenを開きます。管理者として実行すると、Lumenはファイルを移動しません。",
        ],
      ],
    },
    linux: {
      aside: [
        "AppImageが起動しない場合、FUSE 2が必要なことがあります (Ubuntu 24.04: ",
        { code: "sudo apt install libfuse2t64" },
        ")。通常のユーザーとして実行してください。rootで実行すると、ファイルは移動されません。",
      ],
    },
  },
  footer: { source: "GitHubリポジトリ", releases: "すべてのリリース", contact: "お問い合わせ" },
  copy: "コピー",
  client: {
    dateLocale: "ja-JP",
    download: "デスクトップ版を選ぶ",
    downloadFor: "{platform}版をダウンロード",
    viewOnGitHub: "GitHubで開発状況を見る",
    downloadFromGitHub: "GitHubのリリースを確認",
    getFromGitHub: "GitHubのリリースを確認",
    comingSoonShort: "公開中のリリースはまだありません。 ",
    loadFailed: "リリース情報を取得できません。GitHubで確認してください。",
    checkFailed: "リリース情報を取得できません。 ",
    rateLimited: "GitHubへのリクエストが多いため、最新バージョンを確認できませんでした。 ",
    releaseNotes: "リリースノート",
    githubReleases: "GitHubのリリース",
    allFiles: "GitHubですべてのファイルを見る",
    fileHeader: "ファイル",
    sizeHeader: "サイズ",
    copied: "コピーしました",
  },
  notFound: { title: "ページが見つかりません", home: "Lumenのダウンロードページへ" },
};
