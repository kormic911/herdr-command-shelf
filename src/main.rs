mod bindings;
mod herdr;
mod state;
mod theme;
mod ui;
use anyhow::{bail, ensure, Context, Result};
use herdr::Dialog;
use state::Store;
use std::{
    env,
    path::{Path, PathBuf},
};
fn state_dir() -> Result<PathBuf> {
    Ok(env::var_os("HERDR_PLUGIN_STATE_DIR")
        .context("Launch through Herdr")?
        .into())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--version"] => println!("Command Shelf {}", env!("CARGO_PKG_VERSION")),
        ["install-keybindings"] => bindings::install()?,
        ["open"] => herdr::open(&state_dir()?, herdr::origin()?)?,
        ["bind"] => herdr::bind(&state_dir()?, herdr::origin()?)?,
        ["shelf", root, id, terminal] => {
            let root = Path::new(root);
            let p = herdr::get(id)?;
            ensure!(
                p.terminal_id == *terminal && env::var("HERDR_PANE_ID").as_deref() == Ok(*id),
                "Shelf pane identity changed"
            );
            let s = Store::tab(root, &p.tab_id)?;
            let _tab_lease = s.claim()?;
            let mut sources = vec![Store::new(root, terminal)?];
            for other in herdr::panes()? {
                if other.tab_id == p.tab_id && other.terminal_id != *terminal {
                    sources.push(Store::new(root, &other.terminal_id)?);
                }
            }
            s.migrate(&sources)?;
            let _pane_lease = sources[0].claim()?;
            herdr::call(&["pane", "rename", id, "Command Shelf"])?;
            let theme = theme::Theme::load();
            let result = ui::terminal(|t| ui::shelf(t, root, &s, &p, theme));
            if herdr::get(id).is_ok_and(|live| {
                live.target() == p.target() && live.label.as_deref() == Some("Command Shelf")
            }) {
                if let Some(label) = p.label {
                    herdr::call(&["pane", "rename", id, &label])?;
                } else {
                    herdr::call(&["pane", "rename", id, "--clear"])?;
                }
            }
            result?
        }
        ["dialog"] => {
            let root = state_dir()?;
            let dialog = serde_json::from_str(&env::var("COMMAND_SHELF_DIALOG")?)?;
            let palette = theme::Theme::load().palette;
            match dialog {
                Dialog::Editor { tab, index } => {
                    let store = Store::tab(&root, &tab)?;
                    ui::terminal(|t| ui::editor(t, &store, index, palette))?;
                }
                Dialog::Delete { tab, index } => {
                    let store = Store::tab(&root, &tab)?;
                    let entry = store
                        .entries()?
                        .get(index)
                        .cloned()
                        .context("Command no longer exists")?;
                    ui::terminal(|t| {
                        ui::confirm(t, &entry, None, " y delete ", palette, || {
                            store.delete(index, &entry)
                        })
                    })?;
                }
                Dialog::Run {
                    tab,
                    index,
                    fingerprint,
                    target,
                } => {
                    let store = Store::tab(&root, &tab)?;
                    let entry = store
                        .entries()?
                        .get(index)
                        .cloned()
                        .context("Command no longer exists")?;
                    ensure!(
                        entry.require_confirmation && entry.fingerprint()? == fingerprint,
                        "Command changed. Select it again."
                    );
                    let name = herdr::get(&target.pane)
                        .ok()
                        .filter(|p| p.target() == target)
                        .map(|p| p.name())
                        .unwrap_or_else(|| target.pane.clone());
                    let prompt = format!("Run this command in {name}?");
                    ui::terminal(|t| {
                        ui::confirm(t, &entry, Some(&prompt), " y run ", palette, || {
                            store.check_run(index, &entry, &target)?;
                            herdr::run(&root, &tab, &target, &entry.command)
                        })
                    })?;
                }
            }
        }
        _ => bail!("Use the Command Shelf open or bind action in Herdr."),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use state::{Entry, Target};
    #[test]
    fn confirmation_is_opt_in_and_bound_to_the_reviewed_command_and_target() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::tab(dir.path(), "w1:t1")?;
        let legacy: Entry =
            serde_json::from_value(json!({"title":"Tests","command":"cargo test"}))?;
        assert!(!legacy.require_confirmation);
        let guarded = Entry {
            require_confirmation: true,
            ..legacy.clone()
        };
        store.save(None, guarded.clone())?;
        let target = Target {
            pane: "w1:p2".into(),
            terminal: "term_2".into(),
        };
        store.bind(&target)?;
        let reviewed = store.entries()?.remove(0);
        assert_eq!(reviewed, guarded);
        store.check_run(0, &reviewed, &target)?;
        let changed_target = Target {
            terminal: "term_3".into(),
            ..target.clone()
        };
        store.bind(&changed_target)?;
        assert!(store.check_run(0, &reviewed, &target).is_err());
        store.save(Some((0, &reviewed)), legacy)?;
        assert!(store.check_run(0, &reviewed, &changed_target).is_err());
        Ok(())
    }

    #[test]
    fn target_header_uses_one_name_or_the_pane_id() {
        let mut info = json!({"pane_id":"w2:p1","terminal_id":"term_1","tab_id":"w2:t1","cwd":"/repo","agent_status":"unknown"});
        let name = |v: &serde_json::Value| {
            serde_json::from_value::<herdr::Pane>(v.clone())
                .unwrap()
                .name()
        };
        assert_eq!(name(&info), "w2:p1");
        info["label"] = json!(" ");
        info["terminal_title"] = json!("PowerShell");
        assert_eq!(name(&info), "PowerShell");
        info["title"] = json!("Release shell");
        assert_eq!(name(&info), "Release shell");
        info["label"] = json!("Build");
        assert_eq!(name(&info), "Build");
    }
    #[test]
    fn all_agent_prompts_block_input() {
        let plain = json!({"pane_id":"w1:p1","terminal_id":"term_123","tab_id":"w1:t1","agent_status":"unknown"});
        let check = |v| {
            serde_json::from_value::<herdr::Pane>(v)
                .unwrap()
                .ready()
                .is_ok()
        };
        assert!(check(plain.clone()));
        for status in ["idle", "blocked", "working", "done", "unknown"] {
            let mut p = plain.clone();
            p["agent_status"] = json!(status);
            p["agent"] = json!("codex");
            assert!(!check(p));
        }
        for (field, v) in [
            ("display_agent", json!("codex")),
            ("agent_session", json!({})),
            ("agent_status", json!("idle")),
        ] {
            let mut p = plain.clone();
            p[field] = v;
            assert!(!check(p))
        }
    }
    #[test]
    fn tab_migration_and_confirmed_deletion() -> Result<()> {
        let root = tempfile::tempdir()?;
        let a = Store::tab(root.path(), "w1:t1")?;
        let b = Store::tab(root.path(), "w1:t2")?;
        let legacy = Store::new(root.path(), "term_1")?;
        let original = Entry {
            title: "Tests".into(),
            command: "cargo test".into(),
            require_confirmation: false,
        };
        let changed = Entry {
            title: "Build".into(),
            command: "cargo build".into(),
            require_confirmation: false,
        };
        legacy.save(None, original.clone())?;
        let sources = [legacy];
        a.migrate(&sources)?;
        let _lease = a.claim()?;
        assert!(Store::tab(root.path(), "w1:t1")?.claim().is_err());
        let _other = b.claim()?;
        assert!(b.entries()?.is_empty());
        a.save(Some((0, &original)), changed.clone())?;
        assert!(a.delete(0, &original).is_err());
        a.delete(0, &changed)?;
        a.migrate(&sources)?;
        assert!(a.entries()?.is_empty());
        assert_eq!(sources[0].entries()?, vec![original]);
        Ok(())
    }

    #[test]
    fn binding_and_editing_preserve_shelf_ownership() -> Result<()> {
        let root = tempfile::tempdir()?;
        let s = Store::new(root.path(), "term_1")?;
        let other = Store::new(root.path(), "term_2")?;
        let lease = s.claim()?;
        assert!(s.live()?);
        assert!(!other.live()?);
        let entry = Entry {
            title: "Tests".into(),
            command: "cargo fmt\ncargo test".into(),
            require_confirmation: false,
        };
        s.save(None, entry.clone())?;
        s.bind(&Target {
            pane: "w1:p2".into(),
            terminal: "term_2".into(),
        })?;
        assert!(other.entries()?.is_empty());
        let updated = Entry {
            title: "Test".into(),
            command: "cargo test".into(),
            require_confirmation: false,
        };
        s.save(Some((0, &entry)), updated.clone())?;
        assert!(s.save(Some((0, &entry)), entry.clone()).is_err());
        assert_eq!(s.entries()?, vec![updated]);
        drop(lease);
        assert!(!s.live()?);
        Ok(())
    }
}
