import type { Messages } from "./en";
import { countFormat } from "./format";

const count = countFormat("ko");

/** "Spotify, Discord"; past three names, "A, B, C 외 2개". */
function appList(apps: readonly string[]): string {
  const shown = apps.slice(0, 3).join(", ");
  return apps.length > 3 ? `${shown} 외 ${count(apps.length - 3)}개` : shown;
}

/**
 * The particle "이란" after a final consonant and "란" after a vowel
 * (‘격리함’이란, ‘영구 삭제’란), read from the last Hangul syllable.
 */
function withIran(word: string): string {
  const last = (word.codePointAt(word.length - 1) ?? 0) - 0xac00;
  return last >= 0 && last < 11_172 && last % 28 === 0 ? "란" : "이란";
}

/** "항목 1,204개". */
const items = (n: number) => `항목 ${count(n)}개`;

/**
 * The Korean catalog, checked against the English one's shape. It speaks
 * 해요체 throughout (docs/design/ux-writing.md): active, positive, no "~시"
 * honorifics, and "닫기" to dismiss a dialog.
 */
export const ko = {
  language: "언어",
  nav: {
    label: "주 메뉴",
    overview: "저장 공간",
    cleanUp: "탐색",
    quarantine: "격리함",
    apps: "앱",
    settings: "설정",
    held: (n: number) => `격리함에 ${items(n)}`,
  },
  common: {
    loading: "불러오는 중…",
    close: "닫기",
    details: "자세히",
    noDetails: "이 오류는 자세한 내용이 없어요.",
    tryAgain: "다시 시도",
    scanDisk: "디스크 검사",
    scanning: "검사하는 중…",
    scanAgain: "다시 검사",
    scanOther: "다른 폴더 검사",
    items: (n: number) => items(n),
  },
  overview: {
    title: "저장 공간",
    disk: {
      title: "시동 디스크",
      meter: "디스크 사용량",
      used: (size: string) => `사용 ${size}`,
      total: (size: string) => `전체 ${size}`,
      percentUsed: (percent: string) => `${percent}% 사용 중`,
      assessed: (size: string) => `살펴본 항목 ${size}`,
      low: "디스크가 거의 찼어요. 남은 공간이 10%보다 적어요.",
      loadFailed: "디스크 사용량을 읽지 못했어요.",
    },
    session: {
      title: "최근 검사",
      quarantine: "격리함",
      atLeast: (size: string) => `최소 ${size}`,
      found: (n: string) => `${n}개 읽었어요`,
      openExplore: "탐색 열기",
      showProgress: "진행 상황 보기",
      cleaned: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮겼어요.`,
      none: "아직 검사하지 않았어요. 디스크를 검사하면 어떤 항목이 있는지와 판단 근거를 보여 줘요.",
    },
    tools: {
      title: "개발 도구",
      intro:
        "Xcode, Docker, 패키지 관리자에게 저장 공간을 얼마나 쓰는지 물어봐요. Lumen은 답을 읽기만 해요.",
      check: "개발 도구 확인하기",
      checking: "확인하는 중…",
      checkAgain: "다시 확인하기",
      loadFailed: "개발 도구에 물어보지 못했어요.",
      notInstalled: "설치돼 있지 않아요",
      unsupported: "이 컴퓨터에서는 확인할 수 없어요",
      unavailable: "확인하지 못했어요",
      none: "알려 준 항목이 없어요",
      inUse: (detail: string) => `사용 중 ${detail}`,
      reclaimable: (size: string) => `Docker는 ${size}를 쓰지 않는다고 봐요`,
      cache: "캐시",
      sharedStore: "공유 저장소",
      ownCommand: "명령으로 정리",
      xcodeManage: "런타임은 Xcode › Settings › Components에서 관리해요.",
      dockerManage:
        "Docker가 쓰지 않는다고 보는 공간에는 볼륨과 멈춘 컨테이너도 들어 있어서, 필요한 데이터가 있을 수 있어요. Docker 정리는 탐색에서 전체 명령을 확인한 뒤에만 실행하고, 볼륨은 남겨요.",
      sharedStoreNote:
        "프로젝트들이 이 저장소를 연결해 써요. 이 도구의 명령은 아직 쓰는 패키지를 남겨요.",
      cacheNote: "필요하면 다시 내려받아요.",
    },
    copies: {
      title: "똑같은 파일",
      intro:
        "검사가 끝나면 별도 작업으로 검사한 경로의 파일을 읽어 내용이 같은 파일을 찾아요. 이 비교는 읽기만 하며, 어느 파일도 고르거나 지우지 않아요.",
      compareAgain: "다시 비교",
      stop: "멈추기",
      stopping: "멈추는 중…",
      stopped: "비교를 멈췄어요. 멈춘 비교의 결과는 보여 주지 않아요.",
      comparing: "비교하는 중…",
      stage: {
        head: "크기가 같은 파일의 앞부분을 비교하고 있어요",
        tail: "아직 같은 파일의 끝부분을 비교하고 있어요",
        full: "아직 같은 파일을 끝까지 비교하고 있어요",
      },
      filesRead: (n: string) => `파일 ${count(n)}개를 읽었어요`,
      folders: "검사에서 비교한 경로",
      group: (n: number, size: string) => `똑같은 파일 ${count(n)}개 · 각 ${size}`,
      none: "비교한 파일 중에 똑같은 파일이 없어요.",
      more: (n: number) => `더 작은 묶음 ${count(n)}개는 보여 주지 않았어요.`,
      hardLinks: (n: number) =>
        `파일 ${count(n)}개는 이름이 여러 개예요(하드 링크). 사본이 아니라 파일 하나예요.`,
      skipped: (n: number) =>
        `파일 ${count(n)}개는 비교하지 않았어요. 바뀌었거나, 읽을 수 없거나, 클라우드에만 있거나, 시스템이 보호하는 파일이에요.`,
      protectedFiles: (n: number) => `보호된 위치에 있는 파일 ${count(n)}개는 읽지 않았어요.`,
      incomplete: "일부 폴더를 끝까지 읽지 못했어요. 그 안의 파일은 비교하지 않았어요.",
      unsupported: "이 컴퓨터에서는 아직 파일을 비교할 수 없어요.",
      failed: "검사한 경로의 파일을 비교하지 못했어요.",
      advice:
        "사본은 남겨 두고 싶은 백업일 수 있어요. Lumen은 어느 것도 고르거나 지우지 않아요. 사본을 지워도 그 크기보다 적게 비워질 수 있어요.",
    },
  },
  workspace: {
    available: "사용할 수 있는 공간",
    assessed: "이번 검사에서 살펴본 항목 · 크기 기준",
    assessment: (n: number) => `${items(n)}를 살펴봤어요`,
    unknownOrigin: "출처 정보가 없어요",
    workflow: "검사부터 정리까지",
    scanStage: "검사",
    resultsStage: "결과 보기",
    cleanUpStage: "정리",
    activeCommand: "명령 실행 중",
    quitHeld: "Lumen이 항목을 옮기거나 지우고 있어요. 끝나면 닫을 수 있어요.",
  },
  start: {
    title: "디스크에 무엇이 있는지 살펴봐요",
    promise: "검사는 읽기만 하고, 몇 분 걸려요. 무엇이든 옮기기 전에 먼저 물어볼게요.",
    wholeDiskHint: "일부 폴더는 컴퓨터가 접근을 허용할지 물어볼 수 있어요.",
    chooseFolders: "일부 폴더만 검사",
    foldersLabel: "검사할 폴더(한 줄에 하나씩)",
    foldersEmpty: "폴더를 하나 이상 입력해 주세요.",
    scanFolders: "입력한 폴더 검사",
    failed: "검사를 끝내지 못했어요. 파일은 모두 그대로 있어요.",
    aiOn: (service: string) =>
      `AI 검토가 켜져 있어요. 정리를 제안한 항목과 검토할 항목 일부의 정보를 파일 내용은 빼고 ${service} 서버로 보내요. 설정에서 끌 수 있어요.`,
  },
  ai: {
    title: "AI 검토",
    offIntro:
      "Lumen이 정리를 제안한 항목을 AI가 한 번 더 살펴보고, 검토할 항목이 무엇인지 알려 줘요. AI는 Lumen을 더 신중하게 만들 수만 있어요.",
    onIntro: (key: string) =>
      `켜져 있어요(${key}). 정리를 제안한 항목과 검토할 항목 최대 20개의 정보를 파일 내용은 빼고 보내요. AI는 Lumen을 더 신중하게 만들 수만 있어요. 언제든 끌 수 있어요. 설치된 앱의 이름, 게시자, 버전도 경로 없이 보내요.`,
    keyInUse: {
      builtin: (service: string) => `${service}, Lumen 기본 키`,
      stored: (service: string) => `${service}, 내 키`,
    },
    checking: "확인하는 중…",
    short: {
      on: "AI 검토 켜짐",
      off: "AI 검토 꺼짐",
    },
    loadFailed: "AI 검토 설정을 불러오지 못했어요.",
    changeFailed: "AI 검토 설정을 바꾸지 못했어요.",
    consent: {
      title: "AI 검토를 켤까요?",
      what: "정리를 제안한 항목과 검토할 항목 중 큰 순서로 최대 20개의 경로(홈 폴더는 ~로 표시), 크기, 경과 기간, 판단 근거를 보내요. 파일 내용은 보내지 않아요. 앱 화면을 위해 설치된 앱의 이름, 게시자, 버전과 연결된 백그라운드 항목도 경로 없이 보내요.",
      whereJeb:
        "API 키와 함께 학교의 Jeb 서버(jeb.https.gsmsv.site)로 보내요. 서버 운영자가 요청을 보관할 수 있어요.",
      whereJebBuiltin:
        "Lumen 기본 키를 써서 학교의 Jeb 서버(jeb.https.gsmsv.site)로 보내요. 서버 운영자가 요청을 보관할 수 있어요.",
      whereOpenRouter: "API 키와 함께 OpenRouter를 거쳐 TypeSafe의 Jev로 보내요.",
      cautious:
        "AI는 판단을 더 신중하게 바꿀 수만 있어요. 검토할 항목에는 무엇인지 메모만 붙여요. 무엇을 옮기거나 삭제할지는 AI가 정하지 않아요.",
      revocable: "켜 두면 검사할 때마다 AI 검토를 해요. 설정에서 언제든 끌 수 있어요.",
      keyLabel: "Jeb API 키",
      keyHint: "시스템의 보안 저장소에 보관하고, 다시 보여 주지 않아요.",
      keyInvalid: "키 전체를 붙여 넣어 주세요(공백 없이 1~256자).",
      ownKey: "내 키 쓰기",
      ownKeyHint:
        "넣지 않아도 돼요. 넣으면 Lumen 기본 키 대신 이 키를 써요. 시스템의 보안 저장소에 보관하고, 다시 보여 주지 않아요.",
      confirm: "AI 검토 켜기",
      turningOn: "켜는 중…",
    },
    firstLaunch: {
      title: "AI 검토를 쓸까요?",
      intro:
        "Lumen은 안전 규칙으로 판단해요. 원하면 AI가 한 번 더 살펴봐요. 고르기 전에는 아무것도 보내지 않아요.",
      on: "AI 검토 켜기",
      off: "AI 검토 없이 쓰기",
    },
  },
  scanning: {
    title: "검사하는 중…",
    deciding: "판단하는 중…",
    decidingHint: "항목마다 무엇인지, 어떻게 할지 살펴보고 있어요. 아무것도 바꾸지 않아요.",
    stages: {
      inventory: "설치된 앱을 확인하는 중…",
      markers: "항목의 출처를 확인하는 중…",
      evidence: "캐시와 도구 폴더를 알아보는 중…",
      graph: "항목을 앱과 도구에 연결하는 중…",
      deciding: "안전 규칙으로 판단하는 중…",
      describing: "AI 검토로 항목 설명을 받는 중…",
    },
    stagePercent: (n: number) => `이 단계 ${n}%`,
    processed: (done: string, total: string) => `${total}개 중 ${done}개 확인`,
    readOnly: "읽기만 하고, 아무것도 바꾸지 않아요.",
    progress: "검사 진행 상황",
    items: "읽은 항목",
    unreadable: "읽지 못한 폴더",
    reading: "읽는 중",
    readSoFar: (size: string) => `지금까지 읽은 크기 ${size}`,
    failed: "검사한 항목을 판단하지 못했어요. 파일은 모두 그대로 있어요.",
  },
  results: {
    map: {
      label: "이번 검사에서 살펴본 항목 지도, 크기 기준",
      all: "모든 종류",
      smaller: (n: number) => `작은 항목 ${count(n)}개`,
      notRead: "읽지 못함",
      notReadYet: "아직 읽지 않음",
      entries: (n: number) => `${count(n)}개`,
    },
    canFree: (size: string) => `${size}를 정리할 수 있어요`,
    nothingTitle: "정리할 항목이 없어요",
    nothing: "Lumen은 앱이 다시 만드는 캐시처럼 안전 규칙이 허용하는 항목만 제안해요.",
    filters: {
      all: "전체",
    },
    groups: {
      command: "명령으로 정리",
      developer: "개발 도구",
      app_cache: "앱 캐시",
      app_data: "앱 데이터",
      other: "기타",
    },
    otherTools: "기타 도구",
    commandNote:
      "도구의 명령으로만 정리할 수 있는 항목이에요. 항목을 열어 명령을 실행할 수 있어요. 실행하면 되돌릴 수 없으니 무엇을 지우는지 먼저 확인해 주세요.",
    status: {
      quarantine: "정리 제안",
      command: "명령으로 정리",
      review: "검토 필요",
      keep: "유지",
    },
    columns: {
      name: "이름",
      location: "위치",
      status: "상태",
      size: "크기",
    },
    listHint: "위아래 화살표 키로 항목 사이를 이동할 수 있어요.",
    emptyFilter: "이 상태인 항목이 없어요.",
    partial: "검사가 중간에 멈췄어요. 그때까지 읽은 부분만 살펴봤어요.",
    whenClosed: (size: string, ...apps: string[]) =>
      apps.length === 0
        ? `사용 중인 앱을 종료하고 다시 검사하면 ${size}를 더 정리할 수 있어요.`
        : `${appList(apps)} 앱을 종료하고 다시 검사하면 ${size}를 더 정리할 수 있어요.`,
    outOfSpace:
      "디스크 공간이 거의 없어서 검사를 멈췄어요(Lumen은 늘 2 GB를 비워 둬요). 그때까지 읽은 부분만 살펴봤어요. 공간을 조금 비운 뒤 다시 검사해 주세요.",
  },
  action: {
    cleanUp: "정리하기…",
    preparing: "준비하는 중…",
    nothing: "정리를 제안한 항목이 없어요.",
    planFailed: "정리를 준비하지 못했어요. 파일은 모두 그대로 있어요.",
  },
  detail: {
    size: "크기",
    location: "위치",
    decision: "판단",
    reasons: "판단 근거",
    aiNoteHint: "AI 검토의 추측이에요. Lumen의 판단은 바뀌지 않아요.",
    whenClosed: (...apps: string[]) =>
      apps.length === 0
        ? "사용 중인 앱을 종료하고 다시 검사하면 정리할 수 있어요."
        : `${appList(apps)} 앱을 종료하고 다시 검사하면 정리할 수 있어요.`,
    relatedNone: "이 항목과 다른 항목을 잇는 기록이 없어요.",
    sources: {
      rule: "규칙",
      location: "위치",
      time: "시간",
      relation: "관계",
      process: "프로세스",
      ai: "AI 검토",
    },
    owner: (app: string, present: boolean | null) =>
      present === null
        ? `${app}의 항목이에요. 앱이 설치되어 있는지는 알 수 없어요.`
        : `${app}의 항목이에요. 앱이 설치되어 ${present ? "있어요" : "있지 않아요"}.`,
    appRunning: (app: string, running: boolean) =>
      running ? `${app} 앱이 실행 중이에요.` : `${app} 앱이 실행 중이 아니에요.`,
    appOpened: (app: string, date: string) =>
      `${app} 앱을 ${date}에 마지막으로 열었어요. 시스템이 모든 실행을 기록하지는 않아요.`,
    projectChanged: (date: string) =>
      `프로젝트 파일이 ${date}에 마지막으로 바뀌었어요. 매니페스트나 잠금 파일 같은 파일만 봐요.`,
    inUse: (inUse: boolean) => (inUse ? "앱이 열어 두고 있어요." : "열어 둔 앱이 없어요."),
    unseen: (unseen: number | null) =>
      unseen === null
        ? "보이는 앱 중에는 열어 둔 앱이 없어요."
        : `보이는 앱 중에는 열어 둔 앱이 없어요. 확인하지 못한 프로세스가 ${count(unseen)}개 있어요.`,
    changed: (date: string, folder: boolean) =>
      folder
        ? `폴더 항목이 ${date}에 마지막으로 바뀌었어요. 안의 내용은 그 뒤에 바뀌었을 수 있어요.`
        : `${date}에 마지막으로 바뀌었어요.`,
    opened: (date: string, folder: boolean) =>
      `${date} 또는 그 뒤에 마지막으로 ${folder ? "폴더를 읽었어요" : "열었어요"}. 시스템이 모든 사용을 기록하지는 않아요.`,
    notRecorded: (which: "modified" | "accessed") =>
      which === "modified"
        ? "마지막으로 바뀐 시간이 기록되지 않았어요."
        : "마지막으로 쓴 시간이 기록되지 않았어요.",
    notRead: (which: "modified" | "accessed") =>
      which === "modified"
        ? "마지막으로 바뀐 시간을 읽지 못해서 알 수 없어요."
        : "마지막으로 쓴 시간을 읽지 못해서 알 수 없어요.",
    means: {
      keep: "Lumen이 그대로 두고 아무것도 옮기지 않아요.",
      review: "직접 판단하도록 남겨 뒀어요. Lumen이 알아서 옮기지 않아요.",
      command: "이 항목의 도구가 직접 정리해야 해요. 요청할 때만 명령을 실행해요.",
      quarantine: "격리함으로 옮기자고 제안해요. 격리함에서 되돌릴 수 있어요.",
    },
    recheck: {
      unknown: "판단할 때 Lumen이 몰랐던 것:",
      gaps: {
        in_use: "앱이 쓰고 있는지",
        unread: "안에 무엇이 있는지 전부",
        stale: "지금은 어떤지",
        owner: "어느 앱의 것인지, 그 앱이 설치돼 있는지",
      },
      action: "다시 확인하기",
      checking: "확인하는 중…",
      note: "확인은 읽기만 해요. 아무것도 옮기지 않아요.",
      settled: "다시 확인할 것이 없어요. 이 판단은 Lumen이 몰랐던 것에 기대지 않아요.",
      changed: "검사 뒤에 바뀌어서 다시 판단하지 않았어요. 다시 검사해 주세요.",
      still: (status: string) => `다시 확인했어요 · ${status} 그대로예요`,
      now: (before: string, after: string) => `다시 확인했어요 · ${before} → ${after}`,
      failed: "다시 확인하지 못했어요.",
    },
    lens: {
      tool: (n: number, tool: string, size: string) =>
        `${tool}의 다른 항목 ${count(n)}개 · ${size}`,
      processes: (n: number, size: string, ...apps: string[]) =>
        `${appList(apps)} 앱이 함께 쓰는 다른 항목 ${count(n)}개 · ${size}`,
      evidence: (n: number, size: string) =>
        `검사 기록으로 이어진 다른 항목 ${count(n)}개 · ${size}`,
      kind: (n: number, group: string, size: string) =>
        `${group}의 다른 항목 ${count(n)}개 · ${size}`,
    },
  },
  aiNote: {
    likely: (kind: string, confidence: string) => `AI: ${kind}로 보여요(${confidence})`,
    unknown: "AI: 무엇인지 알아내지 못했어요",
    kinds: {
      app_settings: "앱 설정이나 데이터",
      user_documents: "사용자 문서",
      cache: "캐시",
      logs: "로그",
      downloads_or_installers: "다운로드한 파일이나 설치 파일",
      developer_tool_data: "개발 도구 데이터",
      backup_or_archive: "백업이나 압축 파일",
      unknown: "알 수 없는 항목",
    },
    confidence: {
      low: "확신 낮음",
      medium: "확신 보통",
      high: "확신 높음",
    },
  },
  toolCommand: {
    homebrewDownloads:
      "Homebrew가 오래된 설치 버전과 최근 다운로드 캐시를 정리해요. 다음 설치 때 필요한 파일을 다시 받을 수 있어요.",
    title: (tool: string) => `${tool}에서 직접 정리할 수 있어요`,
    copy: "복사",
    copied: "복사했어요",
    copyFailed: "복사하지 못했어요. 명령을 선택해서 직접 복사해 주세요.",
    note: "Lumen은 이 명령을 실행하지 않아요. 실행하기 전에 무엇을 지우는지 확인해 주세요.",
    run: "명령 실행하기…",
    runConfirm: "명령 실행",
    runWarning:
      "이 명령은 파일을 바로 지워요. 격리함을 거치지 않아서 Lumen이 되돌릴 수 없어요. 무엇을 지우는지 확인한 뒤 실행해 주세요.",
    removesData:
      "Docker의 멈춘 컨테이너, 쓰지 않는 네트워크, 이름 없는 이미지, 쓰지 않는 빌드 캐시가 지워져요.",
    running: "실행하는 중… 몇 분 걸릴 수 있어요.",
    succeeded: "명령이 끝났어요. 다시 검사하면 남은 항목을 확인할 수 있어요.",
    failed: (code: number | null) =>
      code === null ? "명령이 실패했어요." : `명령이 실패했어요(종료 코드 ${code}).`,
    timedOut: "15분이 지나도 끝나지 않아서 멈췄어요.",
    output: "출력 보기",
    runFailed: "명령을 실행하지 못했어요.",
    runNote: "실행하면 되돌릴 수 없어요. 무엇을 지우는지 확인한 뒤 실행해 주세요.",
  },
  spaceChange: {
    estimate: (size: string) => `선택한 항목의 예상 확보 공간은 ${size}예요.`,
    increased: (size: string) => `작업 전후 여유 공간이 ${size} 늘었어요.`,
    decreased: (size: string) => `작업 전후 여유 공간이 ${size} 줄었어요.`,
    unchanged: "현재 측정에서는 여유 공간 변화가 없어요.",
    unknown: "여유 공간 변화를 측정하지 못했어요.",
    multiple: "디스크별 여유 공간을 측정했어요.",
    shared: "여유 공간을 공유할 수 있어 디스크별로 표시해요.",
    details: "측정값 보기",
    disk: (index: number) => `디스크 ${index}`,
    readings: (before: string, after: string) => `여유 공간 ${before} → ${after}`,
    note: "측정값에는 다른 앱의 작업도 포함돼요. 디스크 수치는 늦게 반영될 수 있어요.",
  },
  coverage: {
    incomplete: "일부 폴더를 읽지 못해서 그대로 두었어요. 실제 크기는 더 클 수 있어요.",
    howTo: "전체 디스크 접근 권한을 허용하는 방법",
  },
  details: {
    title: "검사 세부 정보",
    unread: {
      title: "읽지 못한 폴더",
      loading: "불러오는 중…",
      failed: "목록을 불러오지 못했어요.",
      more: (n: string) => `외 ${count(n)}개`,
      reasons: {
        privacy:
          "macOS 개인정보 보호 때문에 읽지 못했어요. 전체 디스크 접근 권한을 허용하면 읽을 수 있어요.",
        system_protected: "macOS가 보호하는 폴더예요. 정상이니 바꿀 것은 없어요.",
        permissions: "이 계정에는 읽을 권한이 없어요. 시스템 폴더라면 정상이에요.",
        out_of_scope: "이 기기에서 Lumen이 읽을 수 있는 범위 밖이에요.",
        denied: "접근이 거부됐어요.",
        vanished: "검사하는 동안 사라졌어요.",
        timed_out: "읽는 데 너무 오래 걸렸어요. 다시 검사하면 읽을 수도 있어요.",
        failed: "읽지 못했어요.",
        cut_short: "일부만 읽었어요.",
        not_entered: "검사하지 않았어요. 다른 디스크에 있거나, 제외됐거나, 검사가 닿지 않았어요.",
      },
    },
    entries: (entries: string) => `읽은 항목 ${count(entries)}개`,
    files: "파일",
    fileCount: (files: string) => `${count(files)}개`,
    size: "크기",
    sizeValue: (logical: string, allocated: string) => `${logical}(디스크 사용 ${allocated})`,
    reclaimable: "확보할 수 있는 공간",
    atLeast: (size: string) => `최소 ${size}`,
    complete: "모두 읽었어요.",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `일부를 읽지 못했어요(접근 거부 ${count(denied)}, 실패 ${count(failed)}, 검사 안 함 ${count(notScanned)}).`,
  },
  confirm: {
    title: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮길까요?`,
    checked: "옮기기 직전에 하나씩 다시 확인하고, 검사 뒤에 바뀐 항목은 그대로 둬요.",
    restorable: "영구 삭제하기 전까지 최소 30일 동안 격리함에서 모두 복원할 수 있어요.",
    nothingLeft: "이제 옮길 항목이 없어요. 다시 검사해서 정리할 항목을 확인해 주세요.",
    items: "옮길 항목",
    move: "격리함으로 옮기기",
    moving: "옮기는 중…",
    failed: "정리하지 못했어요.",
    planMade: (createdAt: string) =>
      `${createdAt}에 만든 정리 계획이에요. 24시간 동안 쓸 수 있어요.`,
    planHash: "계획 해시",
  },
  done: {
    title: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮겼어요.`,
    noneMoved: "아무것도 옮기지 않았어요.",
    freedNote: "디스크 공간은 격리함에서 영구 삭제할 때 생겨요.",
    openQuarantine: "격리함 열기",
    notMoved: (n: number) => `옮기지 않은 ${items(n)}`,
    left: (n: number) => `이번 검사에서 남은 ${items(n)}`,
    scanAgainToMove: "더 옮기려면 다시 검사해 주세요.",
  },
  item: {
    quarantined: "격리함으로 옮겼어요. 복원할 수 있어요.",
    skipped: (why: string) => `${why} 옮기지 않았어요.`,
    failed: (reason: string, where: string) => `옮기지 못했어요. ${reason}. ${where}.`,
    failIdentityMismatch: "검사한 뒤에 바뀌었어요",
    failInUse: "프로그램이 쓰고 있어요",
    failVolumeUnsupported: "이 드라이브에는 격리함을 둘 수 없어요",
    failStoreUnavailable: "이 드라이브의 격리함 폴더를 안전하게 열 수 없어요",
    failMoveFailed: "옮기는 중에 문제가 생겼어요",
    failVerificationFailed: "옮긴 뒤 확인한 내용이 계획과 달랐어요",
    failOther: "알 수 없는 이유예요",
    skipChanged: "검사한 뒤에 바뀌었거나 다시 확인할 수 없어서",
    skipOutsideRoots: "검사한 폴더 밖에 있어서",
    skipNowKeep: "이제는 유지할 항목이라서",
    skipNowReview: "이제는 직접 검토할 항목이라서",
    skipReasonsChanged: "제안한 이유가 달라져서",
    skipNotPrepared: "안전하게 옮기는 데 필요한 기록을 남기지 못해서",
    skipManifest: "기록을 저장하지 못해서",
    leftSource: "원래 위치에 그대로 있어요",
    leftQuarantine: "격리함에 있고, 복원할 수 있어요",
    leftGone: "항목이 사라졌어요",
  },
  quarantine: {
    title: "격리함",
    atLeast: (size: string) => `최소 ${size}`,
    columns: {
      name: "이름",
      location: "원래 위치",
      size: "크기",
      held: "보관 기간",
    },
    loadFailed: "격리함을 불러오지 못했어요.",
    empty: "격리함이 비어 있어요",
    emptyHint: "정리한 항목은 여기서 기다려요. 마음이 바뀌면 복원할 수 있어요.",
    goCleanUp: "탐색 열기",
    interrupted: (n: number) =>
      `정리가 중간에 멈춰서 ${items(n)}가 어디 있는지 알 수 없어요. Lumen은 삭제하지 않았어요. 원래 있던 곳을 확인해 주세요.`,
    selection: {
      all: "전체 선택",
      item: (name: string) => `${name} 선택`,
      count: (n: number, size: string) => `${items(n)} 선택 · ${size}`,
      clear: "선택 해제",
      restore: "복원",
      removeAsk: "영구 삭제…",
      removeWarning: (n: number) =>
        `선택한 ${items(n)}를 영구 삭제하면 되돌릴 수 없어요. 삭제하기 전에 항목마다 사용 중인지 먼저 확인해요.`,
      removeSkips: (n: number) => `복원만 가능한 ${items(n)}는 삭제하지 않아요.`,
      remove: "영구 삭제",
      working: (done: number, total: number) => `처리하는 중… ${count(done)}/${count(total)}`,
      restored: (well: number, total: number) =>
        well === total
          ? `${items(total)}를 복원했어요.`
          : `${items(total)} 중 ${count(well)}개를 복원했어요. 나머지는 격리함에 그대로 있어요.`,
      removed: (well: number, total: number) =>
        well === total
          ? `${items(total)}를 영구 삭제했어요.`
          : `${items(total)} 중 ${count(well)}개를 영구 삭제했어요. 나머지는 삭제하지 않았어요.`,
      restoredStopped: (well: number, total: number, untried: number) =>
        [
          `${items(total)} 중 ${count(well)}개를 복원하고, 오류가 생겨서 멈췄어요.`,
          untried > 0 ? `나머지 ${count(untried)}개는 복원하지 않았어요.` : null,
          "목록에 남은 항목은 격리함에 그대로 있어요.",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
      unverified: (n: number) =>
        `복원한 ${items(n)}는 기록과 정확히 같지 않아요. 내용을 확인해 주세요.`,
      removedStopped: (well: number, total: number, untried: number) =>
        [
          `${items(total)} 중 ${count(well)}개를 영구 삭제하고, 오류가 생겨서 멈췄어요.`,
          untried > 0 ? `나머지 ${count(untried)}개는 삭제하지 않았어요.` : null,
          "목록에 남은 항목은 격리함에 그대로 있어서 복원할 수 있어요.",
        ]
          .filter((sentence) => sentence !== null)
          .join(" "),
    },
  },
  held: {
    unknownPath: "(알 수 없는 위치)",
    days: (days: number) => (days === 0 ? "오늘" : `${count(days)}일`),
    restoreOnly: "복원만 가능",
    afterFailure: "옮기다가 문제가 생겨서 여기에 안전하게 보관했어요. 복원만 할 수 있어요.",
    restore: "복원",
    removeForGoodAsk: "영구 삭제…",
    removeForGoodWarning:
      "영구 삭제하면 되돌릴 수 없어요. 삭제하기 전에 사용 중인 항목인지 먼저 확인해요.",
    removeForGood: "영구 삭제",
    failed: "실행하지 못했어요. 다시 시도해 주세요.",
  },
  restore: {
    restored: "원래 위치로 복원했어요.",
    parentReplaced: "그사이 원래 폴더가 새로 만들어져 있었어요.",
    unverified: "다만 기록과 정확히 같지는 않아요. 내용을 확인해 주세요.",
    notRestored: (why: string) => `복원하지 못했어요. 항목은 격리함에 그대로 있어요. ${why}.`,
    nameTaken:
      "원래 위치에 같은 이름의 항목이 생겼어요. 그 항목을 다른 곳으로 옮긴 뒤 다시 시도해 주세요",
    originUnreachable: "원래 폴더에 안전하게 접근할 수 없어요",
    itemChanged: "격리함에 있는 동안 항목이 바뀌었어요",
    moveFailed: "원래 위치로 옮기지 못했어요",
    notRestorable: "이 항목은 지금 상태로는 복원할 수 없어요.",
  },
  finalize: {
    finalized: "영구 삭제했어요.",
    foundAtSource: "이미 원래 위치로 돌아가 있어요. 아무것도 삭제하지 않았어요.",
    notFound:
      "격리함에서 찾지 못했어요. 다른 드라이브에 보관했다면 그 드라이브를 연결한 뒤 다시 시도해 주세요. 아무것도 삭제하지 않았어요.",
    failed: (what: string) => `완전히 삭제하지 못했어요. ${what}.`,
    removedNothing: "아무것도 삭제하지 않았어요",
    removedPartly: "일부만 삭제했어요. 다시 시도하면 나머지를 삭제해요",
    removedItem: "항목은 삭제했고, 기록만 남아 있어요",
    maybeInUse:
      "사용 중이거나 사용 중인지 확인할 수 없어서 삭제하지 않았어요. 이 항목을 쓰는 프로그램을 닫고 다시 시도해 주세요.",
    notFinalizable: "이 항목은 복원만 할 수 있어요.",
  },
  apps: {
    title: "앱",
    desktopOnly: "설치된 앱 목록은 데스크톱 앱에서 볼 수 있어요.",
    reading: "설치된 앱을 읽고 있어요…",
    readAgain: "다시 읽기",
    loadFailed: "설치된 앱 목록을 읽지 못했어요.",
    search: "앱 검색",
    showAll: (n: number) => `의존성과 숨은 항목 ${n}개도 보기`,
    none: "찾는 앱이 없어요.",
    protected: "시스템 구성 요소",
    protectedTip: "Lumen은 이 항목을 지우지 않아요.",
    partial: (names: string) => `일부 목록은 다 읽지 못해서 빠진 앱이 있을 수 있어요: ${names}.`,
    readOnly:
      "이 목록은 컴퓨터에서 읽었고 컴퓨터 밖으로 나가지 않아요. 앱은 직접 확인한 뒤에만 옮겨요.",
    readOnlyAi:
      "AI 검토가 켜져 있는 동안에는 이 앱들의 이름, 게시자, 버전을 경로 없이 AI 검토로 보내요. 앱은 직접 확인한 뒤에만 옮겨요.",
    move: {
      action: "격리함으로 옮기기…",
      title: (name: string) => `${name} 앱을 격리함으로 옮길까요?`,
      moving: (name: string) => `${name} 앱을 격리함으로 옮기고 있어요…`,
      checking: "앱을 확인하고 있어요…",
      failed: "앱을 확인하거나 옮기지 못했어요. 다시 시도하기 전에 격리함을 확인해 주세요.",
      refused: "이 앱은 격리함으로 옮길 수 없어요",
      notFound: "목록에서 이 앱을 찾지 못했어요. 목록을 다시 읽어 주세요.",
      ambiguous: "같은 식별자를 쓰는 앱이 여러 개라서 하나를 고를 수 없어요.",
      unsupported: "앱을 격리함으로 옮기는 기능은 macOS에서 쓸 수 있어요.",
      size: "앱 크기",
      atLeast: (size: string) => `최소 ${size}`,
      expires: (date: string) => `미리보기는 만든 뒤 24시간이 지난 ${date}까지 쓸 수 있어요.`,
      moved: (name: string) => `${name} 앱을 격리함으로 옮겼어요.`,
      noSpace: "디스크 공간은 격리함에서 앱을 영구 삭제할 때 생겨요.",
      changed: "앱이 달라졌어요. 미리보기를 닫고 다시 확인해 주세요.",
      failedMove: "앱을 격리함으로 옮기는 작업을 끝내지 못했어요.",
      quit: "종료하기",
      quitHint:
        "앱의 종료 메뉴처럼 앱에 종료를 요청해요. 저장하지 않은 작업이 있으면 앱이 먼저 물어봐요.",
      quitAsked: "앱에 종료를 요청했어요. 저장할지 물으면 앱에서 답해 주세요.",
      stillRunning: "앱이 아직 실행 중이에요. 앱을 종료한 뒤 다시 확인해 주세요.",
      quitItself: "Lumen은 여기서 스스로 종료할 수 없어요.",
      quitFailed: "앱에 종료를 요청하지 못했어요. 앱을 직접 종료한 뒤 다시 확인해 주세요.",
      recheck: "다시 확인하기",
      notes: {
        goes_to_quarantine:
          "영구 삭제하기 전까지 앱을 복원할 수 있어요. Lumen이 자동으로 지우지 않아요.",
        library_data_stays:
          "~/Library의 앱 데이터는 그대로 둬요. 새로 검사한 뒤 탐색에서 살펴볼 수 있어요.",
      },
      locations: {
        source: "앱은 원래 위치에 있어요.",
        quarantine: "앱은 격리함에 있어요. 격리함을 열어 확인해 주세요.",
        gone: "앱이 어디에 있는지 확인하지 못했어요. 다시 시도하기 전에 격리함을 확인해 주세요.",
      },
      reasons: {
        not_in_applications: "/Applications나 ~/Applications 바로 안에 있는 앱이 아니에요.",
        protected: "macOS의 일부라서 그대로 둬요.",
        not_a_bundle: "앱 폴더가 아니거나 링크예요.",
        owner_unknown: "앱의 소유자를 확인하지 못했어요.",
        not_owned_by_user:
          "다른 계정이 소유한 앱이에요. 앱의 설치 프로그램이나 App Store에서 지워 주세요.",
        services_unknown: "백그라운드 항목을 모두 확인하지 못했어요.",
        service_linked:
          "이 앱에 연결된 백그라운드 항목이 있어요. 앱의 전용 삭제 프로그램을 써 주세요.",
        helpers_unknown: "앱의 백그라운드 보조 프로그램을 확인하지 못했어요.",
        has_helpers: "백그라운드 보조 프로그램이 있어요. 앱의 전용 삭제 프로그램을 써 주세요.",
        running_unknown: "앱이 실행 중인지 확인하지 못했어요.",
        running: "앱이 실행 중이에요. 앱을 종료한 뒤 다시 확인해 주세요.",
        store_unknown: "이 드라이브에 격리함을 둘 수 있는지 확인하지 못했어요.",
        no_store_on_volume: "이 드라이브에는 격리함이 없어요.",
      },
    },
    ai: {
      explaining: "AI 검토가 앱을 설명하고 있어요…",
      note: (purpose: string, confidence: string, impact: string) =>
        `AI: ${purpose}일 가능성이 높아요(${confidence}). ${impact}`,
      withConfidence: (impact: string, confidence: string) => `${impact} (${confidence})`,
      purposes: {
        web_browser: "웹 브라우저",
        communication: "소통 앱",
        media: "미디어 앱",
        office_or_productivity: "사무·생산성 앱",
        development_tool: "개발 도구",
        design_or_creative: "디자인·창작 앱",
        game: "게임",
        security_or_banking: "보안·뱅킹 프로그램",
        system_or_driver: "시스템 구성 요소나 드라이버",
        cloud_sync_or_backup: "클라우드 동기화·백업 앱",
        utility: "유틸리티",
        runtime_or_library: "런타임이나 라이브러리",
        unknown: "알 수 없는 항목",
      },
      impacts: {
        only_this_app: "지워도 다른 곳에는 영향이 없을 것 같아요.",
        other_software_depends: "다른 프로그램이 이 항목에 의존할 가능성이 높아요.",
        stops_background_activity: "지우면 이 앱의 백그라운드 작업이 멈출 가능성이 높아요.",
        security_or_access: "지우면 보안, 뱅킹, 로그인이 동작하지 않을 수 있어요.",
        system_function: "지우면 시스템 기능이 동작하지 않을 수 있어요.",
        unknown: "지웠을 때 어떤 영향이 있을지는 알 수 없었어요.",
      },
    },
    origins: {
      mac_app: "앱",
      mac_installer: "설치 패키지",
      homebrew: "Homebrew",
      homebrew_cask: "Homebrew Cask",
      windows_program: "프로그램",
      msix: "스토어 패키지",
      dpkg: "dpkg",
      rpm: "RPM",
      pacman: "pacman",
      flatpak: "Flatpak",
      snap: "Snap",
      other: "기타",
    },
  },
  settings: {
    title: "설정",
    appearance: {
      title: "테마",
      system: "시스템 설정",
      light: "라이트",
      dark: "다크",
      failed: "창의 테마를 바꾸지 못했어요.",
    },
    permissions: {
      title: "권한",
      fda: "전체 디스크 접근 권한",
      fdaIntro:
        "허용 여부는 macOS 시스템 설정에서 확인할 수 있어요. 이미 허용했다면 이 안내는 건너뛰어도 돼요. 일부 보호된 폴더는 권한이 있어도 읽지 못할 수 있어요.",
      steps: "설정 방법 보기",
      step1: "시스템 설정을 열어 주세요.",
      step2: "개인정보 보호 및 보안 › 전체 디스크 접근 권한으로 이동해 주세요.",
      step3: "Lumen을 허용해 주세요. 이미 켜져 있다면 그대로 두어도 돼요.",
      step4: "설정을 바꿨다면 Lumen을 완전히 종료한 뒤 다시 열고 새로 검사해 주세요.",
      open: "시스템 설정 열기",
      openFailed: "시스템 설정을 열지 못했어요. Apple 메뉴에서 직접 열어 주세요.",
      access: "폴더 접근",
      accessIntro:
        "컴퓨터가 일부 폴더에 Lumen이 접근하지 못하게 막을 수 있어요. 그래도 Lumen은 허용된 폴더를 모두 읽어요.",
      unread:
        "읽지 못한 폴더는 비어 있다고 보지 않고 ‘읽지 못함’으로 표시해요. 그 안의 항목은 정리를 제안하지 않아요.",
      helper: {
        title: "관리자 권한 도우미",
        intro:
          "다른 계정이 설치한 앱처럼 관리자 권한이 필요한 항목을 정리할 때 써요. 정리 규칙은 그대로이고, 옮긴 항목은 격리함에서 되돌릴 수 있어요.",
        notYet: "아직은 연결만 확인하고, 아무것도 옮기지 않아요.",
        off: "꺼져 있어요.",
        waiting:
          "시스템 설정 › 일반 › 로그인 항목에서 Lumen을 허용해 주세요. 관리자 암호가 필요해요.",
        on: (version: string) => `켜져 있어요. 도우미가 응답해요(버전 ${version}).`,
        enable: "켜기",
        disable: "끄기",
        openLoginItems: "로그인 항목 열기",
        changeFailed: "도우미 설정을 바꾸지 못했어요.",
        problems: {
          outdated: "켜져 있지만 도우미가 다른 버전의 Lumen에서 왔어요. 껐다가 다시 켜 주세요.",
          unsigned: "이 Lumen은 Developer ID로 서명되지 않아서 도우미를 쓸 수 없어요.",
          no_answer: "켜져 있지만 도우미가 응답하지 않아요. 껐다가 다시 켜 주세요.",
          not_found: "Lumen에서 도우미를 찾지 못했어요. Lumen을 다시 설치해 주세요.",
          mismatch: "도우미의 서명이 이 Lumen과 맞지 않아요. 껐다가 다시 켜 주세요.",
          failed: "도우미를 확인하지 못했어요.",
        },
      },
    },
    about: {
      title: "정보",
      version: "버전",
      local: "이 기기에서만",
      localBody:
        "검사 결과, 판단, 격리함은 모두 이 컴퓨터에만 있어요. 파일 내용은 어디에도 보내지 않아요. AI 검토가 켜져 있는 동안에는 경로나 크기 같은 정보와 설치된 앱의 이름, 게시자, 버전만 보내요.",
      update: {
        title: "업데이트",
        checking: "새 버전을 확인하는 중…",
        upToDate: "최신 버전이에요.",
        available: (version: string) => `새 버전(${version})을 받을 수 있어요.`,
        packageOnly: "패키지로 설치한 앱이라 zaewc.github.io/Lumen에서 새 버전을 받아 주세요.",
        checkFailed: "새 버전을 확인하지 못했어요.",
        check: "지금 확인",
        install: "업데이트하고 다시 시작",
        installing: "업데이트하는 중…",
        installingNote: "새 버전을 내려받아 서명을 확인한 뒤 설치하고 다시 시작해요.",
        installFailed: "업데이트하지 못했어요. 지금 버전은 그대로예요.",
        waitForItems: "Lumen이 항목을 옮기거나 지우고 나면 업데이트할 수 있어요.",
        sidebar: (version: string) => `새 버전 ${version}`,
        notice: (version: string) => `Lumen ${version} 업데이트가 있어요.`,
        noticeAction: "업데이트 보기",
        later: "나중에",
      },
    },
  },
  whatIs: (term: string) => `‘${term}’${withIran(term)}?`,
  glossary: {
    verdicts: {
      title: "Lumen이 판단하는 방법",
      body: "Lumen은 추측하지 않고, 정해진 안전 규칙으로 항목마다 판단해요. ‘정리 제안’은 앱이나 도구가 다시 만드는 캐시 같은 항목이에요. ‘명령으로 정리’는 도구로만 지워야 하는 항목이라, 직접 실행할 수 있게 도구의 명령을 보여 드려요. ‘검토 필요’는 Lumen이 확신하지 못하는 큰 항목이나 삭제한 앱이 남긴 항목이라 자동으로 옮기지 않아요. ‘유지’는 시스템 파일, 사용 중인 파일, 앱의 데이터, Lumen이 모르는 작은 항목처럼 Lumen이 건드리지 않는 항목이에요. 확실하지 않으면 그대로 둬요.",
    },
    quarantine: {
      title: "격리함",
      body: "격리함은 항목을 안전하게 잠시 맡아 두는 곳이에요. 정리하면 항목을 삭제하지 않고 격리함으로 옮기고, 영구 삭제하기 전까지 원래 위치로 복원할 수 있어요. 격리함의 항목도 영구 삭제하기 전까지는 디스크 공간을 차지해요.",
    },
  },
  reviewReasons: {
    unknownOwner: "어떤 앱이나 도구가 만든 항목인지 확인하지 못했어요.",
    sharedResource:
      "다른 파일과 저장 공간을 공유하거나 이 항목 밖에도 연결된 이름이 있을 수 있어요.",
    staleEvidence: "확인한 근거가 오래됐어요. 다시 검사한 뒤 판단해 주세요.",
    uncertainSize: "영구 삭제한 뒤 확보할 수 있는 공간이 확실하지 않아요.",
  },
  reasons: {
    location: (name: string) => `유지: 보호된 위치(${name})에 있어요.`,
    rules: {
      "default.review": "검토 필요: Lumen이 모르는 항목이에요.",
      "default.keep-small": "유지: 크기가 작아서 검토하지 않아도 돼요.",
      "protect.os-protected": "유지: 운영 체제가 보호하는 항목이에요.",
      "protect.system-file": "유지: Windows가 시스템 파일로 표시한 항목이에요.",
      "protect.cloud-backed":
        "유지: 클라우드 서비스와 동기화돼 있어서, 여기서 지우면 모든 기기에서 지워질 수 있어요.",
      "protect.in-use": "유지: 실행 중인 프로그램이 쓰고 있어요.",
      "protect.app-running":
        "유지: 이 항목의 앱이 실행 중이에요. 앱을 끄고 다시 검사하면 정리할 수 있어요.",
      "protect.in-use-partial":
        "유지: Lumen이 확인한 프로그램 중에는 쓰는 곳이 없지만, 모든 프로그램을 확인하지는 못했어요.",
      "protect.in-use-unknown": "유지: 사용 중인지 아직 확인하지 않았어요.",
      "protect.not-observed": "유지: 검사에서 이 항목을 확인하지 못했어요.",
      "protect.incomplete-scan": "유지: 일부를 읽지 못해서 안에 무엇이 있는지 알 수 없어요.",
      "protect.location-data": "유지: 보호 위치 목록을 확인하지 못했어요.",
      "protect.location-context": "유지: 보호 위치 목록을 확인하지 못했어요.",
      "protect.contains-system": "유지: 운영 체제의 일부가 들어 있어요.",
      "protect.contains-sensitive":
        "유지: 비밀번호, 키, 웹 브라우저 로그인 정보가 들어 있어서 Lumen이 건드리지 않아요.",
      "protect.contains-cloud":
        "유지: 클라우드 동기화 폴더가 들어 있어서, 여기서 지우면 그 파일이 모든 기기에서 지워질 수 있어요.",
      "eligible.quarantine-unsupported": "검토 필요: 이 기기에서는 복원할 수 있게 옮길 수 없어요.",
      "eligible.frees-nothing": "유지: 지워도 공간이 생기지 않아요.",
      "eligible.object-kind": "검토 필요: 데이터가 아니라 링크나 특수 파일이에요.",
      "eligible.in-use-partial":
        "Lumen이 확인한 프로그램 중에는 쓰는 곳이 없어요. 시스템 프로그램은 확인하지 못해서 옮기기 직전에 다시 확인해요.",
      "eligible.in-use-partial-single-user":
        "Lumen이 확인한 프로그램 중에는 쓰는 곳이 없어요. 이 컴퓨터의 사용자는 나뿐이라 확인하지 못한 건 시스템 프로그램이에요. 옮기기 직전에 다시 확인해요.",
      "classify.devkb-regenerable": "정리 제안: 이 항목을 만든 도구가 다시 만들 수 있어요.",
      "classify.devkb-tool-action":
        "검토 필요: 도구의 정리 명령으로만 지워야 하고, 지우면 되돌릴 수 없어요.",
      "classify.devkb-not-regenerable": "검토 필요: 다시 만들 수 없는 항목이에요.",
      "classify.devkb-self-managed": "유지: 이 항목을 만든 도구가 알아서 정리해요.",
      "classify.devkb-unknown": "검토 필요: 이 항목에 맞는 도구 정보를 불러오지 못했어요.",
      "classify.vendor-declared-cache": "검토 필요: 만든 회사가 지워도 된다고 밝힌 데이터예요.",
      "classify.browser-profile":
        "유지: 웹 브라우저의 로그인 정보, 쿠키, 사이트 데이터가 들어 있어서 Lumen이 건드리지 않아요.",
      "classify.app-cache": "정리 제안: 앱의 캐시라서 앱이 다시 만들어요.",
      "classify.app-state": "유지: 앱의 데이터, 설정, 문서가 들어 있어서 Lumen이 건드리지 않아요.",
      "classify.orphan": "검토 필요: 이 항목을 쓰던 앱이 삭제된 것 같아요.",
      "jev.demote": "검토 필요: AI 검토가 주의를 권했어요.",
    },
  },
} satisfies Messages;
