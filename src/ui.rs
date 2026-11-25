use crate::{
    herdr::{self, Dialog, Pane},
    state::{Entry, Store, Target},
    theme::{Palette, Theme},
};
use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode as K, KeyEventKind, KeyModifiers as M, MouseButton, MouseEventKind,
    },
    execute,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::{
    io::{self, Stdout},
    path::Path,
    time::{Duration, Instant},
};
use tui_textarea::TextArea;
use unicode_truncate::UnicodeTruncateStr;
type Term = Terminal<CrosstermBackend<Stdout>>;
pub fn terminal(f: impl FnOnce(&mut Term) -> Result<()>) -> Result<()> {
    let mut t = ratatui::init();
    let result = (|| {
        execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
        f(&mut t)
    })();
    let cleanup = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
    ratatui::restore();
    result.and(cleanup.map_err(Into::into))
}
fn key(e: &Event) -> Option<K> {
    match e {
        Event::Key(k) if k.kind == KeyEventKind::Press => Some(k.code),
        _ => None,
    }
}
fn clicked(e: &Event) -> Option<Position> {
    match e {
        Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
            Some((m.column, m.row).into())
        }
        _ => None,
    }
}
fn row(f: &mut Frame, area: Rect, text: &str, style: Style) {
    f.render_widget(Paragraph::new(text).style(style), area)
}
fn line(area: Rect, y: u16) -> Rect {
    Rect::new(area.x, y, area.width, 1)
}
fn command_preview(command: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let (first, multiline) = match command.split_once(['\r', '\n']) {
        Some((first, _)) => (first, true),
        None => (command, false),
    };
    let first = first.replace('\t', "    ");
    let (visible, _) = first.unicode_truncate(width);
    if !multiline && visible == first {
        return first;
    }
    let (prefix, _) = visible.unicode_truncate(width - 1);
    format!("{prefix}…")
}
fn center(a: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(a.width);
    let h = h.min(a.height);
    Rect::new(a.x + (a.width - w) / 2, a.y + (a.height - h) / 2, w, h)
}
fn target_status(root: &Path, tab: &str, target: &Option<Target>) -> (String, String, bool) {
    let Some(t) = target else {
        return (
            "Choose a shell".into(),
            "Use the bind shortcut in the target pane".into(),
            false,
        );
    };
    match herdr::get(&t.pane) {
        Ok(p) if p.target() == *t => match herdr::verify(root, tab, t, &p) {
            Ok(()) => (p.name(), String::new(), true),
            Err(e) => (p.name(), format!("Run disabled · {e}"), false),
        },
        _ => (
            t.pane.clone(),
            "Target unavailable · bind another shell".into(),
            false,
        ),
    }
}
pub fn shelf(t: &mut Term, root: &Path, s: &Store, p: &Pane, mut theme: Theme) -> Result<()> {
    let mut selected = 0usize;
    let mut first = 0usize;
    let mut message = String::new();
    let mut refresh = Instant::now();
    let mut entries = Vec::new();
    let mut target = None;
    let mut status = (String::new(), String::new(), false);
    loop {
        if Instant::now() >= refresh {
            theme.refresh();
            entries = s.entries()?;
            target = s.target()?;
            if herdr::get(&p.pane_id)
                .is_ok_and(|now| now.tab_id != p.tab_id || now.terminal_id != p.terminal_id)
            {
                return Ok(());
            }
            status = target_status(root, &p.tab_id, &target);
            selected = selected.min(entries.len().saturating_sub(1));
            refresh = Instant::now() + Duration::from_millis(750);
        }
        let palette = theme.palette;
        let mut hits = Vec::new();
        t.draw(|f| {
            let a = f.area();
            let has_selection = !entries.is_empty();
            let runnable = has_selection && status.2;
            let primary = palette.primary;
            let secondary = palette.secondary;
            let buttons = [
                (" ↵ run ", K::Enter, runnable, primary),
                (
                    " n new ",
                    K::Char('n'),
                    true,
                    if runnable { secondary } else { primary },
                ),
                (" e edit ", K::Char('e'), has_selection, secondary),
                (" d delete ", K::Char('d'), has_selection, secondary),
                (" esc close ", K::Esc, true, secondary),
            ];
            let gap = 1;
            let mut rows = vec![(0usize, 0u16)];
            for (i, (label, ..)) in buttons.iter().enumerate() {
                let width = label.chars().count() as u16;
                let (_, used) = rows.last_mut().unwrap();
                if *used == 0 {
                    *used = width;
                } else if *used + gap + width <= a.width {
                    *used += gap + width;
                } else {
                    rows.push((i, width));
                }
            }
            let [header, state, help, body, feedback, actions] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(if status.1.is_empty() { 0 } else { 1 }),
                Constraint::Length(u16::from(
                    has_selection && a.height >= rows.len() as u16 + 4,
                )),
                Constraint::Min(0),
                Constraint::Length(if message.is_empty() { 0 } else { 1 }),
                Constraint::Length(rows.len() as u16),
            ])
            .areas(a);
            row(
                f,
                header,
                &format!("Run in: {}", status.0),
                Style::new().fg(palette.accent).bold(),
            );
            row(f, state, &status.1, Style::new().fg(palette.warning));
            row(f, feedback, &message, Style::new().fg(palette.accent));
            row(f, help, " ↑↓ select command", Style::new().dim());
            for (n, &(start, used)) in rows.iter().take(actions.height as usize).enumerate() {
                let end = rows.get(n + 1).map_or(buttons.len(), |&(i, _)| i);
                let mut x = actions.x + actions.width.saturating_sub(used) / 2;
                for &(label, key, enabled, style) in &buttons[start..end] {
                    let width = label.chars().count() as u16;
                    let rect = Rect::new(
                        x,
                        actions.y + n as u16,
                        width.min(actions.right().saturating_sub(x)),
                        1,
                    );
                    row(
                        f,
                        rect,
                        label,
                        if enabled { style } else { secondary.dim() },
                    );
                    if enabled {
                        hits.push((rect, selected, key));
                    }
                    x = x.saturating_add(width + gap);
                }
            }
            if entries.is_empty() {
                row(f, body, "Press n to save your first command", Style::new());
                return;
            }
            let width = usize::from(body.width.saturating_sub(2));
            let capacity = usize::from(body.height / 2).max(1);
            first = first.clamp(selected.saturating_sub(capacity - 1), selected);
            let mut y = body.y;
            for (i, entry) in entries.iter().enumerate().skip(first) {
                if y >= body.bottom() {
                    break;
                }
                let active = i == selected;
                let title_style = if active {
                    palette.title
                } else {
                    palette.inactive_title
                };
                let head = line(body, y);
                let title = if entry.title.is_empty() {
                    "Command"
                } else {
                    &entry.title
                };
                row(
                    f,
                    head,
                    &format!("{} {title}", if active { "›" } else { " " }),
                    title_style,
                );
                hits.push((head, i, K::Enter));
                y += 1;
                if y < body.bottom() {
                    let preview = line(body, y);
                    row(
                        f,
                        preview,
                        &format!("  {}", command_preview(&entry.command, width)),
                        if active {
                            palette.subtitle
                        } else {
                            palette.inactive_subtitle
                        },
                    );
                    hits.push((preview, i, K::Enter));
                    y += 1;
                }
            }
        })?;
        if !event::poll(refresh.saturating_duration_since(Instant::now()))? {
            continue;
        }
        let e = event::read()?;
        let hit = clicked(&e)
            .and_then(|pos| hits.iter().find(|(r, _, _)| r.contains(pos)))
            .copied();
        let mut k = key(&e);
        if let Some((_, i, action)) = hit {
            selected = i;
            k = Some(action)
        }
        if let Event::Mouse(m) = e {
            match m.kind {
                MouseEventKind::ScrollDown => k = Some(K::Down),
                MouseEventKind::ScrollUp => k = Some(K::Up),
                _ => {}
            }
        }
        let action = match k {
            Some(K::Char('q') | K::Esc) => return s.forget_placement(),
            Some(K::Up) => {
                selected = selected.saturating_sub(1);
                continue;
            }
            Some(K::Down) => {
                selected = (selected + 1).min(entries.len().saturating_sub(1));
                continue;
            }
            Some(K::Char('d') | K::Delete) if !entries.is_empty() => {
                herdr::dialog(Dialog::Delete {
                    tab: p.tab_id.clone(),
                    index: selected,
                })
            }
            Some(K::Char('n')) => herdr::dialog(Dialog::Editor {
                tab: p.tab_id.clone(),
                index: None,
            }),
            Some(K::Char('e')) if !entries.is_empty() => herdr::dialog(Dialog::Editor {
                tab: p.tab_id.clone(),
                index: Some(selected),
            }),
            Some(K::Enter) if !entries.is_empty() && status.2 => (|| {
                let target = target.as_ref().context("Unbound shelf")?;
                let entry = &entries[selected];
                s.check_run(selected, entry, target)?;
                if entry.require_confirmation {
                    message.clear();
                    return herdr::dialog(Dialog::Run {
                        tab: p.tab_id.clone(),
                        index: selected,
                        fingerprint: entry.fingerprint()?,
                        target: target.clone(),
                    });
                }
                herdr::run(root, &p.tab_id, target, &entry.command)?;
                message = "Sent to shell".into();
                Ok(())
            })(),
            _ => continue,
        };
        if let Err(e) = action {
            message = e.to_string()
        } else if k != Some(K::Enter) {
            message.clear()
        }
        refresh = Instant::now();
    }
}
pub fn editor(t: &mut Term, s: &Store, index: Option<usize>, palette: Palette) -> Result<()> {
    let old = index
        .map(|i| {
            s.entries()?
                .get(i)
                .cloned()
                .context("Command no longer exists")
        })
        .transpose()?;
    let mut title = TextArea::from([old.as_ref().map(|e| e.title.as_str()).unwrap_or("")]);
    let mut cmd = TextArea::from(
        old.as_ref()
            .map(|e| e.command.as_str())
            .unwrap_or("")
            .split('\n'),
    );
    #[derive(Clone, Copy, PartialEq)]
    enum Focus {
        Title,
        Command,
        Confirmation,
        Actions,
    }
    let mut require_confirmation = old.as_ref().is_some_and(|entry| entry.require_confirmation);
    let mut focus = Focus::Title;
    let mut error = String::new();
    loop {
        let mut fields = [Rect::default(); 3];
        let mut buttons = [Rect::default(); 2];
        t.draw(|f| {
            let area = f.area();
            f.render_widget(Block::default().style(palette.panel), area);
            let [name, text, checkbox, msg, help, actions] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(if error.is_empty() { 0 } else { 1 }),
                Constraint::Length(u16::from(area.height >= 8)),
                Constraint::Length(1),
            ])
            .areas(area);
            fields = [name, text, checkbox];
            row(
                f,
                checkbox,
                if require_confirmation {
                    "[x] Require confirmation to run"
                } else {
                    "[ ] Require confirmation to run"
                },
                if focus == Focus::Confirmation {
                    palette.title
                } else {
                    palette.panel
                },
            );
            for (field, widget, label, active) in [
                (
                    name,
                    &mut title,
                    " Title (optional) ",
                    focus == Focus::Title,
                ),
                (text, &mut cmd, " Command ", focus == Focus::Command),
            ] {
                widget.set_style(palette.panel);
                widget.set_block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(label)
                        .border_style(if active {
                            Style::new().fg(palette.accent)
                        } else {
                            palette.inactive_subtitle
                        }),
                );
                widget.set_cursor_style(if active {
                    Style::new().reversed()
                } else {
                    Style::new()
                });
                widget.set_cursor_line_style(Style::new());
                f.render_widget(&*widget, field);
            }
            row(f, msg, &error, Style::new().fg(palette.warning));
            let hint = match focus {
                Focus::Title => " tab command field",
                Focus::Command => " tab to confirmation  ·  enter newline",
                Focus::Confirmation => " space to toggle  ·  tab to save",
                Focus::Actions => " s or enter to save  ·  tab to edit",
            };
            row(f, help, hint, palette.inactive_subtitle);
            let strip = center(actions, 21, 1);
            let [save, _, close] = Layout::horizontal([
                Constraint::Length(8),
                Constraint::Length(2),
                Constraint::Length(11),
            ])
            .areas(strip);
            buttons = [save, close];
            row(
                f,
                save,
                " s save ",
                if focus == Focus::Actions {
                    palette.primary.underlined()
                } else {
                    palette.primary
                },
            );
            row(f, close, " esc close ", palette.secondary);
        })?;
        let e = event::read()?;
        if let Event::Mouse(mouse) = e {
            if matches!(
                mouse.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) && fields[1].contains((mouse.column, mouse.row).into())
            {
                cmd.input(mouse);
                continue;
            }
        }
        let mut k = key(&e);
        if let Some(pos) = clicked(&e) {
            if buttons[0].contains(pos) {
                focus = Focus::Actions;
                k = Some(K::Enter);
            } else if buttons[1].contains(pos) {
                return Ok(());
            } else {
                if fields[0].contains(pos) {
                    focus = Focus::Title
                } else if fields[1].contains(pos) {
                    focus = Focus::Command
                } else if fields[2].contains(pos) {
                    focus = Focus::Confirmation;
                    require_confirmation = !require_confirmation;
                }
                continue;
            }
        }
        match k {
            Some(K::Esc) => return Ok(()),
            Some(K::Tab | K::BackTab) => {
                focus = match (focus, k == Some(K::BackTab)) {
                    (Focus::Title, false) | (Focus::Confirmation, true) => Focus::Command,
                    (Focus::Command, false) | (Focus::Actions, true) => Focus::Confirmation,
                    (Focus::Confirmation, false) | (Focus::Title, true) => Focus::Actions,
                    _ => Focus::Title,
                };
                continue;
            }
            Some(K::Char(' ') | K::Enter) if focus == Focus::Confirmation => {
                require_confirmation = !require_confirmation;
                continue;
            }
            Some(K::Enter) if focus == Focus::Title => {
                focus = Focus::Command;
                continue;
            }
            Some(K::Enter | K::Char('s'))
                if focus == Focus::Actions
                    || matches!(e,Event::Key(k) if k.code==K::Char('s') && k.modifiers.contains(M::CONTROL)) =>
            {
                let entry = Entry {
                    title: title.lines().join(" "),
                    command: cmd.lines().join("\n"),
                    require_confirmation,
                };
                match s.save(index.zip(old.as_ref()), entry) {
                    Ok(()) => return Ok(()),
                    Err(e) => error = e.to_string(),
                }
                continue;
            }
            _ => {}
        }
        let widget = match focus {
            Focus::Title => &mut title,
            Focus::Command => &mut cmd,
            Focus::Actions | Focus::Confirmation => continue,
        };
        match e {
            Event::Paste(s) => {
                let s = s.replace("\r\n", "\n").replace('\r', "\n");
                widget.insert_str(if focus == Focus::Title {
                    s.replace('\n', " ")
                } else {
                    s
                });
            }
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                widget.input(k);
            }
            _ => {}
        }
    }
}
pub fn confirm(
    t: &mut Term,
    entry: &Entry,
    prompt: Option<&str>,
    button: &str,
    palette: Palette,
    confirm_action: impl Fn() -> Result<()>,
) -> Result<()> {
    let mut error = String::new();
    let mut scroll = 0u16;
    loop {
        let mut buttons = [Rect::default(); 2];
        let mut max_scroll = 0;
        let mut page = 1;
        t.draw(|f| {
            let area = f.area();
            f.render_widget(Block::default().style(palette.panel), area);
            let [question, title, body, message, actions] = Layout::vertical([
                Constraint::Length(u16::from(prompt.is_some())),
                Constraint::Length(if prompt.is_some() {
                    3
                } else {
                    u16::from(!entry.title.is_empty())
                }),
                Constraint::Min(1),
                Constraint::Length(u16::from(!error.is_empty())),
                Constraint::Length(1),
            ])
            .areas(area);
            row(f, question, prompt.unwrap_or_default(), palette.panel);
            if prompt.is_some() {
                f.render_widget(
                    Paragraph::new(entry.title.as_str())
                        .style(palette.panel)
                        .block(
                            Block::bordered()
                                .title(" Title ")
                                .border_style(palette.inactive_subtitle),
                        ),
                    title,
                );
            } else {
                row(f, title, &entry.title, palette.panel.bold());
            }
            let block = Block::bordered()
                .title(" Command ")
                .border_style(palette.inactive_subtitle);
            let inner = block.inner(body);
            page = inner.height.max(1);
            f.render_widget(block, body);
            let command = Paragraph::new(entry.command.as_str())
                .style(palette.panel)
                .wrap(ratatui::widgets::Wrap { trim: false });
            max_scroll = command
                .line_count(inner.width)
                .saturating_sub(inner.height as usize)
                .min(u16::MAX as usize) as u16;
            scroll = scroll.min(max_scroll);
            f.render_widget(command.scroll((scroll, 0)), inner);
            row(f, message, &error, Style::new().fg(palette.warning));
            let width = button.len() as u16;
            let strip = center(actions, width + 14, 1);
            let [accept, _, cancel] = Layout::horizontal([
                Constraint::Length(width),
                Constraint::Length(2),
                Constraint::Length(12),
            ])
            .areas(strip);
            buttons = [accept, cancel];
            row(f, accept, button, palette.primary);
            row(f, cancel, " esc cancel ", palette.secondary);
        })?;
        let event = event::read()?;
        let mut action = key(&event);
        if let Some(pos) = clicked(&event) {
            if buttons[0].contains(pos) {
                action = Some(K::Char('y'))
            } else if buttons[1].contains(pos) {
                action = Some(K::Esc)
            }
        }
        if let Event::Mouse(m) = event {
            match m.kind {
                MouseEventKind::ScrollDown => action = Some(K::Down),
                MouseEventKind::ScrollUp => action = Some(K::Up),
                _ => {}
            }
        }
        match action {
            Some(K::Char('y')) => match confirm_action() {
                Ok(()) => return Ok(()),
                Err(e) => error = e.to_string(),
            },
            Some(K::Esc | K::Char('n')) => return Ok(()),
            Some(K::PageDown) => scroll = scroll.saturating_add(page).min(max_scroll),
            Some(K::PageUp) => scroll = scroll.saturating_sub(page),
            Some(K::Down) => scroll = scroll.saturating_add(1).min(max_scroll),
            Some(K::Up) => scroll = scroll.saturating_sub(1),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::command_preview;
    #[test]
    fn preview_fits_one_terminal_line() {
        assert_eq!(command_preview("cargo test", 10), "cargo test");
        assert_eq!(command_preview("cargo test", 6), "cargo…");
        assert_eq!(command_preview("pwd\r\nls", 20), "pwd…");
        assert_eq!(command_preview("界界界", 4), "界…");
        assert_eq!(command_preview("exit", 1), "…");
        assert_eq!(command_preview("exit", 0), "");
    }
}
