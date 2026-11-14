//! Confirmation drawn by the host, outside the webview (FS-1 in
//! `docs/security/public-beta-readiness.md`).
//!
//! The page's own dialog explains a clean-up, but a compromised page could
//! call the commands directly with the plan's hash it was given. So right
//! before anything is moved, removed for good or run, the host shows a native
//! dialog whose content it builds itself from what it stored: the plan, the
//! held item or the knowledge base's command. Only "OK" there lets the command
//! go on; anything else changes nothing. The page chooses only the language
//! of the fixed sentences ([`Language`]), never what is listed.

use std::path::Path;

use lumen_app::SystemClock;
use lumen_application::ports::Clock as _;
use lumen_domain::{Chooser, OperationId, PlanId, RawPath};
use lumen_store_sqlite::{IndexStore, open_index};
use serde::Deserialize;

use crate::commands::CommandError;

/// Asks the person `title`, `description`, with the action's button; `true`
/// only when they chose it. Tests use a fixed answer.
pub(crate) type Prompt = dyn Fn(&str, &str, &str, &str) -> bool + Send + Sync;

/// The most items a dialog names one by one. Items the person chose are never
/// summarised, so a plan with more of them than this is refused.
const LISTED: usize = 32;

/// The longest description shown, so the dialog stays readable.
const MAX_TEXT: usize = 8192;

/// The page's language, for the dialog's fixed sentences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub(crate) enum Language {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "ko")]
    Ko,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "ru")]
    Ru,
    #[serde(rename = "fr")]
    Fr,
}

/// The dialog's fixed sentences in one language.
struct Text {
    cancel: &'static str,
    move_title: &'static str,
    move_button: &'static str,
    move_count: fn(usize) -> String,
    move_restorable: &'static str,
    move_chosen: fn(usize) -> String,
    move_proposed: fn(usize) -> String,
    uninstall_title: &'static str,
    uninstall_button: &'static str,
    uninstall_note: &'static str,
    finalize_title: &'static str,
    finalize_button: &'static str,
    finalize_note: &'static str,
    finalize_chosen: &'static str,
    tool_title: &'static str,
    tool_button: &'static str,
    tool_note: &'static str,
    more: fn(usize) -> String,
}

