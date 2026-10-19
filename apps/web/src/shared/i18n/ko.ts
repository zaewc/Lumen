import type { Messages } from "./en";
import { countFormat } from "./format";

const count = countFormat("ko");

/**
 * The particle "이란" after a final consonant and "란" after a vowel
 * (‘정리 계획’이란, ‘격리’란), read from the last Hangul syllable.
 */
function withIran(word: string): string {
  const last = (word.codePointAt(word.length - 1) ?? 0) - 0xac00;
  return last >= 0 && last < 11_172 && last % 28 === 0 ? "란" : "이란";
}

/** The Korean catalog (합니다체), checked against the English one's shape. */
export const ko = {
  language: "언어",
  nav: {
    quarantine: "격리",
    scan: "스캔",
    plan: "정리 계획",
  },
  home: {
    tagline: "직접 결정하기 전까지 모든 작업을 되돌릴 수 있는 저장 공간 정리 도구입니다.",
    heading: "격리된 항목",
    spaceNote: "격리된 항목도 계속 공간을 차지합니다. 항목을 영구 삭제해야 공간이 확보됩니다.",
    loading: "불러오는 중…",
    empty: "격리된 항목이 없습니다.",
  },
  held: {
    unknownPath: "(알 수 없는 경로)",
    retained: "보관 중",
    heldAfterFailure: "이동 실패 후 보관 중",
    age: (days: number) => `${count(days)}일 경과`,
    restore: "복원",
    removeForGoodAsk: "영구 삭제…",
    removeForGoodWarning: "되돌릴 수 없습니다. 삭제하기 전에 사용 중인지 먼저 확인합니다.",
    removeForGood: "영구 삭제",
    cancel: "취소",
  },
  restore: {
    restored: "원래 위치로 복원했습니다.",
    parentReplaced: "그사이 원래 폴더가 다시 만들어져 있었습니다.",
    unverified: "복원했지만 기록과 정확히 일치하지 않습니다. 내용을 확인하십시오.",
    notRestored: (why: string) => `복원하지 못했습니다: ${why}. 격리 상태로 유지됩니다.`,
    nameTaken:
      "원래 위치에 같은 이름의 항목이 생겼습니다. 그 항목을 다른 곳으로 옮긴 뒤 다시 시도하십시오",
    originUnreachable: "원래 폴더에 안전하게 접근할 수 없습니다",
    itemChanged: "격리된 항목이 변경되었습니다",
    moveFailed: "원래 위치로 옮기지 못했습니다",
    notRestorable: "이 항목은 현재 상태에서 복원할 수 없습니다.",
  },
  finalize: {
    finalized: "영구 삭제했습니다.",
    foundAtSource: "이미 원래 위치로 돌아가 있습니다. 아무것도 삭제하지 않았습니다.",
    notFound: "격리 공간에 없습니다. 아무것도 삭제하지 않았습니다.",
    failed: (what: string) => `완전히 삭제하지 못했습니다: ${what}.`,
    removedNothing: "아무것도 삭제되지 않았습니다",
    removedPartly: "일부만 삭제되었습니다. 다시 시도하면 마저 삭제합니다",
    removedItem: "항목은 삭제되었고 기록만 남아 있습니다",
    maybeInUse:
      "사용 중이거나 사용 여부를 확인할 수 없어 아무것도 삭제하지 않았습니다. 이 항목을 사용하는 프로그램을 닫고 다시 시도하십시오.",
    notFinalizable: "보관 중인 항목만 영구 삭제할 수 있습니다.",
  },
  plan: {
    title: "정리 계획",
    intro:
      "정리 계획에는 최근 스캔에서 정책이 격리를 제안한 항목이 모두 담깁니다. 계획을 만들어도 아무것도 옮기지 않습니다.",
    planning: "계획을 만드는 중…",
    make: "정리 계획 만들기",
    summary: (items: number, size: string) => `항목 ${count(items)}개, ${size}`,
    made: (createdAt: string) => `${createdAt}에 생성되었으며 24시간 동안 유효합니다. 계획 해시`,
    confirm:
      "정확히 이 항목들만 격리 공간으로 옮깁니다. 각 항목은 옮기기 전에 다시 확인하며, 그사이 변경된 항목은 그대로 둡니다. 아무것도 삭제하지 않으며, 직접 영구 삭제하기 전까지는 모든 항목을 복원할 수 있습니다.",
    quarantining: "격리하는 중…",
    execute: (items: number) => `항목 ${count(items)}개 격리`,
    moved: (size: string) => `격리 공간으로 옮긴 용량: ${size}`,
    freedNote: "항목을 영구 삭제해야 공간이 확보됩니다.",
  },
  item: {
    quarantined: "격리 공간으로 옮겼습니다(복원 가능).",
    skipped: (why: string) => `옮기지 않았습니다: ${why}.`,
    failed: (reason: string, where: string) => `이동에 실패했습니다(${reason}). ${where}.`,
    skipChanged: "변경되었거나 더 이상 확인할 수 없습니다",
    skipOutsideRoots: "스캔한 폴더 밖에 있습니다",
    skipNowKeep: "이제 정책상 유지할 항목입니다",
    skipNowReview: "이제 정책상 직접 검토해야 할 항목입니다",
    skipReasonsChanged: "이제는 다른 이유로 제안될 항목입니다",
    skipNotPrepared: "안전하게 옮기기 위한 기록을 남기지 못했습니다",
    skipManifest: "기록을 저장하지 못했습니다",
    leftSource: "원래 위치에 그대로 있습니다",
    leftQuarantine: "격리 공간에 있으며 복원할 수 있습니다",
    leftGone: "항목이 사라졌습니다",
  },
  scan: {
    title: "스캔",
    rootsLabel: "스캔할 폴더(한 줄에 하나씩)",
    readOnly: "스캔은 읽기만 하며 아무것도 변경하지 않습니다.",
    scanning: "스캔하는 중…",
    submit: "스캔",
    wholeDisk: "디스크 전체 스캔",
    progress: (entries: string, folders: string, size: string) =>
      `항목 ${count(entries)}개 · 폴더 ${count(folders)}개 · 디스크 사용 ${size}`,
    unreadableSoFar: (folders: string) => `지금까지 읽지 못한 폴더 ${count(folders)}개`,
    reading: (folder: string) => `읽는 중: ${folder}`,
    wholeDiskHint:
      "시동 디스크의 모든 폴더를 읽으며, 몇 분 걸릴 수 있습니다. 시스템이 일부 폴더에 대한 접근 허용을 물을 수 있으며, 허용하지 않은 폴더는 비어 있는 것이 아니라 읽지 못한 것으로 표시됩니다.",
    found: "스캔 결과",
    partial: "스캔이 도중에 멈춰 결과가 일부만 있습니다.",
    outOfSpace:
      "디스크 여유 공간이 거의 없어 스캔을 멈췄습니다. Lumen은 항상 2 GB를 남겨 둡니다. 결과는 일부만 있으니, 공간을 확보한 뒤 다시 스캔하십시오.",
    assessment: "판정 결과",
    nothingMoved: "아무것도 옮기지 않았습니다.",
    deciding: "판정하는 중…",
  },
  root: {
    files: "파일",
    fileCount: (files: string) => `${count(files)}개`,
    size: "크기",
    sizeValue: (logical: string, allocated: string) => `${logical} (디스크 할당 ${allocated})`,
    reclaimable: "확보 가능한 공간",
    atLeast: (size: string) => `최소 ${size}`,
    complete: "하위 항목을 모두 읽었습니다.",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `일부를 읽지 못했습니다(접근 거부 ${count(denied)}, 실패 ${count(failed)}, 스캔 안 함 ${count(notScanned)}). 표시된 크기는 최솟값이며, 읽지 못한 부분을 비어 있는 것으로 간주하지 않습니다.`,
  },
  candidates: {
    none: "최근 스캔에서 정리 후보를 찾지 못했습니다.",
    quarantine: "격리 제안",
    review: "검토 필요",
    keep: "유지",
    group: (label: string, items: number) => `${label} (${count(items)})`,
  },
  whatIs: (term: string) => `‘${term}’${withIran(term)}?`,
  glossary: {
    scan: {
      title: "스캔",
      body: "스캔은 선택한 폴더를 읽고 그 안에 무엇이 있는지 측정합니다. 읽기만 하므로 아무것도 옮기거나 변경하거나 삭제하지 않습니다. 이름과 크기 같은 파일 정보만 살펴보며, 파일을 열거나 클라우드에만 있는 파일을 내려받지 않습니다.",
    },
    coverage: {
      title: "스캔 범위",
      body: "스캔 범위는 Lumen이 폴더 안의 모든 항목을 읽었는지를 나타냅니다. 모두 읽었다면 표시된 크기가 빠짐없는 값입니다. 일부를 읽지 못했다면(접근 거부, 실패, 스캔 안 함) 표시된 크기는 최솟값입니다. Lumen은 읽지 못한 부분을 비어 있는 것으로 간주하지 않으며, 확인하지 못한 항목은 삭제를 제안하지 않습니다.",
    },
    reclaimable: {
      title: "확보 가능한 공간",
      body: "확보 가능한 공간은 항목을 영구 삭제했을 때 되찾을 수 있는 디스크 공간입니다. 다른 파일과 함께 쓰는 공간(하드 링크, 복제본, 스냅샷)은 포함하지 않으며, 격리만 해서는 공간이 늘어나지 않습니다. ‘최소’는 폴더 일부를 읽지 못해 실제 값이 더 클 수 있다는 뜻입니다. 크기는 파일에 담긴 데이터의 양이고, 디스크 할당은 디스크에서 실제로 차지하는 공간입니다.",
    },
    verdicts: {
      title: "판정",
      body: "Lumen은 추측하지 않고 정해진 안전 정책에 따라 모든 항목을 판정합니다. 각 항목은 ‘유지’, ‘검토 필요’, ‘격리 제안’ 중 하나이며, 그 이유가 항목 아래에 표시됩니다. 확실하지 않으면 항목을 유지하거나 사용자에게 묻고, 정리 계획을 확인하기 전까지는 아무것도 옮기지 않습니다.",
    },
    keep: {
      title: "유지",
      body: "Lumen이 이 항목을 건드리지 않습니다. 시스템이 보호하거나, 사용 중이거나, 클라우드와 동기화되어 있거나, 끝까지 읽지 못했거나, 필요한 항목으로 알려진 경우입니다. 유지 항목은 절대 옮기지 않습니다.",
    },
    review: {
      title: "검토 필요",
      body: "Lumen이 스스로 안전하게 판단할 수 없는 항목입니다. 예를 들어 Lumen이 알지 못하는 항목이거나 앱 고유의 데이터가 들어 있는 경우입니다. 자동으로 옮기지 않으므로, 이유를 읽고 직접 판단하십시오.",
    },
    proposed: {
      title: "격리 제안",
      body: "정책상 따로 치워 두어도 안전하다고 판단한 항목입니다. 예를 들어 도구나 앱이 다시 만드는 캐시가 여기에 해당합니다. 제안만으로는 아무것도 옮기지 않으며, 직접 읽고 확인하는 정리 계획에 담길 뿐입니다. 확인한 뒤에도 격리 공간으로 옮겨지므로 복원할 수 있습니다.",
    },
    plan: {
      title: "정리 계획",
      body: "정리 계획은 최근 스캔에서 격리를 제안한 항목을 고정된 목록으로 묶은 것으로, 확인한 내용 그대로 실행되도록 해시로 봉인됩니다. 24시간 동안 유효하며, 계획을 만들어도 아무것도 옮기지 않습니다. 계획을 확인하면 각 항목을 옮기기 전에 다시 확인하고, 그사이 변경된 항목은 건너뛰어 원래 위치에 그대로 둡니다.",
    },
    quarantine: {
      title: "격리",
      body: "격리는 항목을 삭제하는 대신 Lumen 전용 폴더로 옮겨 두는 것입니다. 격리된 항목은 모두 원래 위치로 복원할 수 있습니다. 격리된 항목은 영구 삭제하기 전까지 계속 디스크 공간을 차지합니다.",
    },
    finalize: {
      title: "영구 삭제",
      body: "영구 삭제는 격리된 항목을 완전히 지우고 공간을 확보합니다. 되돌릴 수 없는 유일한 단계입니다. Lumen은 먼저 항목을 사용하는 프로그램이 없는지 확인하고, 사용 중이거나 확인할 수 없으면 아무것도 삭제하지 않습니다. 보관 중인 항목만 영구 삭제할 수 있습니다.",
    },
    restore: {
      title: "복원",
      body: "복원은 격리된 항목을 원래 위치에 원래 이름으로 되돌립니다. Lumen은 어떤 것도 덮어쓰지 않습니다. 그 자리에 같은 이름의 항목이 생겼거나 원래 폴더에 안전하게 접근할 수 없으면, 항목은 격리 상태로 남고 Lumen이 그 이유를 알려 줍니다.",
    },
    held: {
      title: "보관 상태",
      body: "‘보관 중’은 항목을 격리 공간으로 옮긴 뒤 기록과 일치하는지 확인까지 마친 상태로, 복원하거나 영구 삭제할 때까지 그대로 보관됩니다. ‘이동 실패 후 보관 중’은 옮기는 도중 문제가 생겨, 그 확인을 마치지 못한 채 격리 공간에 안전하게 보관된 상태입니다. 이런 항목은 복원할 수 있지만 영구 삭제할 수는 없습니다.",
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
