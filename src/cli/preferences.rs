use crate::{
    cli::status_spinner::StatusSpinner,
    preferences::{CPPCompiler, DataStore, Language},
};
use clap::{Subcommand, ValueEnum};
use console::{strip_ansi_codes, style, user_attended};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use indicatif::MultiProgress;
use std::{borrow::Cow, env::current_dir, path::PathBuf};
use tokio::fs::canonicalize;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Read a preference key
    Get {
        /// Preference key to retrieve
        #[arg(value_enum)]
        key: PrefKey,
    },
    /// Set a preference key. Will prompt for value
    Set {
        /// Preference key to set
        #[command(subcommand)]
        key: SetValues,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetValues {
    /// Default problem ID for problem and solution commands
    CurrentProblem { value: Option<u64> },
    /// Preferred language for boilerplate code
    PreferredLanguage {
        #[arg(value_enum)]
        value: Option<Language>,
    },
    /// Preferred C++ compiler
    CPPCompiler {
        #[arg(value_enum)]
        value: Option<CPPCompiler>,
    },
    /// Directory to hold solutions in
    SolutionsDirectory {
        #[arg(value_enum)]
        value: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum PrefKey {
    /// Default problem ID for problem and solution commands
    CurrentProblem,
    /// Preferred language for boilerplate code
    PreferredLanguage,
    /// Preferred C++ compiler
    CPPCompiler,
    /// Directory to hold solutions in
    SolutionsDirectory,
}

pub async fn handle(
    command: Option<Command>,
    prefs: &DataStore,
    multi: MultiProgress,
) -> super::Result {
    match command {
        Some(Command::Get { key }) => {
            let lock = prefs.read()?;
            let value = match key {
                PrefKey::CurrentProblem => lock
                    .current_problem
                    .map(|s| style(Cow::Owned(s.to_string())).cyan())
                    // orange if no problem set
                    .unwrap_or_else(|| style(Cow::Borrowed("Not Set")).color256(215)),
                PrefKey::PreferredLanguage => match lock.preferred_language {
                    Language::CPP => style(Cow::Borrowed("C++")).blue(),
                    Language::Python => style(Cow::Borrowed("Python")).yellow(),
                },
                PrefKey::CPPCompiler => style(match lock.cpp_compiler {
                    CPPCompiler::GCC => Cow::Borrowed("g++"),
                    CPPCompiler::Clang => Cow::Borrowed("clang"),
                })
                .magenta(),
                PrefKey::SolutionsDirectory => match lock.solutions_dir.as_ref() {
                    Some(dir) => style(dir.to_string_lossy()).blue(),
                    None => style(Cow::Borrowed("Not set")).red(),
                }
                .bright()
                .bold(),
            }
            .bright()
            .bold()
            .to_string();
            if user_attended() {
                println!(
                    "{} {}",
                    style(match key {
                        PrefKey::CurrentProblem => "Current problem:",
                        PrefKey::PreferredLanguage => "Preferred language:",
                        PrefKey::CPPCompiler => "C++ compiler:",
                        PrefKey::SolutionsDirectory => "Solutions directory:",
                    })
                    .dim(),
                    value
                );
            } else {
                // just print the value without formatting
                println!("{}", strip_ansi_codes(&value));
            }
        }
        Some(Command::Set { key }) => {
            {
                let mut lock = prefs.write()?;
                match key {
                    // prompt for corresponding value (if needed)
                    SetValues::CurrentProblem { value } => {
                        let input = if let Some(value) = value {
                            value
                        } else {
                            Input::with_theme(&ColorfulTheme::default())
                                .with_prompt("Enter a problem ID")
                                .validate_with(|input: &String| input.parse::<u64>().map(|_| ()))
                                .interact_text()?
                                .parse::<u64>()
                                .unwrap()
                        };
                        lock.current_problem = Some(input);
                    }
                    SetValues::PreferredLanguage { value } => {
                        let input = if let Some(value) = value {
                            value
                        } else {
                            let result = Select::with_theme(&ColorfulTheme::default())
                                .with_prompt("Select a language")
                                .items(&["C++", "Python"])
                                .default(lock.preferred_language as usize)
                                .interact()?;

                            // parse back into enum value
                            match result {
                                0 => Language::CPP,
                                1 => Language::Python,
                                _ => unreachable!(),
                            }
                        };

                        lock.preferred_language = input;
                    }
                    SetValues::CPPCompiler { value } => {
                        let input = if let Some(value) = value {
                            value
                        } else {
                            let result = Select::with_theme(&ColorfulTheme::default())
                                .with_prompt("Select a C++ compiler")
                                .items(&["g++", "clang"])
                                .default(lock.cpp_compiler as usize)
                                .interact()?;

                            match result {
                                0 => CPPCompiler::GCC,
                                1 => CPPCompiler::Clang,
                                _ => unreachable!(),
                            }
                        };

                        lock.cpp_compiler = input;
                    }
                    SetValues::SolutionsDirectory { value } => {
                        let input = if let Some(value) = value {
                            canonicalize(value).await?
                        } else {
                            let theme = ColorfulTheme::default();
                            let mut prompt = Input::<String>::with_theme(&theme)
                                .with_prompt("Select a solutions directory");

                            // set the default to the current dir
                            if let Some(cwd) = current_dir()
                                .ok()
                                .and_then(|s| s.into_os_string().into_string().ok())
                            {
                                prompt = prompt.default(cwd);
                            }

                            let result = canonicalize(prompt.interact_text()?).await?;

                            result
                        };

                        lock.solutions_dir = Some(input);
                    }
                }
            }
            let status = StatusSpinner::new("Saving...", &multi);
            prefs.save_prefs().await?;
            status.finish("Saved", true);
        }
        None => {
            // list all values
            let lock = prefs.read()?;
            println!("{}", style("Preferences:").green().bold().bright());
            println!(
                "{} {}",
                style("Current problem:").dim(),
                if let Some(cp) = lock.current_problem {
                    style(Cow::Owned(cp.to_string())).bright().cyan().bold()
                } else {
                    style(Cow::Borrowed("Not set"))
                        .bright()
                        .color256(215)
                        .bold()
                }
            );
            println!(
                "{} {}",
                style("Preferred language:").dim(),
                style(match lock.preferred_language {
                    Language::CPP => "C++",
                    Language::Python => "Python",
                })
                .bright()
                .yellow()
                .bold(),
            );
            println!(
                "{} {}",
                style("C++ compiler:").dim(),
                style(match lock.cpp_compiler {
                    CPPCompiler::GCC => "g++",
                    CPPCompiler::Clang => "clang",
                })
                .bright()
                .magenta()
                .bold(),
            );
            println!(
                "{} {}",
                style("Solutions directory:").dim(),
                if let Some(dir) = &lock.solutions_dir {
                    style(dir.display()).blue().bright().bold().to_string()
                } else {
                    style("Not set").red().bright().bold().to_string()
                }
            );
        }
    }
    Ok(())
}
