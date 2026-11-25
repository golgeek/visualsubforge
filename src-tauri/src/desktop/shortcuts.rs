//! Serialize accelerator publication with preference persistence. Tauri dispatches
//! menu mutations to its UI thread; callers and filesystem work stay on a
//! blocking worker. Settings owns modal admission while the transaction runs.
use crate::preferences::shortcuts::Shortcuts;
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};
use tauri::{Manager, menu::MenuItemKind};
#[derive(Default)]
pub(super) struct Runtime(Mutex<Shortcuts>);

/// A native accelerator fires before the focused WebView text field sees the key.
fn menu_binding<'a>(label: &str, key: Option<&'a str>, mac: bool) -> (String, Option<&'a str>) {
    #[derive(serde::Deserialize)]
    struct TextNavigationKeys {
        macos: BTreeMap<String, String>,
        other: BTreeMap<String, String>,
    }
    static KEYS: LazyLock<TextNavigationKeys> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../src/lib/workspace/text-navigation-keys.json"
        ))
        .expect("valid text navigation keys")
    });
    let keys = if mac { &KEYS.macos } else { &KEYS.other };
    let key = key.filter(|key| !key.is_empty());
    if let Some(hint) = key.and_then(|key| keys.get(key)) {
        (format!("{label} ({hint})"), None)
    } else {
        (label.to_owned(), key)
    }
}

pub(super) fn native_binding<'a>(label: &str, key: Option<&'a str>) -> (String, Option<&'a str>) {
    menu_binding(label, key, cfg!(target_os = "macos"))
}

fn publish(app: &tauri::AppHandle, shortcuts: &Shortcuts) -> Result<(), String> {
    let menu = app.menu().ok_or("Native menu unavailable")?;
    let descriptions = super::action_descriptions().map_err(|e| e.to_string())?;
    fn find(items: Vec<MenuItemKind<tauri::Wry>>, id: &str) -> Option<MenuItemKind<tauri::Wry>> {
        for item in items {
            if item.id().as_ref() == id {
                return Some(item);
            }
            if let Some(submenu) = item.as_submenu()
                && let Some(found) = find(submenu.items().ok()?, id)
            {
                return Some(found);
            }
        }
        None
    }
    let items: Vec<_> = descriptions
        .into_iter()
        .filter(|(_, d)| d.native)
        .filter_map(|(id, description)| {
            find(menu.items().ok()?, &id).map(|item| (id, description.label, item))
        })
        .collect();
    // Clear first so a swap never temporarily installs duplicate accelerators.
    for (_, _, item) in &items {
        set(item, None)?;
    }
    for (id, label, item) in &items {
        let (text, key) = native_binding(label, shortcuts.native_key(id));
        match item {
            MenuItemKind::MenuItem(item) => item.set_text(&text),
            MenuItemKind::Check(item) => item.set_text(&text),
            _ => continue,
        }
        .map_err(|e| e.to_string())?;
        set(item, key)?;
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
    fn text_navigation_is_shown_in_native_menus_without_intercepting_the_chord() {
        for (mac, key, hint) in [
            (true, "Alt+Left", "⌥←"),
            (true, "Alt+Shift+Right", "⌥⇧→"),
            (true, "CmdOrCtrl+Shift+Up", "⇧⌘↑"),
            (false, "CmdOrCtrl+Right", "Ctrl+→"),
            (false, "Shift+End", "Shift+End"),
        ] {
            assert_eq!(
                menu_binding("Seek", Some(key), mac),
                (format!("Seek ({hint})"), None)
            );
        }
        for mac in [false, true] {
            assert_eq!(
                menu_binding("Save", Some("CmdOrCtrl+S"), mac),
                ("Save".into(), Some("CmdOrCtrl+S"))
            );
            assert_eq!(menu_binding("Seek", None, mac), ("Seek".into(), None));
        }
        assert_eq!(
            menu_binding("Seek", Some("Alt+Left"), false),
            ("Seek".into(), Some("Alt+Left"))
        );
    }

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
