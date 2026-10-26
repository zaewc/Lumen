# UX writing

How Lumen talks, in every interface: the desktop app, the mobile app, the website and
the CLI's messages. It adapts Toss's UX writing principles
([Toss Tech, "토스 UX 라이팅 원칙"](https://toss.tech/article/21022) and the
[Apps in Toss consumer UX guide](https://developers-apps-in-toss.toss.im/design/consumer-ux-guide))
to a tool that decides about people's files. It complements
[ui-principles.md](ui-principles.md).

## Values

| Value | For Lumen |
| --- | --- |
| **Clear** | Every sentence understood on first read; one meaning per word. |
| **Concise** | Only what the user needs now, written for scanning. |
| **Casual** | Plain words instead of storage, security or AI jargon; a friendly, not stiff, voice. |
| **Respect** | Never overstate what is freed or understate a risk. Quarantine does not free space until items are removed for good; say so. |
| **Emotional** | Notice how the moment feels (a full disk, a finished clean-up, a failure) and answer it plainly, without drama. |

## Principles (check every screen against these)

1. **Predictable hint:** can the user tell what happens next? Buttons name their result
   ("격리함으로 옮기기", not "확인"); a trailing "…" means a confirmation follows.
2. **Weed cutting:** remove words that change nothing ("앞으로", "현재", "해당", "모든"
   when not needed).
3. **Remove empty sentences:** never repeat the title in the description; never add a
   line because the screen looks empty.
4. **Focus on the key message:** say only what the user needs to decide right now
   (size, count, what is reversible); details go behind Details.
5. **Easy to speak:** it should sound natural read aloud. One message per sentence,
   short breaths, no written-style or Sino-Korean noun chains.
6. **Suggest, never force:** no fear, pressure or guilt. A full disk is a fact, not an
   emergency; turning on AI review is a choice with a neutral "닫기".
7. **Universal words:** no slang, memes or insider terms; facts in everyday language.
8. **Hidden emotion:** after a clean-up, acknowledge it ("4.2 GB를 격리함으로
   옮겼어요"); after a failure, say what still works and what to do.

## Korean rules

- **해요체 everywhere** in the product and on the website ("옮겼어요", "할 수 있어요").
  No 합니다체 and no 반말.
- **Active voice:** "옮겼어요", not "옮겨졌습니다"; "됐어요" becomes "했어요" when someone
  did it. Always "돼요", never "되어요". Passive only for results the user did not
  directly do (expiry, an item that was skipped) or to reassure about sensitive data.
- **Positive phrasing:** tell people what they can do. "영구 삭제하기 전까지 복원할 수
  있어요", not "영구 삭제하면 복원할 수 없습니다". Use the negative only when something is
  truly unavailable, and then say why and what to do.
- **Casual honorifics:** no "~하시겠습니까?", "~시나요?", "~께". Requests use
  "~해 주세요" or a plain verb; if dropping "~시" makes a sentence awkward, rewrite it
  around the thing being asked.
- **No noun chains:** "검사를 완료했어요" becomes "검사했어요"; "권한 허용 필요" becomes
  "권한을 허용해 주세요". Exception: official OS names are quoted exactly
  (시스템 설정 › 개인정보 보호 및 보안 › 전체 디스크 접근 권한).
- **Dialogs:** the dismiss button is **"닫기"**, never "취소" (it reads as cancelling
  the work). The confirming button names the action ("격리함으로 옮기기",
  "영구 삭제", "AI 검토 켜기").
- Units and numbers: "항목 4개", "9.9 GB" (space before the unit), dates through the
  locale formatter.

## English rules

- Sentence case for every label, title and button.
- Active, plain, second person when needed ("Lumen moved 4 items to Quarantine").
- Buttons name the action; "Close" dismisses a dialog, "Cancel" only stops work in
  progress.
- Positive phrasing as in Korean ("You can restore items until you remove them for
  good").

## Lumen's words

Use these and only these, in both languages.

| Korean | English | Meaning |
| --- | --- | --- |
| 검사 | Scan | Read-only look at the disk |
| 정리 제안 | Proposed | The policy proposes quarantine |
| 명령으로 정리 | Clean up with its own command (status: Own command) | Only the tool's own command should remove it; Lumen shows the command and never runs it |
| 검토 필요 | Needs review | The user should look before anything moves |
| 유지 | Kept | Lumen leaves it alone |
| 정리하기 | Clean up | Move proposed items to Quarantine (after confirming) |
| 격리함 | Quarantine | Where moved items wait, restorable |
| 복원 | Restore | Put an item back where it was |
| 영구 삭제 | Remove for good | Delete from Quarantine; cannot be undone |
| AI 검토 | AI review | Optional, metadata-only second opinion (Jeb) |
| 전체 디스크 접근 권한 | Full Disk Access | macOS permission (official name) |

## No dark patterns

- Nothing pops up on launch; the first screen does what the user came for.
- Every request can be declined with an equal, visible choice.
- No fake urgency, guilt, begging or exaggerated savings.
- Icons match the situation: a warning sign only for real warnings, a spinner only
  while the user actually waits.

## Checklist

- [ ] Can the user predict the next screen from the button?
- [ ] Are meaningless words and sentences gone?
- [ ] Is only the message needed now on screen?
- [ ] Does it sound natural read aloud, with no jargon?
- [ ] Is nothing forced, scary or guilt-inducing?
- [ ] Would everyone understand it?
- [ ] Does it acknowledge how the moment feels?
- [ ] Korean: 해요체, active, positive, no "~시" honorifics, no noun chains, "닫기"?
- [ ] Are Lumen's words used exactly as in the table?
