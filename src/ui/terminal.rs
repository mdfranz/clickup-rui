use crate::util::errors::Result;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

/// RAII guard for terminal raw mode, alternate screen, and mouse capture.
/// Restores the terminal to its normal state automatically when dropped,
/// even in the event of a panic.
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    /// Enables raw mode, enters the alternate screen, enables mouse capture,
    /// hides the cursor, and returns the guard holding the Terminal.
    pub fn create() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;
        Ok(Self { terminal })
    }

    /// Access the underlying `Terminal`.
    pub fn inner(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            self.terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}