#[allow(clippy::too_many_lines, reason = "one catalog entry per language")]
fn text(language: Language) -> Text {
    match language {
        Language::En => Text {
            cancel: "Cancel",
            move_title: "Move to Quarantine?",
            move_button: "Move to Quarantine",
            move_count: |n| format!("Lumen will move {n} item(s) to Quarantine."),
            move_restorable: "You can restore them from Quarantine.",
            move_chosen: |n| {
                format!(
                    "You chose these {n} item(s) yourself; Lumen did not propose them. They may affect apps, and they stay in Quarantine until you restore or remove them:"
                )
            },
            move_proposed: |n| format!("Lumen's proposals: {n} item(s)."),
            uninstall_title: "Move this app to Quarantine?",
            uninstall_button: "Move to Quarantine",
            uninstall_note: "You can restore it from Quarantine.",
            finalize_title: "Remove for good?",
            finalize_button: "Remove for Good",
            finalize_note: "This cannot be undone.",
            finalize_chosen: "You chose this item yourself; Lumen did not propose it.",
            tool_title: "Run this command?",
            tool_button: "Run",
            tool_note: "What it removes cannot be restored.",
            more: |n| format!("…and {n} more"),
        },
        Language::Ko => Text {
            cancel: "취소",
            move_title: "격리함으로 옮길까요?",
            move_button: "격리함으로 옮기기",
            move_count: |n| format!("항목 {n}개를 격리함으로 옮겨요."),
            move_restorable: "격리함에서 복원할 수 있어요.",
            move_chosen: |n| {
                format!(
                    "직접 선택한 항목 {n}개예요. Lumen이 정리를 제안한 항목은 아니에요. 앱 동작에 영향을 줄 수 있고, 직접 복원하거나 영구 삭제할 때까지 격리함에 보관해요:"
                )
            },
            move_proposed: |n| format!("Lumen의 정리 제안: 항목 {n}개"),
            uninstall_title: "이 앱을 격리함으로 옮길까요?",
            uninstall_button: "격리함으로 옮기기",
            uninstall_note: "격리함에서 복원할 수 있어요.",
            finalize_title: "영구 삭제할까요?",
            finalize_button: "영구 삭제",
            finalize_note: "되돌릴 수 없어요.",
            finalize_chosen: "직접 선택한 항목이에요. Lumen이 정리를 제안한 항목은 아니에요.",
            tool_title: "이 명령을 실행할까요?",
            tool_button: "실행",
            tool_note: "명령이 지운 것은 복원할 수 없어요.",
            more: |n| format!("…외 {n}개"),
        },
        Language::Ja => Text {
            cancel: "キャンセル",
            move_title: "隔離に移動しますか？",
            move_button: "隔離に移動",
            move_count: |n| format!("{n} 項目を隔離に移動します。"),
            move_restorable: "隔離から復元できます。",
            move_chosen: |n| {
                format!(
                    "あなたが選んだ {n} 項目です。Lumen が提案したものではありません。アプリに影響する可能性があり、復元するか完全に削除するまで隔離に残ります："
                )
            },
            move_proposed: |n| format!("Lumen の提案：{n} 項目"),
            uninstall_title: "このアプリを隔離に移動しますか？",
            uninstall_button: "隔離に移動",
            uninstall_note: "隔離から復元できます。",
            finalize_title: "完全に削除しますか？",
            finalize_button: "完全に削除",
            finalize_note: "元に戻せません。",
            finalize_chosen: "あなたが選んだ項目です。Lumen が提案したものではありません。",
            tool_title: "このコマンドを実行しますか？",
            tool_button: "実行",
            tool_note: "削除されたものは復元できません。",
            more: |n| format!("…ほか {n} 項目"),
        },
        Language::ZhCn => Text {
            cancel: "取消",
            move_title: "移至隔离区？",
            move_button: "移至隔离区",
            move_count: |n| format!("Lumen 将把 {n} 个项目移至隔离区。"),
            move_restorable: "你可以从隔离区恢复它们。",
            move_chosen: |n| {
                format!(
                    "以下 {n} 个项目是你自己选择的，并非 Lumen 的建议。它们可能影响应用，并会一直留在隔离区，直到你恢复或永久移除："
                )
            },
            move_proposed: |n| format!("Lumen 的建议：{n} 个项目。"),
            uninstall_title: "将此应用移至隔离区？",
            uninstall_button: "移至隔离区",
            uninstall_note: "你可以从隔离区恢复它。",
            finalize_title: "永久移除？",
            finalize_button: "永久移除",
            finalize_note: "此操作无法撤销。",
            finalize_chosen: "此项目是你自己选择的，并非 Lumen 的建议。",
            tool_title: "运行此命令？",
            tool_button: "运行",
            tool_note: "它删除的内容无法恢复。",
            more: |n| format!("……还有 {n} 个"),
        },
        Language::Es => Text {
            cancel: "Cancelar",
            move_title: "¿Mover a Cuarentena?",
            move_button: "Mover a Cuarentena",
            move_count: |n| format!("Lumen moverá {n} elemento(s) a Cuarentena."),
            move_restorable: "Puedes restaurarlos desde Cuarentena.",
            move_chosen: |n| {
                format!(
                    "Elegiste estos {n} elemento(s) tú mismo; Lumen no los propuso. Pueden afectar a apps y se quedan en Cuarentena hasta que los restaures o elimines:"
                )
            },
            move_proposed: |n| format!("Propuestas de Lumen: {n} elemento(s)."),
            uninstall_title: "¿Mover esta app a Cuarentena?",
            uninstall_button: "Mover a Cuarentena",
            uninstall_note: "Puedes restaurarla desde Cuarentena.",
            finalize_title: "¿Eliminar definitivamente?",
            finalize_button: "Eliminar definitivamente",
            finalize_note: "No se puede deshacer.",
            finalize_chosen: "Elegiste este elemento tú mismo; Lumen no lo propuso.",
            tool_title: "¿Ejecutar este comando?",
            tool_button: "Ejecutar",
            tool_note: "Lo que elimine no se podrá restaurar.",
            more: |n| format!("…y {n} más"),
        },
        Language::Ru => Text {
            cancel: "Отмена",
            move_title: "Переместить в карантин?",
            move_button: "Переместить в карантин",
            move_count: |n| format!("Lumen переместит в карантин элементов: {n}."),
            move_restorable: "Их можно восстановить из карантина.",
            move_chosen: |n| {
                format!(
                    "Эти элементы ({n}) вы выбрали сами; Lumen их не предлагал. Они могут влиять на приложения и останутся в карантине, пока вы их не восстановите или не удалите:"
                )
            },
            move_proposed: |n| format!("Предложения Lumen: {n}."),
            uninstall_title: "Переместить это приложение в карантин?",
            uninstall_button: "Переместить в карантин",
            uninstall_note: "Его можно восстановить из карантина.",
            finalize_title: "Удалить окончательно?",
            finalize_button: "Удалить окончательно",
            finalize_note: "Это нельзя отменить.",
            finalize_chosen: "Этот элемент вы выбрали сами; Lumen его не предлагал.",
            tool_title: "Выполнить эту команду?",
            tool_button: "Выполнить",
            tool_note: "Удалённое ею восстановить нельзя.",
            more: |n| format!("…и ещё {n}"),
        },
        Language::Fr => Text {
            cancel: "Annuler",
            move_title: "Déplacer en quarantaine ?",
            move_button: "Déplacer en quarantaine",
            move_count: |n| format!("Lumen va déplacer {n} élément(s) en quarantaine."),
            move_restorable: "Vous pourrez les restaurer depuis la quarantaine.",
            move_chosen: |n| {
                format!(
                    "Vous avez choisi ces {n} élément(s) vous-même ; Lumen ne les a pas proposés. Ils peuvent affecter des apps et restent en quarantaine jusqu'à ce que vous les restauriez ou les supprimiez :"
                )
            },
            move_proposed: |n| format!("Propositions de Lumen : {n} élément(s)."),
            uninstall_title: "Déplacer cette app en quarantaine ?",
            uninstall_button: "Déplacer en quarantaine",
            uninstall_note: "Vous pourrez la restaurer depuis la quarantaine.",
            finalize_title: "Supprimer définitivement ?",
            finalize_button: "Supprimer définitivement",
            finalize_note: "Cette action est irréversible.",
            finalize_chosen: "Vous avez choisi cet élément vous-même ; Lumen ne l'a pas proposé.",
            tool_title: "Exécuter cette commande ?",
            tool_button: "Exécuter",
            tool_note: "Ce qu'elle supprime ne pourra pas être restauré.",
            more: |n| format!("…et {n} de plus"),
        },
    }
}

