use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub struct Spinner {
    running: Arc<AtomicBool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Spinner {
    pub fn start(message: &'static str) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = tokio::spawn(async move {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut i = 0;
            while running_clone.load(Ordering::Relaxed) {
                print!("\r\x1B[35m{}\x1B[0m {} ", frames[i % frames.len()], message);
                let _ = std::io::stdout().flush();
                i += 1;
                sleep(Duration::from_millis(80)).await;
            }
            // Clear the line when done
            print!("\r\x1B[K");
            let _ = std::io::stdout().flush();
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    pub fn stop(&mut self) {
        if self.running.swap(false, Ordering::Relaxed) {
            if let Some(handle) = self.handle.take() {
                handle.abort();
            }
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}
