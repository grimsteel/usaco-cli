use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use std::time::Duration;

pub struct StatusSpinner<'a> {
    multi: &'a MultiProgress,
    bar: ProgressBar,
}

impl<'a> StatusSpinner<'a> {
    pub fn new(loading: &str, multi: &'a MultiProgress) -> Self {
        let bar = multi.add(ProgressBar::new_spinner());
        bar.enable_steady_tick(Duration::from_millis(100));
        // set styled message
        bar.set_message(style(loading).yellow().bright().to_string());
        Self { bar, multi }
    }

    pub fn finish(&self, message: &str, success: bool) {
        // show the prefix
        self.bar.set_style(
            ProgressStyle::default_spinner()
                .template("{prefix} {msg}")
                .unwrap(),
        );

        self.bar.set_prefix(
            if success {
                style("✓").green()
            } else {
                style("✕").red()
            }
            .bold()
            .to_string(),
        );

        // show finish message
        self.bar.finish_with_message(
            if success {
                style(message).green()
            } else {
                style(message).red()
            }
            .bright()
            .to_string(),
        );

        self.multi.remove(&self.bar);
    }
}