/// The native dialog. Run on a blocking worker, never on the main thread:
/// rfd hands the dialog to the main thread and waits for the answer.
pub(crate) fn native(title: &str, description: &str, button: &str, cancel: &str) -> bool {
    std::panic::catch_unwind(|| {
        rfd::MessageDialog::new()
            .set_title(title)
            .set_description(description)
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                button.to_owned(),
                cancel.to_owned(),
            ))
            .show()
    })
    .is_ok_and(|answer| {
        answer == rfd::MessageDialogResult::Ok
            || answer == rfd::MessageDialogResult::Custom(button.to_owned())
    })
}

/// Shows the dialog and turns anything but the action into an error that
/// changed nothing. Refuses a description too long to read.
fn ask(
    prompt: &Prompt,
    title: &str,
    description: &str,
    button: &str,
    cancel: &str,
) -> Result<(), CommandError> {
    if description.len() > MAX_TEXT {
        return Err(CommandError::new(
            "too much to confirm in one dialog; nothing was changed",
        ));
    }
    if prompt(title, description, button, cancel) {
        Ok(())
    } else {
        Err(CommandError::new("not confirmed; nothing was changed"))
    }
}

/// Up to [`LISTED`] lines, then how many more.
fn lines(paths: &[String], t: &Text) -> String {
    let mut shown: Vec<String> = paths.iter().take(LISTED).cloned().collect();
    if paths.len() > LISTED {
        shown.push((t.more)(paths.len() - LISTED));
    }
    shown.join("\n")
}

fn shown(path: &RawPath) -> String {
    path.display()
}

/// Confirms executing stored plan `id`, after checking `hash` and its age the
/// way executing it will. An uninstall preview (`uninstall`) names the app; a
/// clean-up lists every item the person chose and how many the policy
/// proposed, with the first of those.
pub(crate) fn plan(
    db: &Path,
    id: PlanId,
    hash: &str,
    uninstall: bool,
    language: Language,
    prompt: &Prompt,
) -> Result<(), CommandError> {
    lumen_app::refuse_privileged().map_err(CommandError::new)?;
    let store = IndexStore::new(open_index(db).map_err(CommandError::new)?);
    let stored = lumen_app::quarantine::checked_plan(&store, id, hash, SystemClock.now())
        .map_err(CommandError::new)?;
    if lumen_application::uninstall::any_uninstall(&stored.decisions) != uninstall {
        return Err(CommandError::new(
            "that proposal cannot run through this action; nothing was changed",
        ));
    }
    let t = text(language);
    let items = stored.plan.items();
    if uninstall {
        let paths: Vec<_> = items.iter().map(|item| shown(&item.path)).collect();
        let description = format!("{}\n\n{}", lines(&paths, &t), t.uninstall_note);
        return ask(
            prompt,
            t.uninstall_title,
            &description,
            t.uninstall_button,
            t.cancel,
        );
    }
    let (chosen, proposed): (Vec<_>, Vec<_>) = items
        .iter()
        .partition(|item| item.chosen_by == Chooser::Person);
    if chosen.len() > LISTED {
        return Err(CommandError::new(
            "too many chosen items to confirm in one dialog; nothing was changed",
        ));
    }
    let mut parts = vec![(t.move_count)(items.len())];
    if !chosen.is_empty() {
        let paths: Vec<_> = chosen.iter().map(|item| shown(&item.path)).collect();
        parts.push(format!(
            "{}\n{}",
            (t.move_chosen)(chosen.len()),
            paths.join("\n")
        ));
    }
    if !proposed.is_empty() {
        let paths: Vec<_> = proposed.iter().map(|item| shown(&item.path)).collect();
        parts.push(format!(
            "{}\n{}",
            (t.move_proposed)(proposed.len()),
            lines(&paths, &t)
        ));
    }
    parts.push(t.move_restorable.to_owned());
    ask(
        prompt,
        t.move_title,
        &parts.join("\n\n"),
        t.move_button,
        t.cancel,
    )
}

