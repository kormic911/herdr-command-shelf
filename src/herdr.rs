use crate::state::{Store, Target};
use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env,
    path::Path,
    process::{Command, Output},
};

fn output(args: &[&str]) -> Result<Output> {
    Command::new(env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into()))
        .args(args)
        .output()
        .context("Starting Herdr")
}
pub fn call(args: &[&str]) -> Result<Value> {
    let o = output(args)?;
    // Herdr 0.9.5 acknowledges pane run with exit status alone.
    if args.starts_with(&["pane", "run"]) && o.status.success() && o.stdout.is_empty() {
        return Ok(Value::Null);
    }
    let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
    if !o.status.success() || v.get("error").is_some() {
        bail!(
            "{}",
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or(String::from_utf8_lossy(&o.stderr).trim())
        );
    }
    v.get("result").cloned().context("Missing Herdr result")
}
#[derive(Deserialize, Clone)]
pub struct Pane {
    pub pane_id: String,
    pub terminal_id: String,
    pub tab_id: String,
    pub label: Option<String>,
    title: Option<String>,
    terminal_title_stripped: Option<String>,
    terminal_title: Option<String>,
    agent: Option<String>,
    display_agent: Option<String>,
    agent_session: Option<Value>,
    agent_status: String,
}
impl Pane {
    pub fn target(&self) -> Target {
        Target {
            pane: self.pane_id.clone(),
            terminal: self.terminal_id.clone(),
        }
    }
    pub fn name(&self) -> String {
        [
            &self.label,
            &self.title,
            &self.terminal_title_stripped,
            &self.terminal_title,
        ]
        .into_iter()
        .filter_map(|value| value.as_deref())
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or(&self.pane_id)
        .to_owned()
    }
    pub fn ready(&self) -> Result<()> {
        if let Some(agent) = self.agent.as_ref().or(self.display_agent.as_ref()) {
            bail!("{agent} ({})", self.agent_status)
        }
        ensure!(
            self.agent_session.is_none() && self.agent_status == "unknown",
            "Agent state: {}",
            self.agent_status
        );
        Ok(())
    }
}
pub fn panes() -> Result<Vec<Pane>> {
    Ok(serde_json::from_value(
        call(&["pane", "list"])?["panes"].clone(),
    )?)
}
pub fn get(id: &str) -> Result<Pane> {
    Ok(serde_json::from_value(
        call(&["pane", "get", id])?["pane"].clone(),
    )?)
}
pub fn origin() -> Result<Pane> {
    let ctx: Value = serde_json::from_str(
        &env::var("HERDR_PLUGIN_CONTEXT_JSON").unwrap_or_else(|_| "{}".into()),
    )?;
    let id = ctx["focused_pane_id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| env::var("HERDR_PANE_ID").ok())
        .context("Invoke the action from a shell pane")?;
    get(&id)
}
pub fn verify(root: &Path, tab: &str, t: &Target, p: &Pane) -> Result<()> {
    ensure!(p.tab_id == tab, "Target moved to another tab");
    ensure!(p.target() == *t, "Target closed or replaced");
    ensure!(
        !Store::new(root, &p.terminal_id)?.live()?,
        "Target is a Command Shelf"
    );
    p.ready()
}
pub fn run(root: &Path, tab: &str, t: &Target, command: &str) -> Result<()> {
    verify(root, tab, t, &get(&t.pane)?)?;
    call(&["pane", "run", &t.pane, command])?;
    Ok(())
}
#[derive(Serialize, Deserialize)]
pub enum Dialog {
    Editor {
        tab: String,
        index: Option<usize>,
    },
    Delete {
        tab: String,
        index: usize,
    },
    Run {
        tab: String,
        index: usize,
        fingerprint: String,
        target: Target,
    },
}
pub fn dialog(d: Dialog) -> Result<()> {
    let payload = format!("COMMAND_SHELF_DIALOG={}", serde_json::to_string(&d)?);
    let mode = match d {
        Dialog::Editor { index: Some(_), .. } => "edit-command",
        Dialog::Editor { index: None, .. } => "new-command",
        Dialog::Delete { .. } => "delete-command",
        Dialog::Run { .. } => "confirm-run",
    };
    let entrypoint = if cfg!(windows) {
        format!("{mode}-windows")
    } else {
        mode.to_owned()
    };
    let args = [
        "plugin",
        "pane",
        "open",
        "--plugin",
        "herdr-command-shelf",
        "--entrypoint",
        &entrypoint,
        "--env",
        &payload,
    ];
    call(&args)?;
    Ok(())
}
pub fn shelves(root: &Path, tab: &str) -> Result<Vec<Pane>> {
    let mut result = Vec::new();
    for p in panes()? {
        if p.tab_id == tab && Store::new(root, &p.terminal_id)?.live()? {
            result.push(p)
        }
    }
    Ok(result)
}
pub fn bind(root: &Path, p: Pane) -> Result<()> {
    ensure!(
        !Store::new(root, &p.terminal_id)?.live()?,
        "Invoke Bind from the target shell"
    );
    let store = Store::tab(root, &p.tab_id)?;
    ensure!(store.live()?, "Open the tab’s Command Shelf first");
    store.bind(&p.target())
}
pub fn open(root: &Path, p: Pane) -> Result<()> {
    let store = Store::tab(root, &p.tab_id)?;
    let _launch = store.launch_lock()?;
    let shelves = shelves(root, &p.tab_id)?;
    if let Some(existing) = shelves.first() {
        if existing.pane_id != p.pane_id {
            let layout = call(&["pane", "layout", "--pane", &p.pane_id])?;
            call(&["pane", "zoom", &existing.pane_id, "--on"])?;
            if layout.pointer("/layout/zoomed").and_then(Value::as_bool) != Some(true) {
                call(&["pane", "zoom", &existing.pane_id, "--off"])?;
            }
        }
        return Ok(());
    }
    launch(root, &p)
}

// Startup must never focus a shelf or substitute another pane for its host.
pub fn restore(root: &Path) -> Result<()> {
    let mut failures = Vec::new();
    for pane in panes()?
        .into_iter()
        .filter(|p| p.label.as_deref() == Some("Command Shelf"))
    {
        if let Err(error) = restore_pane(root, &pane) {
            failures.push(format!("{}: {error}", pane.pane_id));
        }
    }
    ensure!(failures.is_empty(), "{}", failures.join(", "));
    Ok(())
}

fn restore_pane(root: &Path, pane: &Pane) -> Result<()> {
    let store = Store::tab(root, &pane.tab_id)?;
    let matches_host = || -> Result<bool> {
        Ok(store
            .placement()?
            .is_some_and(|placement| placement.pane == pane.pane_id))
    };
    if !matches_host()? {
        return Ok(());
    }
    let _launch = store.launch_lock()?;
    let current = get(&pane.pane_id)?;
    if store.live()?
        || !matches_host()?
        || current.tab_id != pane.tab_id
        || current.target() != pane.target()
        || current.label.as_deref() != Some("Command Shelf")
        || current.ready().is_err()
    {
        return Ok(());
    }
    let info = call(&["pane", "process-info", "--pane", &current.pane_id])?;
    if !foreground_shell(&info["process_info"]) {
        return Ok(());
    }
    launch(root, &current)
}

fn foreground_shell(info: &Value) -> bool {
    let Some(pid) = info["shell_pid"].as_u64() else {
        return false;
    };
    info["foreground_process_group_id"].as_u64() == Some(pid)
        && info["foreground_processes"]
            .as_array()
            .is_some_and(|processes| {
                processes.len() == 1
                    && processes[0]["pid"].as_u64() == Some(pid)
                    && processes[0]["name"].as_str().is_some_and(|name| {
                        let lower = name
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or(name)
                            .trim_start_matches('-')
                            .to_ascii_lowercase();
                        matches!(
                            lower.trim_end_matches(".exe"),
                            "sh" | "bash"
                                | "dash"
                                | "zsh"
                                | "fish"
                                | "ksh"
                                | "mksh"
                                | "cmd"
                                | "pwsh"
                                | "powershell"
                        )
                    })
            })
}

fn launch(root: &Path, p: &Pane) -> Result<()> {
    let exe = env::current_exe()?;
    let exe = exe.to_str().context("Invalid executable path")?;
    #[cfg(windows)]
    let path = windows_shell_path(exe);
    #[cfg(windows)]
    let exe = path.as_ref();
    let args = [
        exe,
        "shelf",
        root.to_str().context("Invalid state path")?,
        &p.pane_id,
        &p.terminal_id,
    ];
    let cmd = launch_command(&args)?;
    run(root, &p.tab_id, &p.target(), &cmd)?;
    let pane_store = Store::new(root, &p.terminal_id)?;
    let start = std::time::Instant::now();
    while !pane_store.live()? {
        ensure!(
            start.elapsed() < std::time::Duration::from_secs(8),
            "Shelf did not start in {}. Check that shell for a startup error.",
            p.pane_id
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}
fn launch_command(args: &[&str]) -> Result<String> {
    if args.iter().any(|s| s.chars().any(char::is_control)) {
        return Err(anyhow!("Invalid control character in launch argument"));
    }
    if cfg!(windows) {
        use base64::Engine;
        let script = format!(
            "& {}",
            args.iter()
                .map(|s| quote_word(s, true))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let bytes: Vec<_> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        Ok(format!(
            "powershell.exe -NoLogo -NoProfile -EncodedCommand {}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    } else {
        Ok(args
            .iter()
            .map(|s| quote_word(s, false))
            .collect::<Vec<_>>()
            .join(" "))
    }
}

#[cfg(any(windows, test))]
fn windows_shell_path(path: &str) -> std::borrow::Cow<'_, str> {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}").into();
    }
    if let Some(drive) = path.strip_prefix(r"\\?\") {
        if drive.as_bytes().get(1..3) == Some(b":\\") {
            return drive.into();
        }
    }
    path.into()
}

fn quote_word(word: &str, powershell: bool) -> String {
    let mut quoted = String::from("'");
    for c in word.chars() {
        match c {
            '\'' | '‘'..='‛' if powershell => {
                quoted.push(c);
                quoted.push(c);
            }
            '\'' if !powershell => quoted.push_str("'\\''"),
            // Escape outside single quotes so both POSIX shells and fish preserve it.
            '\\' if !powershell => quoted.push_str("'\\\\'"),
            _ => quoted.push(c),
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_requires_a_foreground_shell() {
        let mut info = serde_json::json!({"shell_pid": 10, "foreground_process_group_id": 10,
            "foreground_processes": [{"pid":10,"name":"pwsh.exe"}]});
        assert!(foreground_shell(&info));
        info["foreground_process_group_id"] = 11.into();
        assert!(!foreground_shell(&info));
        info["foreground_process_group_id"] = 10.into();
        info["foreground_processes"][0]["name"] = "vim".into();
        assert!(!foreground_shell(&info));
        assert!(!foreground_shell(&Value::Null));
    }

    #[test]
    fn windows_launcher_paths() {
        assert_eq!(
            windows_shell_path(r"\\?\UNC\server\share\shelf.exe"),
            r"\\server\share\shelf.exe"
        );
        assert_eq!(
            windows_shell_path(r"\\?\D:\Tools\shelf.exe"),
            r"D:\Tools\shelf.exe"
        );
        let volume = r"\\?\Volume{guid}\shelf.exe";
        assert_eq!(windows_shell_path(volume), volume);
    }

    #[test]
    fn launcher_keeps_shell_arguments_literal() {
        let value = r"Build team's ‘release’ ‚test‛ $HOME; & [a] 日本 \\end\";
        #[cfg(unix)]
        {
            let command = launch_command(&["printf", "%s", value]).unwrap();
            for shell in ["sh", "bash", "zsh", "fish"] {
                let mut process = Command::new(shell);
                if shell == "fish" {
                    process.arg("--no-config");
                }
                let output = match process.args(["-c", &command]).output() {
                    Err(e) if shell != "sh" && e.kind() == std::io::ErrorKind::NotFound => continue,
                    other => other.unwrap(),
                };
                assert!(output.status.success(), "{shell}");
                assert_eq!(output.stdout, value.as_bytes(), "{shell}");
            }
        }
        #[cfg(windows)]
        {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("argument.txt");
            // Add-Content detects the BOM even in Windows PowerShell 5.1.
            std::fs::write(&path, b"\xef\xbb\xbf").unwrap();
            let cmd = launch_command(&["Add-Content", path.to_str().unwrap(), value]).unwrap();
            let mut words = cmd.split_ascii_whitespace();
            let result = Command::new(words.next().unwrap())
                .args(words)
                .output()
                .unwrap();
            assert!(result.status.success());
            assert_eq!(
                std::fs::read_to_string(path).unwrap(),
                format!("\u{feff}{value}\r\n")
            );
        }
    }
}
