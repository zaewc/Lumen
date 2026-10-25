import type { Messages } from "./en";
import { countFormat } from "./format";

const count = countFormat("ko");

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

/** The Korean catalog (합니다체), checked against the English one's shape. */
export const ko = {
  language: "언어",
  nav: {
    label: "주 메뉴",
    overview: "개요",
    cleanUp: "정리",
    quarantine: "격리함",
    settings: "설정",
    held: (n: number) => `격리함에 ${items(n)}`,
  },
  common: {
    loading: "불러오는 중…",
    cancel: "취소",
    close: "닫기",
    details: "자세히",
    tryAgain: "다시 시도",
    scanDisk: "디스크 검사",
    scanning: "검사하는 중…",
    scanAgain: "다시 검사",
    items: (n: number) => items(n),
  },
  overview: {
    title: "개요",
    disk: {
      title: "시동 디스크",
      meter: "디스크 사용량",
      used: (size: string) => `사용 ${size}`,
      available: (size: string) => `남은 공간 ${size}`,
      total: (size: string) => `전체 ${size}`,
      percentUsed: (percent: string) => `${percent}% 사용 중`,
      low: "디스크가 거의 찼습니다. 남은 공간이 10% 미만입니다.",
      loadFailed: "디스크 사용량을 읽지 못했습니다.",
    },
    session: {
      title: "최근 검사",
      canFree: "정리 가능",
      review: "검토 필요",
      quarantine: "격리함",
      atLeast: (size: string) => `최소 ${size}`,
      scanning: "검사하는 중…",
      deciding: "판정하는 중…",
      found: (n: string) => `${n}개 찾음`,
      showProgress: "진행 상황 보기",
      cleaned: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮겼습니다.`,
      none: "아직 검사하지 않았습니다. 디스크를 검사해 확보할 수 있는 공간을 확인하십시오.",
    },
  },
  start: {
    title: "아직 검사하지 않았습니다",
    promise:
      "디스크 전체나 일부 폴더만 검사할 수 있습니다. 확인하기 전에는 아무것도 옮기지 않습니다.",
    wholeDiskHint:
      "검사는 읽기만 하며 몇 분 정도 걸립니다. 일부 폴더는 컴퓨터가 접근 허용 여부를 물어볼 수 있습니다.",
    chooseFolders: "폴더 선택…",
    foldersLabel: "검사할 폴더(한 줄에 하나씩)",
    foldersEmpty: "폴더를 하나 이상 입력하십시오.",
    scanFolders: "이 폴더 검사",
    failed: "검사를 마치지 못했습니다. 아무것도 변경하지 않았습니다.",
  },
  ai: {
    title: "AI 검토",
    offIntro:
      "Lumen이 정리를 제안한 항목을 AI 모델이 한 번 더 확인합니다. AI는 Lumen을 더 신중하게 만들 수만 있습니다. 직접 켜기 전까지는 꺼져 있습니다.",
    onIntro: (service: string) =>
      `켜짐(${service}). 항목의 정리를 제안하기 전에 그 정보(내용은 제외)를 보내 의견을 한 번 더 받습니다. AI는 Lumen을 더 신중하게 만들 수만 있습니다.`,
    checking: "AI 검토 설정을 확인하는 중…",
    short: {
      on: "AI 검토 켜짐",
      off: "AI 검토 꺼짐",
    },
    loadFailed: "AI 검토 설정을 불러오지 못했습니다.",
    changeFailed: "AI 검토 설정을 바꾸지 못했습니다.",
    consent: {
      title: "AI 검토를 켜시겠습니까?",
      what: "격리를 제안한 항목마다 메타데이터를 보냅니다. 경로(홈 폴더는 ~로 표시), 크기, 경과 기간, 판단 근거가 포함됩니다. 파일 내용은 보내지 않습니다.",
      whereJeb:
        "API 키와 함께 학교의 Jeb 서버(jeb.https.gsmsv.site)로 보냅니다. 서버 운영자가 요청을 보관할 수 있습니다.",
      whereOpenRouter: "API 키와 함께 OpenRouter를 거쳐 TypeSafe의 Jev로 보냅니다.",
      cautious:
        "AI는 판단을 더 신중하게 바꿀 수만 있습니다. 무엇을 옮기거나 삭제할지는 정하지 않습니다.",
      revocable: "켜고 나면 끌 때까지 모든 검사에 AI 검토를 적용하며, 언제든지 끌 수 있습니다.",
      keyLabel: "Jeb API 키",
      keyHint: "시스템의 보안 저장소에 보관하며, 다시 표시하지 않습니다.",
      keyInvalid: "키 전체를 붙여 넣으십시오(공백 없이 1~256자).",
      confirm: "켜기",
      turningOn: "켜는 중…",
    },
  },
  scanning: {
    title: "검사하는 중…",
    deciding: "판정 중…",
    decidingHint: "찾은 항목에 안전 규칙을 적용하고 있습니다.",
    readOnly: "읽기만 하며 아무것도 변경하지 않습니다.",
    progress: "검사 진행 상황",
    items: "찾은 항목",
    size: "디스크 사용량",
    unreadable: "읽지 못한 폴더",
    reading: "읽는 중",
    failed: "찾은 항목을 판단하지 못했습니다. 아무것도 변경하지 않았습니다.",
  },
  results: {
    canFree: (size: string) => `${size}를 정리할 수 있습니다`,
    nothingTitle: "정리할 항목이 없습니다",
    nothing:
      "지금은 정리할 항목이 없습니다. Lumen은 치워 두어도 안전하다고 확신하는 항목만 정리를 제안합니다.",
    filter: "표시",
    filters: {
      all: "전체",
      quarantine: "정리 제안",
      review: "검토 필요",
      keep: "유지",
    },
    groups: {
      developer: "개발 도구",
      app_cache: "앱 캐시",
      app_data: "앱 데이터",
      other: "기타",
    },
    otherTools: "기타 도구",
    verdict: {
      quarantine: "정리 제안",
      review: "검토 필요",
      keep: "유지",
    },
    columns: {
      name: "이름",
      location: "위치",
      status: "상태",
      size: "크기",
    },
    listHint: "위아래 화살표 키로 항목 사이를 이동할 수 있습니다.",
    emptyFilter: "이 보기에는 항목이 없습니다.",
    partial: "검사가 도중에 멈춰 결과가 완전하지 않습니다.",
    outOfSpace:
      "디스크 공간이 거의 없어 검사를 멈췄습니다(Lumen은 항상 2 GB를 남겨 둡니다). 결과가 완전하지 않으니, 공간을 확보한 뒤 다시 검사하십시오.",
  },
  action: {
    cleanUp: "정리하기…",
    preparing: "준비하는 중…",
    nothing: "정리를 제안한 항목이 없습니다.",
    planFailed: "정리를 준비하지 못했습니다. 아무것도 옮기지 않았습니다.",
  },
  detail: {
    size: "확보 가능",
    location: "위치",
    decision: "상태",
    reasons: "이유",
    aiTitle: "AI 검토가 주의를 권고했습니다.",
    aiBody: "그래서 정리를 제안하지 않고 직접 검토하도록 남겨 두었습니다.",
  },
  toolCommand: {
    title: (tool: string) => `${tool}에서 직접 정리할 수 있습니다`,
    copy: "복사",
    copied: "복사됨",
    copyFailed: "복사하지 못했습니다. 명령을 선택해 직접 복사하십시오.",
    note: "Lumen은 이 명령을 실행하지 않습니다. 무엇을 삭제하는지 먼저 확인하십시오.",
  },
  coverage: {
    incomplete: "일부 폴더를 읽지 못해 그대로 두었습니다. 실제 크기는 더 클 수 있습니다.",
    howTo: "전체 디스크 접근 권한 허용…",
  },
  details: {
    title: "검사 세부 정보",
    entries: (entries: string) => `읽은 항목 ${count(entries)}개`,
    files: "파일",
    fileCount: (files: string) => `${count(files)}개`,
    size: "크기",
    sizeValue: (logical: string, allocated: string) => `${logical}(디스크 사용 ${allocated})`,
    reclaimable: "확보 가능",
    atLeast: (size: string) => `최소 ${size}`,
    complete: "모두 읽었습니다.",
    scanOther: "다른 폴더 검사…",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `일부를 읽지 못했습니다: 접근 거부 ${count(denied)}, 실패 ${count(failed)}, 검사 안 함 ${count(notScanned)}.`,
  },
  confirm: {
    title: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮기시겠습니까?`,
    checked: "옮기기 전에 각 항목을 다시 확인합니다. 검사 후 바뀐 항목은 그대로 둡니다.",
    restorable: "영구 삭제하기 전까지 최소 30일 동안 격리함에서 모든 항목을 복원할 수 있습니다.",
    nothingLeft: "더 이상 옮길 항목이 없습니다. 다시 검사해 정리할 항목을 확인하십시오.",
    move: "격리함으로 옮기기",
    moving: "옮기는 중…",
    failed: "정리를 실행하지 못했습니다.",
    planMade: (createdAt: string) => `${createdAt}에 만든 정리 계획이며, 24시간 동안 유효합니다.`,
    planHash: "계획 해시",
  },
  done: {
    title: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮겼습니다.`,
    noneMoved: "아무것도 옮기지 않았습니다.",
    freedNote: "격리함에서 영구 삭제하면 공간이 확보됩니다.",
    openQuarantine: "격리함 열기",
    notMoved: (n: number) => `옮기지 않은 ${items(n)}`,
  },
  item: {
    quarantined: "격리함으로 옮겼습니다(복원 가능).",
    skipped: (why: string) => `옮기지 않았습니다: ${why}.`,
    failed: (reason: string, where: string) => `옮기지 못했습니다(${reason}). ${where}.`,
    skipChanged: "변경되었거나 더 이상 확인할 수 없습니다",
    skipOutsideRoots: "검사한 폴더 밖에 있습니다",
    skipNowKeep: "이제는 유지해야 할 항목입니다",
    skipNowReview: "이제는 직접 검토해야 할 항목입니다",
    skipReasonsChanged: "이제는 다른 이유로 제안될 항목입니다",
    skipNotPrepared: "안전하게 옮기기 위한 기록을 남기지 못했습니다",
    skipManifest: "기록을 저장하지 못했습니다",
    leftSource: "원래 위치에 그대로 있습니다",
    leftQuarantine: "격리함에 있으며 복원할 수 있습니다",
    leftGone: "항목이 사라졌습니다",
  },
  quarantine: {
    title: "격리함",
    count: (n: number) => `보관 중인 ${items(n)}`,
    total: (size: string) => `영구 삭제하면 ${size} 확보`,
    totalAtLeast: (size: string) => `영구 삭제하면 최소 ${size} 확보`,
    intro:
      "정리한 항목은 이곳에 보관됩니다. 복원하면 원래 위치로 돌아가고, 영구 삭제하면 공간이 확보됩니다.",
    loadFailed: "격리함을 불러오지 못했습니다.",
    empty: "격리함이 비어 있습니다",
    emptyHint: "정리한 항목은 이곳에 보관되므로, 마음이 바뀌면 복원할 수 있습니다.",
    goCleanUp: "정리하러 가기",
  },
  held: {
    unknownPath: "(알 수 없는 위치)",
    days: (days: number) => (days === 0 ? "오늘 옮김" : `보관한 지 ${count(days)}일`),
    afterFailure:
      "옮기는 도중 문제가 생겨 이곳에 안전하게 보관했습니다. 복원할 수 있지만 영구 삭제할 수는 없습니다.",
    restore: "복원",
    removeForGoodAsk: "영구 삭제…",
    removeForGoodWarning:
      "완전히 삭제되며 되돌릴 수 없습니다. 삭제하기 전에 사용 중인지 먼저 확인합니다.",
    removeForGood: "영구 삭제",
    failed: "작업을 완료하지 못했습니다.",
  },
  restore: {
    restored: "원래 위치로 복원했습니다.",
    parentReplaced: "그사이 원래 폴더가 다시 만들어져 있었습니다.",
    unverified: "복원했지만 기록과 정확히 일치하지 않습니다. 내용을 확인하십시오.",
    notRestored: (why: string) => `복원하지 못했습니다: ${why}. 격리함에 그대로 있습니다.`,
    nameTaken:
      "원래 위치에 같은 이름의 항목이 생겼습니다. 그 항목을 다른 곳으로 옮긴 뒤 다시 시도하십시오",
    originUnreachable: "원래 폴더에 안전하게 접근할 수 없습니다",
    itemChanged: "격리함에 있는 동안 항목이 변경되었습니다",
    moveFailed: "원래 위치로 옮기지 못했습니다",
    notRestorable: "이 항목은 현재 상태에서 복원할 수 없습니다.",
  },
  finalize: {
    finalized: "영구 삭제했습니다.",
    foundAtSource: "이미 원래 위치로 돌아가 있습니다. 아무것도 삭제하지 않았습니다.",
    notFound: "격리함에 없습니다. 아무것도 삭제하지 않았습니다.",
    failed: (what: string) => `완전히 삭제하지 못했습니다: ${what}.`,
    removedNothing: "아무것도 삭제되지 않았습니다",
    removedPartly: "일부만 삭제되었습니다. 다시 시도하면 마저 삭제합니다",
    removedItem: "항목은 삭제되었고 기록만 남아 있습니다",
    maybeInUse:
      "사용 중이거나 사용 여부를 확인할 수 없어 아무것도 삭제하지 않았습니다. 이 항목을 사용하는 프로그램을 닫고 다시 시도하십시오.",
    notFinalizable: "이 항목은 복원만 할 수 있으며 영구 삭제할 수는 없습니다.",
  },
  settings: {
    title: "설정",
    permissions: {
      title: "권한",
      fda: "전체 디스크 접근 권한",
      fdaIntro:
        "macOS는 메일, 메시지, 다른 앱의 데이터 같은 일부 폴더를 보호합니다. Lumen은 허용한 경우에만 이 폴더를 읽습니다.",
      steps: "허용하는 방법:",
      step1: "시스템 설정을 엽니다.",
      step2: "개인정보 보호 및 보안 › 전체 디스크 접근 권한으로 이동합니다.",
      step3: "Lumen을 켠 뒤 다시 검사합니다.",
      open: "시스템 설정 열기",
      openFailed: "시스템 설정을 열지 못했습니다. Apple 메뉴에서 직접 여십시오.",
      access: "폴더 접근",
      accessIntro:
        "컴퓨터가 일부 폴더에 대한 접근을 거부할 수 있습니다. 그럴 때도 Lumen은 허용된 폴더를 모두 읽습니다.",
      unread:
        "읽지 못한 폴더는 비어 있는 것으로 보지 않고 ‘읽지 못함’으로 표시하며, 그 안의 항목은 정리를 제안하지 않습니다.",
    },
    about: {
      title: "정보",
      version: "버전",
      local: "이 기기에서만",
      localBody:
        "검사, 판단, 격리함은 모두 이 컴퓨터에 있습니다. 파일 내용은 어디에도 보내지 않으며, AI 검토를 켜면 메타데이터만 보냅니다.",
    },
  },
  whatIs: (term: string) => `‘${term}’${withIran(term)}?`,
  glossary: {
    scan: {
      title: "검사",
      body: "검사는 폴더를 읽고 그 안에 무엇이 있는지 측정합니다. 읽기만 하므로 아무것도 옮기거나 바꾸거나 삭제하지 않습니다. 이름과 크기 같은 정보만 살펴보며, 파일을 열거나 클라우드에만 있는 파일을 내려받지 않습니다.",
    },
    coverage: {
      title: "읽지 못한 폴더",
      body: "접근이 허용되지 않았거나 읽는 도중 문제가 생겨 Lumen이 폴더를 읽지 못할 때가 있습니다. Lumen은 이런 폴더를 비어 있는 것으로 여기지 않으며, 확인하지 못한 항목은 정리를 제안하지 않고 그대로 둡니다. 이런 폴더가 포함된 크기는 최솟값이므로 실제로는 더 클 수 있습니다.",
    },
    reclaimable: {
      title: "확보할 수 있는 공간",
      body: "항목을 영구 삭제했을 때 되찾을 수 있는 디스크 공간입니다. 다른 파일과 함께 쓰는 공간(복사본이나 스냅샷 등)은 포함하지 않습니다. 격리함으로 옮기기만 해서는 공간이 늘어나지 않으며, 영구 삭제해야 확보됩니다.",
    },
    verdicts: {
      title: "Lumen의 판단 방식",
      body: "Lumen은 추측하지 않고 정해진 안전 규칙에 따라 모든 항목을 판단합니다. ‘정리 제안’은 앱이나 도구가 다시 만드는 캐시 같은 항목입니다. ‘검토 필요’는 Lumen이 확신할 수 없는 항목으로, 자동으로 옮기지 않습니다. ‘유지’는 시스템 파일, 사용 중인 파일, 클라우드와 동기화된 파일처럼 Lumen이 건드리지 않는 항목입니다. 확실하지 않으면 항목을 그대로 둡니다.",
    },
    quarantine: {
      title: "격리함",
      body: "격리함은 안전한 임시 보관 장소입니다. 정리하기를 하면 항목을 삭제하지 않고 격리함으로 옮기며, 모든 항목을 원래 위치로 되돌릴 수 있습니다. 격리함의 항목은 영구 삭제하기 전까지 계속 디스크 공간을 차지합니다.",
    },
    finalize: {
      title: "영구 삭제",
      body: "영구 삭제는 격리함의 항목을 완전히 지우고 공간을 확보합니다. 되돌릴 수 없는 유일한 단계입니다. Lumen은 먼저 항목을 사용하는 프로그램이 없는지 확인하고, 사용 중이거나 확인할 수 없으면 아무것도 삭제하지 않습니다.",
    },
    restore: {
      title: "복원",
      body: "복원은 항목을 원래 위치에 원래 이름으로 되돌립니다. Lumen은 어떤 것도 덮어쓰지 않습니다. 그 자리에 같은 이름의 항목이 생겼거나 원래 폴더에 안전하게 접근할 수 없으면, 항목은 격리함에 남고 Lumen이 그 이유를 알려 드립니다.",
    },
  },
  reasons: {
    location: (name: string) => `유지: 보호된 위치(${name})에 있습니다.`,
    rules: {
      "default.review": "검토 필요: Lumen이 알지 못하는 항목입니다.",
      "protect.os-protected": "유지: 운영 체제가 보호하는 항목입니다.",
      "protect.system-file": "유지: Windows가 시스템 파일로 표시한 항목입니다.",
      "protect.cloud-backed":
        "유지: 클라우드 서비스와 동기화되어 있어, 여기서 삭제하면 모든 기기에서 삭제될 수 있습니다.",
      "protect.in-use": "유지: 실행 중인 프로세스가 사용하고 있습니다.",
      "protect.in-use-partial":
        "유지: Lumen이 확인할 수 있는 프로세스 중에는 사용하는 것이 없지만, 모든 프로세스를 확인하지는 못했습니다.",
      "protect.in-use-unknown": "유지: 사용 중인지 Lumen이 아직 확인하지 않았습니다.",
      "protect.not-observed": "유지: 스캔에서 이 항목을 확인하지 못했습니다.",
      "protect.incomplete-scan": "유지: 일부를 읽을 수 없어 내용을 알 수 없습니다.",
      "protect.location-data": "유지: Lumen이 보호 위치 목록을 확인하지 못했습니다.",
      "protect.location-context": "유지: Lumen이 보호 위치 목록을 확인하지 못했습니다.",
      "eligible.quarantine-unsupported":
        "검토만 가능: 이 기기에서는 되돌릴 수 있는 방식으로 격리할 수 없습니다.",
      "eligible.frees-nothing":
        "검토만 가능: 지금 삭제해도 공간이 확보되지 않습니다(하드 링크, 복제본, 스냅샷 또는 클라우드 전용 데이터).",
      "eligible.object-kind": "검토만 가능: 데이터가 아니라 링크 또는 특수 파일입니다.",
      "eligible.in-use-partial":
        "Lumen이 확인할 수 있는 프로세스 중에는 사용하는 것이 없습니다. 시스템 프로세스는 확인하지 못했으므로, 옮기기 직전에 다시 확인합니다.",
      "classify.devkb-regenerable": "격리 제안: 이 항목을 만든 도구가 다시 생성할 수 있습니다.",
      "classify.devkb-tool-action":
        "검토 필요: 해당 도구의 자체 정리 명령으로만 삭제해야 하며, 이 작업은 되돌릴 수 없습니다.",
      "classify.devkb-not-regenerable": "검토 필요: 다시 생성할 수 없는 항목입니다.",
      "classify.devkb-self-managed": "유지: 이 항목을 만든 도구가 자동으로 정리합니다.",
      "classify.devkb-unknown":
        "검토 필요: 이 항목과 일치한 지식 베이스 항목을 사용할 수 없습니다.",
      "classify.vendor-declared-cache": "검토 필요: 제공 업체가 내용을 삭제해도 된다고 밝혔습니다.",
      "classify.app-cache": "격리 제안: 앱의 캐시이며, 앱이 다시 만듭니다.",
      "classify.app-state": "검토 필요: 앱 고유의 데이터, 설정 또는 문서가 들어 있습니다.",
      "classify.orphan": "검토 필요: 이 항목이 속한 앱이 삭제된 것으로 보입니다.",
      "jev.demote": "검토 필요: 선택 사항인 AI 검토가 주의를 권고했습니다.",
    },
  },
} satisfies Messages;