/// Confirms removing held item `operation` for good, naming where it was.
pub(crate) fn finalize(
    db: &Path,
    operation: OperationId,
    language: Language,
    prompt: &Prompt,
) -> Result<(), CommandError> {
    let item = lumen_app::held::get(db, operation).map_err(CommandError::new)?;
    let original = item
        .original
        .ok_or_else(|| CommandError::new("where the item was is unknown; nothing was changed"))?;
    let t = text(language);
    let mut parts = vec![original.display().to_string()];
    if item.chosen_by == Some(Chooser::Person) {
        parts.push(t.finalize_chosen.to_owned());
    }
    parts.push(t.finalize_note.to_owned());
    ask(
        prompt,
        t.finalize_title,
        &parts.join("\n\n"),
        t.finalize_button,
        t.cancel,
    )
}

/// Confirms running the knowledge base's `command` for `targets`.
pub(crate) fn tool(
    command: &lumen_app::tools::ToolCommand,
    targets: &[RawPath],
    language: Language,
    prompt: &Prompt,
) -> Result<(), CommandError> {
    let t = text(language);
    let line = std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let paths: Vec<_> = targets.iter().map(shown).collect();
    let description = format!(
        "{} — {line}\n\n{}\n\n{}",
        command.tool,
        lines(&paths, &t),
        t.tool_note
    );
    ask(prompt, t.tool_title, &description, t.tool_button, t.cancel)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn raw(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text.as_bytes().to_vec()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn only_the_action_button_confirms() {
        let yes = |_: &str, _: &str, _: &str, _: &str| true;
        let no = |_: &str, _: &str, _: &str, _: &str| false;
        assert!(ask(&yes, "t", "d", "b", "c").is_ok());
        assert!(ask(&no, "t", "d", "b", "c").is_err());
        let long = "x".repeat(MAX_TEXT + 1);
        assert!(ask(&yes, "t", &long, "b", "c").is_err(), "too long to read");
    }

    #[test]
    fn long_lists_say_how_many_more() {
        let paths: Vec<String> = (0..40).map(|i| format!("/p/{i}")).collect();
        let t = text(Language::En);
        let shown = lines(&paths, &t);
        assert_eq!(shown.lines().count(), LISTED + 1);
        assert!(shown.ends_with("…and 8 more"));
    }

    #[test]
    fn a_tool_dialog_names_the_command_and_its_targets() {
        let seen = std::sync::Arc::new(Mutex::new(String::new()));
        let into = std::sync::Arc::clone(&seen);
        let record = move |title: &str, description: &str, button: &str, cancel: &str| {
            if let Ok(mut s) = into.lock() {
                *s = format!("{title}|{description}|{button}|{cancel}");
            }
            false
        };
        let command = lumen_app::tools::ToolCommand {
            entry: "pnpm-store".into(),
            tool: "pnpm".into(),
            program: "pnpm".into(),
            args: vec!["store".into(), "prune".into()],
            removes_data: false,
        };
        let result = tool(
            &command,
            &[raw("/Users/a/Library/pnpm")],
            Language::Ko,
            &record,
        );
        assert!(result.is_err(), "declined");
        let seen = seen.lock().map(|s| s.clone()).unwrap_or_default();
        assert!(seen.starts_with("이 명령을 실행할까요?|pnpm — pnpm store prune"));
        assert!(seen.contains("/Users/a/Library/pnpm"));
        assert!(seen.ends_with("|실행|취소"));
    }

    #[test]
    fn languages_parse_from_the_page_s_codes() -> Result<(), serde_json::Error> {
        for (code, language) in [
            ("\"en\"", Language::En),
            ("\"ko\"", Language::Ko),
            ("\"zh-CN\"", Language::ZhCn),
        ] {
            assert_eq!(serde_json::from_str::<Language>(code)?, language);
        }
        assert!(serde_json::from_str::<Language>("\"xx\"").is_err());
        Ok(())
    }
}
