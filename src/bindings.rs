use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::NamedTempFile;
use toml::Value;

const BINDINGS: [(&str, &str, &str, &str); 2] = [
    (
        "prefix+f2",
        "ctrl+alt+k",
        "herdr-command-shelf.open",
        "Open Command Shelf here",
    ),
    (
        "prefix+f3",
        "ctrl+alt+b",
        "herdr-command-shelf.bind",
        "Bind this shell to Command Shelf",
    ),
];

fn same_chord(a: &str, b: &str) -> bool {
    fn parts(key: &str) -> Vec<String> {
        let mut parts: Vec<_> = key
            .split('+')
            .map(|s| s.trim().to_ascii_lowercase())
            .collect();
        parts.sort();
        parts
    }
    parts(a) == parts(b)
}
fn additions(original: &str) -> Result<String> {
    let doc = original
        .trim_start_matches('\u{feff}')
        .parse::<toml::Table>()?;
    let keys = doc.get("keys").and_then(Value::as_table);
    let commands = keys
        .and_then(|k| k.get("command"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut text = String::new();
    for (key, previous_default, action, description) in BINDINGS {
        if let Some(existing) = commands.iter().find(|e| {
            e.get("command").and_then(Value::as_str) == Some(action)
                && e.get("type").and_then(Value::as_str) == Some("plugin_action")
                && !e
                    .get("key")
                    .and_then(Value::as_str)
                    .is_some_and(|k| same_chord(k, previous_default))
        }) {
            println!(
                "{action} already uses {}",
                existing
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or("(no key)")
            );
            continue;
        }
        let occupied = commands
            .iter()
            .filter_map(|e| e.get("key").and_then(Value::as_str))
            .chain(keys.into_iter().flat_map(|k| k.values()).flat_map(|v| {
                v.as_str()
                    .into_iter()
                    .chain(v.as_array().into_iter().flatten().filter_map(Value::as_str))
            }))
            .any(|s| same_chord(s, key));
        if occupied {
            println!("Skipped {action}: {key} is already assigned.");
            continue;
        }
        text.push_str(&format!("\n[[keys.command]]\nkey = \"{key}\"\ntype = \"plugin_action\"\ncommand = \"{action}\"\ndescription = \"{description}\"\n"));
    }
    if original.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    Ok(text)
}
fn checked(cmd: &mut Command) -> Result<String> {
    let output = cmd.output().context("Starting Herdr")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    ensure!(
        output.status.success(),
        "Herdr returned {}: {} {}",
        output.status,
        stdout.trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(stdout.into_owned())
}

fn read(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub fn config_path() -> Result<PathBuf> {
    let bin = std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into());
    let help = checked(Command::new(&bin).arg("--help"))?;
    let reported = help
        .lines()
        .find_map(|s| s.strip_prefix("Config: "))
        .context("Herdr did not report its config path")?;
    Ok(std::path::absolute(PathBuf::from(reported))?)
}
pub fn install() -> Result<()> {
    let bin = std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into());
    let path = config_path()?;
    let path = if path.exists() {
        path.canonicalize()?
    } else {
        path
    };
    let parent = path.parent().context("Missing config directory")?;
    fs::create_dir_all(parent)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_added_extension("command-shelf.lock"))?;
    FileExt::lock_exclusive(&lock)?;
    let before = read(&path)?;
    let original = before.as_deref().unwrap_or_default();
    let extra = additions(original)?;
    if !extra.is_empty() {
        let mut candidate = NamedTempFile::new_in(parent)?;
        candidate.write_all(original.as_bytes())?;
        candidate.write_all(extra.as_bytes())?;
        candidate.flush()?;
        checked(
            Command::new(&bin)
                .args(["config", "check"])
                .env("HERDR_CONFIG_PATH", candidate.path()),
        )?;
        if let Ok(meta) = fs::metadata(&path) {
            candidate.as_file().set_permissions(meta.permissions())?;
        }
        ensure!(
            read(&path)? == before,
            "Herdr config changed during installation. Run the action again."
        );
        candidate.persist(&path)?;
        println!("Added missing shortcuts to {}", path.display());
    } else {
        checked(Command::new(&bin).args(["config", "check"]))?;
    }
    checked(Command::new(&bin).args(["server", "reload-config"]))
        .context("Reloading Herdr config")?;
    println!("Reloaded Herdr config.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_installs_and_custom_bindings() -> Result<()> {
        let both = additions("")?;
        assert_eq!(both.matches("[[keys.command]]").count(), 2);
        assert!(additions(&both)?.is_empty());
        assert!(additions(&both.replace("prefix+f2", "f8"))?.is_empty());
        let previous = both
            .replace("prefix+f2", "ctrl+alt+k")
            .replace("prefix+f3", "ctrl+alt+b");
        let upgrade = additions(&previous)?;
        assert_eq!(upgrade, both);
        assert!(additions(&(previous + &upgrade))?.is_empty());
        let one = additions("[keys]\r\nsettings=['prefix+s','Prefix+F2']\r\n")?;
        assert!(one.contains("shelf.bind"));
        assert!(!one.contains("shelf.open"));
        assert!(one.starts_with("\r\n"));
        assert!(
            additions("[[keys.command]]\nkey='ctrl+alt+b'\ntype='shell'\ncommand='other'")?
                .contains("shelf.open")
        );
        assert!(additions("invalid = [").is_err());
        Ok(())
    }
}
