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

/**
 * The Korean catalog, checked against the English one's shape. It speaks
 * 해요체 throughout (docs/design/ux-writing.md): active, positive, no "~시"
 * honorifics, and "닫기" to dismiss a dialog.
 */
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
      low: "디스크가 거의 찼어요. 남은 공간이 10%보다 적어요.",
      loadFailed: "디스크 사용량을 읽지 못했어요.",
    },
    session: {
      title: "최근 검사",
      canFree: "정리 제안",
      command: "명령으로 정리",
      review: "검토 필요",
      quarantine: "격리함",
      atLeast: (size: string) => `최소 ${size}`,
      found: (n: string) => `${n}개 찾았어요`,
      showProgress: "진행 상황 보기",
      cleaned: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮겼어요.`,
      none: "아직 검사하지 않았어요. 디스크를 검사하면 정리할 수 있는 항목을 보여 드려요.",
    },
  },
  start: {
    title: "아직 검사하지 않았어요",
    promise: "디스크 전체나 원하는 폴더만 검사할 수 있어요. 무엇이든 옮기기 전에 먼저 물어볼게요.",
    wholeDiskHint:
      "검사는 읽기만 하고, 몇 분 걸려요. 일부 폴더는 컴퓨터가 접근을 허용할지 물어볼 수 있어요.",
    chooseFolders: "일부 폴더만 검사",
    foldersLabel: "검사할 폴더(한 줄에 하나씩)",
    foldersEmpty: "폴더를 하나 이상 입력해 주세요.",
    scanFolders: "입력한 폴더 검사",
    failed: "검사를 끝내지 못했어요. 파일은 모두 그대로 있어요.",
  },
  ai: {
    title: "AI 검토",
    offIntro:
      "Lumen이 정리를 제안한 항목을 AI가 한 번 더 살펴보고, 검토할 항목이 무엇인지 알려 줘요. AI는 Lumen을 더 신중하게 만들 수만 있어요.",
    onIntro: (key: string) =>
      `켜져 있어요(${key}). 정리를 제안한 항목과 검토할 항목 최대 20개의 정보를 파일 내용은 빼고 보내요. AI는 Lumen을 더 신중하게 만들 수만 있어요.`,
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
      what: "정리를 제안한 항목과 검토할 항목 중 큰 순서로 최대 20개의 경로(홈 폴더는 ~로 표시), 크기, 경과 기간, 판단 근거를 보내요. 파일 내용은 보내지 않아요.",
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
  },
  scanning: {
    title: "검사하는 중…",
    deciding: "판단하는 중…",
    decidingHint: "찾은 항목에 안전 규칙을 적용하고 있어요.",
    readOnly: "읽기만 하고, 아무것도 바꾸지 않아요.",
    progress: "검사 진행 상황",
    items: "찾은 항목",
    size: "디스크 사용량",
    unreadable: "읽지 못한 폴더",
    reading: "읽는 중",
    failed: "찾은 항목을 판단하지 못했어요. 파일은 모두 그대로 있어요.",
  },
  results: {
    canFree: (size: string) => `${size}를 정리할 수 있어요`,
    nothingTitle: "정리할 항목이 없어요",
    nothing: "Lumen은 치워 두어도 안전하다고 확신하는 항목만 제안해요.",
    filter: "표시",
    filters: {
      all: "전체",
      quarantine: "정리 제안",
      command: "명령으로 정리",
      review: "검토 필요",
      keep: "유지",
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
      "도구의 명령으로만 정리할 수 있는 항목이에요. Lumen은 명령을 실행하지 않아요. 실행하기 전에 무엇을 지우는지 확인해 주세요.",
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
    partial: "검사가 중간에 멈췄어요. 그때까지 찾은 항목만 보여 드려요.",
    outOfSpace:
      "디스크 공간이 거의 없어서 검사를 멈췄어요(Lumen은 늘 2 GB를 비워 둬요). 그때까지 찾은 항목만 보여 드려요. 공간을 조금 비운 뒤 다시 검사해 주세요.",
  },
  action: {
    cleanUp: "정리하기…",
    preparing: "준비하는 중…",
    nothing: "정리를 제안한 항목이 없어요.",
    planFailed: "정리를 준비하지 못했어요. 파일은 모두 그대로 있어요.",
  },
  detail: {
    size: "확보할 수 있는 공간",
    location: "위치",
    decision: "상태",
    reasons: "이유",
    aiTitle: "AI 검토가 주의를 권했어요.",
    aiBody: "그래서 정리를 제안하지 않고, 직접 검토하도록 남겨 두었어요.",
    aiNoteHint: "AI 검토의 추측이에요. Lumen의 판단은 바뀌지 않아요.",
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
    title: (tool: string) => `${tool}에서 직접 정리할 수 있어요`,
    copy: "복사",
    copied: "복사했어요",
    copyFailed: "복사하지 못했어요. 명령을 선택해서 직접 복사해 주세요.",
    note: "Lumen은 이 명령을 실행하지 않아요. 실행하기 전에 무엇을 지우는지 확인해 주세요.",
  },
  coverage: {
    incomplete: "일부 폴더를 읽지 못해서 그대로 두었어요. 실제 크기는 더 클 수 있어요.",
    howTo: "전체 디스크 접근 권한을 허용하는 방법",
  },
  details: {
    title: "검사 세부 정보",
    entries: (entries: string) => `읽은 항목 ${count(entries)}개`,
    files: "파일",
    fileCount: (files: string) => `${count(files)}개`,
    size: "크기",
    sizeValue: (logical: string, allocated: string) => `${logical}(디스크 사용 ${allocated})`,
    reclaimable: "확보할 수 있는 공간",
    atLeast: (size: string) => `최소 ${size}`,
    complete: "모두 읽었어요.",
    scanOther: "다른 폴더 검사",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `일부를 읽지 못했어요(접근 거부 ${count(denied)}, 실패 ${count(failed)}, 검사 안 함 ${count(notScanned)}).`,
  },
  confirm: {
    title: (n: number, size: string) => `${items(n)}(${size})를 격리함으로 옮길까요?`,
    checked: "옮기기 직전에 하나씩 다시 확인하고, 검사 뒤에 바뀐 항목은 그대로 둬요.",
    restorable: "영구 삭제하기 전까지 최소 30일 동안 격리함에서 모두 복원할 수 있어요.",
    nothingLeft: "이제 옮길 항목이 없어요. 다시 검사해서 정리할 항목을 확인해 주세요.",
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
  },
  item: {
    quarantined: "격리함으로 옮겼어요. 복원할 수 있어요.",
    skipped: (why: string) => `${why} 옮기지 않았어요.`,
    failed: (reason: string, where: string) => `옮기지 못했어요(${reason}). ${where}.`,
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
    goCleanUp: "정리하러 가기",
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
    notFound: "격리함에 없어요. 아무것도 삭제하지 않았어요.",
    failed: (what: string) => `완전히 삭제하지 못했어요. ${what}.`,
    removedNothing: "아무것도 삭제하지 않았어요",
    removedPartly: "일부만 삭제했어요. 다시 시도하면 나머지를 삭제해요",
    removedItem: "항목은 삭제했고, 기록만 남아 있어요",
    maybeInUse:
      "사용 중이거나 사용 중인지 확인할 수 없어서 삭제하지 않았어요. 이 항목을 쓰는 프로그램을 닫고 다시 시도해 주세요.",
    notFinalizable: "이 항목은 복원만 할 수 있어요.",
  },
  settings: {
    title: "설정",
    permissions: {
      title: "권한",
      fda: "전체 디스크 접근 권한",
      fdaIntro:
        "macOS는 메일, 메시지, 다른 앱의 데이터 같은 일부 폴더를 보호해요. 허용해 주면 Lumen이 이 폴더도 읽을 수 있어요.",
      steps: "허용하는 방법:",
      step1: "시스템 설정을 열어 주세요.",
      step2: "개인정보 보호 및 보안 › 전체 디스크 접근 권한으로 이동해 주세요.",
      step3: "Lumen을 켜고 다시 검사해 주세요.",
      open: "시스템 설정 열기",
      openFailed: "시스템 설정을 열지 못했어요. Apple 메뉴에서 직접 열어 주세요.",
      access: "폴더 접근",
      accessIntro:
        "컴퓨터가 일부 폴더에 Lumen이 접근하지 못하게 막을 수 있어요. 그래도 Lumen은 허용된 폴더를 모두 읽어요.",
      unread:
        "읽지 못한 폴더는 비어 있다고 보지 않고 ‘읽지 못함’으로 표시해요. 그 안의 항목은 정리를 제안하지 않아요.",
    },
    about: {
      title: "정보",
      version: "버전",
      local: "이 기기에서만",
      localBody:
        "검사 결과, 판단, 격리함은 모두 이 컴퓨터에만 있어요. 파일 내용은 어디에도 보내지 않아요. AI 검토를 켜면 경로나 크기 같은 정보만 보내요.",
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
      "protect.in-use-partial":
        "유지: Lumen이 확인한 프로그램 중에는 쓰는 곳이 없지만, 모든 프로그램을 확인하지는 못했어요.",
      "protect.in-use-unknown": "유지: 사용 중인지 아직 확인하지 않았어요.",
      "protect.not-observed": "유지: 검사에서 이 항목을 확인하지 못했어요.",
      "protect.incomplete-scan": "유지: 일부를 읽지 못해서 안에 무엇이 있는지 알 수 없어요.",
      "protect.location-data": "유지: 보호 위치 목록을 확인하지 못했어요.",
      "protect.location-context": "유지: 보호 위치 목록을 확인하지 못했어요.",
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
      "classify.app-cache": "정리 제안: 앱의 캐시라서 앱이 다시 만들어요.",
      "classify.app-state": "유지: 앱의 데이터, 설정, 문서가 들어 있어서 Lumen이 건드리지 않아요.",
      "classify.orphan": "검토 필요: 이 항목을 쓰던 앱이 삭제된 것 같아요.",
      "jev.demote": "검토 필요: AI 검토가 주의를 권했어요.",
    },
  },
} satisfies Messages;
