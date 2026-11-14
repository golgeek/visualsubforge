//! Serialize accelerator publication with preference persistence. Tauri dispatches
//! menu mutations to its UI thread; callers and filesystem work stay on a
//! blocking worker. Settings owns modal admission while the transaction runs.
use crate::preferences::shortcuts::Shortcuts;
use std::sync::Mutex;
use tauri::{Manager, menu::MenuItemKind};
#[derive(Default)]
pub(super) struct Runtime(Mutex<Shortcuts>);

fn publish(app: &tauri::AppHandle, shortcuts: &Shortcuts) -> Result<(), String> {
    let menu = app.menu().ok_or("Native menu unavailable")?;
    let descriptions = super::action_descriptions().map_err(|e| e.to_string())?;
    let submenus = menu.items().map_err(|e| e.to_string())?;
    let items: Vec<_> = descriptions
        .into_iter()
        .filter(|(_, d)| d.native)
        .filter_map(|(id, _)| {
            submenus
                .iter()
                .find_map(|v| v.as_submenu().and_then(|m| m.get(&id)))
                .map(|item| (id, item))
        })
        .collect();
    // Clear first so a swap never temporarily installs duplicate accelerators.
    for (_, item) in &items {
        set(item, None)?;
    }
    for (id, item) in &items {
        set(item, shortcuts.native_key(id))?;
    }
    Ok(())
}
fn set(item: &MenuItemKind<tauri::Wry>, key: Option<&str>) -> Result<(), String> {
    match item {
        MenuItemKind::MenuItem(item) => item.set_accelerator(key),
        MenuItemKind::Check(item) => item.set_accelerator(key),
        // OS-provided Quit/Cut/Copy/Paste keep their native bindings.
        _ => Ok(()),
    }
    .map_err(|e| e.to_string())
}

pub(crate) fn with_shortcuts<T>(
    app: &tauri::AppHandle,
    next: &Shortcuts,
    persist: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    next.validate()?;
    let runtime = app.state::<Runtime>();
    let mut current = runtime.0.lock().map_err(|_| "Shortcut state unavailable")?;
    transition(&mut current, next, |value| publish(app, value), persist)
}

fn transition<T>(
    current: &mut Shortcuts,
    next: &Shortcuts,
    mut publish: impl FnMut(&Shortcuts) -> Result<(), String>,
    persist: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if *current == *next {
        return persist();
    }
    if let Err(error) = publish(next) {
        return match publish(current) {
            Ok(()) => Err(format!("Cannot update native shortcuts: {error}")),
            Err(rollback) => Err(format!(
                "Cannot update native shortcuts: {error}; cannot restore previous accelerators: {rollback}. Restart the app."
            )),
        };
    }
    match persist() {
        Ok(value) => {
            *current = next.clone();
            Ok(value)
        }
        Err(error) => match publish(current) {
            Ok(()) => Err(error),
            Err(rollback) => Err(format!(
                "{error}; cannot restore previous accelerators: {rollback}. Restart the app."
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_publication_and_disk_failures_restore_confirmed_bindings() {
        let initial = Shortcuts::default();
        let mut next = initial.clone();
        next.bindings
            .iter_mut()
            .find(|b| b.action == "splitCue")
            .unwrap()
            .key = "CmdOrCtrl+Shift+L".into();
        for publication_failure in [false, true] {
            let mut current = initial.clone();
            let mut published = Vec::new();
            let mut persisted = false;
            let result: Result<(), String> = transition(
                &mut current,
                &next,
                |value| {
                    published.push(value.clone());
                    if publication_failure && published.len() == 1 {
                        Err("menu failed".into())
                    } else {
                        Ok(())
                    }
                },
                || {
                    persisted = true;
                    Err("disk failed".into())
                },
            );
            assert!(result.is_err());
            assert_eq!(persisted, !publication_failure);
            assert_eq!(published, vec![next.clone(), initial.clone()]);
            assert_eq!(current, initial);
        }
        let mut current = initial.clone();
        let result = transition(&mut current, &next, |_| Ok(()), || Ok(17));
        assert_eq!(result, Ok(17));
        assert_eq!(current, next);
    }

    #[test]
    fn failed_rollback_is_explicit_and_does_not_claim_new_confirmation() {
        let mut current = Shortcuts::default();
        let before = current.clone();
        let mut next = before.clone();
        next.bindings.pop();
        let result: Result<(), String> = transition(
            &mut current,
            &next,
            |_| Err("unavailable".into()),
            || panic!("must not persist"),
        );
        assert!(
            result
                .unwrap_err()
                .contains("cannot restore previous accelerators")
        );
        assert_eq!(current, before);
    }
}
