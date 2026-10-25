import { REPO_URL } from "../data/release";
import type { Messages } from "./types";

// Korean copy in 해요체: active, positive, no honorific ~시, Lumen's glossary.
export const ko: Messages = {
  lang: "ko",
  otherLanguage: "English",
  description:
    "공간을 이해하고, 안전하게 정리해요. Lumen은 기기의 저장 공간과 파일의 근거를 살펴보는 시험 배포 단계의 오픈 소스 소프트웨어예요.",
  ogDescription:
    "비우는 공간보다 파일의 안전이 먼저예요. 기기 안에서 근거를 살펴보는 시험 배포 단계의 오픈 소스 소프트웨어예요.",
  skip: "본문으로 건너뛰기",
  nav: { label: "사이트", docs: "문서" },
  hero: {
    headline: ["공간을 이해하고,", "안전하게 정리해요."],
    maturity: "시험 배포 단계 · 서명되지 않은 소프트웨어",
    lead: "무엇이 공간을 차지하는지, 무엇을 남겨야 하는지 살펴봐요.",
    download: "릴리스 보기",
    requirements: "macOS 13+ · Windows 10 22H2+ · Linux x86_64",
  },
  product: { pending: "실제 앱 화면을 준비하고 있어요." },
  trust: {
    items: [
      {
        headline: ["비우는 공간보다", "파일의 안전이 먼저예요."],
        text: "자동으로 지우지 않아요. 격리한 파일은 영구 삭제하기 전까지 되돌릴 수 있어요.",
      },
      {
        headline: ["추측 대신", "근거를 확인해요."],
        text: "어떤 파일인지, 무엇이 아직 그 파일을 쓰는지. 확실하지 않으면 그대로 둬요.",
      },
      {
        headline: ["파일은 기기에.", "판단은 안전 규칙으로."],
        text: "파일 내용은 기기에 남아요. AI 검토는 선택할 수 있고, 정리는 안전 규칙이 결정해요.",
      },
    ],
    more: "설계 살펴보기",
  },
  download: {
    title: "모든 다운로드와 설치 방법",
    noScript: [
      "파일은 ",
      { link: "GitHub 릴리스", href: `${REPO_URL}/releases` },
      "에서 받을 수 있어요.",
    ],
    checking: "릴리스를 확인하고 있어요…",
    thisComputer: "너가 다운받아야 할거",
    installer: "설치 프로그램",
    requirements: {
      mac: "macOS 13 이상 · Apple silicon, Intel",
      windows: "Windows 10 22H2 이상, Windows 11 · 64비트 · 관리자 권한 없이 설치",
      linux: "x86_64 · Ubuntu 24.04 이상 또는 비슷한 배포판",
    },
  },
  files: {
    summary: "다른 파일: command line, 체크섬, SBOM",
    intro: [
      "command line ",
      { code: "lumen" },
      "은 ",
      { code: "lumen-<target>.tar.gz" },
      "(macOS Apple silicon, Linux)와 ",
      { code: ".zip" },
      "(Windows)으로 받을 수 있어요. ",
      { code: ".cdx.json" },
      "은 빌드에 들어간 구성 요소 목록(SBOM)이에요.",
    ],
  },
  verify: {
    summary: "내려받은 파일 확인하기",
    introBefore: [],
    introAfter: [
      "를 내려받은 파일과 같은 폴더에 두고, 그 폴더에서 아래 명령을 실행해 주세요. 파일 이름 옆에 ",
      { code: "OK" },
      "가 나오면 돼요.",
    ],
    windowsNote:
      " · 나온 값을 SHA256SUMS에서 같은 파일의 값과 비교해 주세요. 대소문자는 달라도 괜찮아요.",
    provenance: "빌드 증명",
    provenanceNote: [
      " · 이 저장소의 릴리스 워크플로가 만든 파일인지 확인해요. ",
      { link: "GitHub CLI", href: "https://cli.github.com/" },
      "가 있어야 해요.",
    ],
  },
  install: {
    title: "처음 열기",
    intro:
      "Lumen은 아직 코드 서명을 하지 않아서 macOS와 Windows가 처음 실행을 막아요. 한 번만 허용하면 다음부터는 바로 열려요.",
    mac: {
      steps: [
        [{ code: ".dmg" }, "를 열고 Lumen을 ", { ui: "응용 프로그램" }, " 폴더로 옮겨 주세요."],
        ["Lumen을 열어 주세요. 열 수 없다는 창이 나오면 ", { ui: "완료" }, "를 눌러 주세요."],
        [
          { ui: "시스템 설정 › 개인정보 보호 및 보안" },
          "에서 아래쪽 ",
          { ui: "보안" },
          " 부분의 ",
          { ui: "“그래도 열기”" },
          "를 누르고 암호나 Touch ID로 확인해 주세요.",
        ],
        ["Lumen을 다시 열고 ", { ui: "그래도 열기" }, "를 눌러 주세요. 다음부터는 바로 열려요."],
      ],
      aside: [
        "macOS 13, 14에서는 Control-클릭 › ",
        { ui: "열기" },
        "로도 열 수 있어요. macOS 15부터는 위 방법으로 열어 주세요.",
      ],
    },
    fda: {
      summary: "macOS 전체 디스크 접근 권한",
      intro:
        "허용하면 메일, 메시지, 다른 앱의 데이터처럼 보호된 폴더도 검사할 수 있어요. 허용하지 않으면 이런 폴더는 ‘읽지 못함’으로 남고, 그 안의 항목은 그대로 둬요.",
      steps: [
        [
          { ui: "시스템 설정 › 개인정보 보호 및 보안 › 전체 디스크 접근 권한" },
          "을 열어 주세요. Lumen 설정의 버튼으로도 열 수 있어요.",
        ],
        [
          "Lumen을 켜 주세요. 목록에 없으면 ",
          { ui: "+" },
          "를 눌러 응용 프로그램 폴더의 Lumen을 추가해 주세요.",
        ],
        ["Lumen을 종료했다가 다시 열고, 다시 검사해 주세요."],
      ],
      aside: [
        "아직 서명이 없어서, 업데이트한 Lumen을 macOS가 다른 앱으로 볼 수 있어요. 업데이트한 뒤 폴더를 읽지 못하면 목록에서 Lumen을 ",
        { ui: "−" },
        "로 지우고 다시 추가해 주세요.",
      ],
    },
    windows: {
      steps: [
        [
          "브라우저가 다운로드를 경고하면 다운로드 목록의 메뉴에서 ",
          { ui: "유지" },
          "를 골라 주세요.",
        ],
        [
          "설치 프로그램을 실행해 주세요. ",
          { ui: "Windows의 PC 보호" },
          " 창이 나오면 ",
          { ui: "추가 정보" },
          "를 누른 다음 ",
          { ui: "실행" },
          "을 눌러 주세요.",
        ],
        [
          "시작 메뉴에서 Lumen을 일반 사용자 권한으로 열어 주세요. 관리자 권한으로 실행하면 파일을 옮기지 않아요.",
        ],
      ],
    },
    linux: {
      aside: [
        "AppImage가 열리지 않으면 FUSE 2가 필요할 수 있어요(Ubuntu 24.04: ",
        { code: "sudo apt install libfuse2t64" },
        "). Lumen은 일반 사용자로 실행해 주세요. root로 실행하면 파일을 옮기지 않아요.",
      ],
    },
  },
  footer: {
    source: "소스 코드",
    releases: "모든 릴리스",
  },
  copy: "복사",
  client: {
    dateLocale: "ko-KR",
    download: "PC용 다운로드 고르기",
    downloadFor: "{platform}용 다운로드",
    viewOnGitHub: "GitHub에서 개발 과정 보기",
    downloadFromGitHub: "GitHub 릴리스 확인하기",
    getFromGitHub: "GitHub 릴리스 확인하기",
    comingSoon: "첫 공개 릴리스를 준비하고 있어요.",
    comingSoonShort: "아직 공개한 릴리스가 없어요. ",
    loadFailed: "릴리스를 확인하지 못했어요. GitHub에서 직접 확인해 주세요.",
    checkFailed: "릴리스를 확인하지 못했어요. ",
    rateLimited: "GitHub 요청이 많아서 최신 버전을 확인하지 못했어요. ",
    releaseNotes: "릴리스 노트",
    githubReleases: "GitHub 릴리스",
    allFiles: "GitHub 릴리스에서 파일 보기",
    fileHeader: "파일",
    sizeHeader: "크기",
    copied: "복사했어요",
  },
  notFound: { title: "페이지를 찾을 수 없어요", home: "Lumen 다운로드 페이지로 가기" },
};
