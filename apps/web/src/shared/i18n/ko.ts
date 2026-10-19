import type { Messages } from "./en";
import { countFormat } from "./format";

const count = countFormat("ko");

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
    found: "스캔 결과",
    partial: "스캔이 도중에 멈춰 결과가 일부만 있습니다.",
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
