use super::plan::{Action, Plan};
use crate::cli::dotfile::Prompt;
use anyhow::{bail, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, IsTerminal};

pub fn confirm(plan: &Plan, prompt: Prompt) -> Result<bool> {
    if matches!(prompt, Prompt::Never) {
        return Ok(false);
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        if matches!(prompt, Prompt::Always) {
            bail!("Interactive conflict handling requires a terminal; use --force for an explicit noninteractive override");
        }
        return Ok(false);
    }
    terminal::enable_raw_mode()?;
    let _restore = Restore;
    execute!(io::stderr(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stderr()))?;
    let paths = plan
        .changes
        .iter()
        .filter(|change| change.action == Action::Conflict)
        .map(|change| change.entry.relative.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    loop {
        terminal.draw(|frame| {
            let text=format!("Conflicting destination entries:\n\n{paths}\n\n[y] Back up and overwrite\n[n / Esc / Enter] Cancel");
            frame.render_widget(Paragraph::new(text).block(Block::default().title("stool dotfile").borders(Borders::ALL)).wrap(Wrap {trim:false}),frame.size());
        })?;
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('y' | 'Y') => return Ok(true),
                KeyCode::Char('n' | 'N' | 'c') | KeyCode::Esc | KeyCode::Enter => return Ok(false),
                _ => {}
            }
        }
    }
}
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), LeaveAlternateScreen, crossterm::cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}
