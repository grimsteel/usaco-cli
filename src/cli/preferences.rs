use clap::{Subcommand, ValueEnum};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use indicatif::MultiProgress;
use console::{user_attended, style, strip_ansi_codes};
use crate::{preferences::{PreferencesStore, Language, CPPCompiler}, cli::status_spinner::StatusSpinner};
use std::{error::Error, borrow::Cow};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Read a preference key
    Get {
        /// Preference key to retrieve
        #[arg(value_enum)]
        key: PrefKey
    },
    /// Set a preference key. Will prompt for value
    Set {
        /// Preference key to set
        #[arg(value_enum)]
        key: PrefKey
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum PrefKey {
    /// Default problem ID for problem and solution commands
    CurrentProblem,
    /// Preferred language for boilerplate code
    PreferredLanguage,
    /// Preferred C++ compiler
    CPPCompiler
}



pub async fn handle(command: Option<Command>, prefs: &PreferencesStore, multi: MultiProgress) -> Result<(), Box<dyn Error>> {
    match command {
        Some(Command::Get { key }) => {
            let lock = prefs.read()?;
            let value = match key {
                PrefKey::CurrentProblem => lock.current_problem
                    .map(|s| style(Cow::Owned(s.to_string())).cyan())
                    // orange if no problem set
                    .unwrap_or_else(|| style(Cow::Borrowed("Not Set")).color256(215)),
                PrefKey::PreferredLanguage => match lock.preferred_language {
                    Language::CPP => style(Cow::Borrowed("C++")).blue(),
                    Language::Python => style(Cow::Borrowed("Python")).yellow()
                },
                PrefKey::CPPCompiler => style(match lock.cpp_compiler {
                    CPPCompiler::GCC => Cow::Borrowed("g++"),
                    CPPCompiler::Clang => Cow::Borrowed("clang")
                }).magenta()
            }.bright().bold().to_string();
            if user_attended() {
                println!("{} {}", style(match key {
                    PrefKey::CurrentProblem => "Current problem:",
                    PrefKey::PreferredLanguage => "Preferred language:",
                    PrefKey::CPPCompiler => "C++ compiler:"
                }).dim(), value);
            } else {
                // just print the value without formatting
                println!("{}", strip_ansi_codes(&value));
            }
        },
        Some(Command::Set { key }) =>{
            {
                let mut lock = prefs.write()?;
                match key {
                    // prompt for corresponding value
                    PrefKey::CurrentProblem => {
                        let input = Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter a problem ID")
                            .validate_with(|input: &String| {
                                input.parse::<u64>().map(|_| ())
                            })
                            .interact_text()?
                            .parse::<u64>().unwrap();

                        lock.current_problem = Some(input);
                    },
                    PrefKey::PreferredLanguage => {
                        let input = Select::with_theme(&ColorfulTheme::default())
                            .with_prompt("Select a language")
                            .items(&["C++", "Python"])
                            .default(lock.preferred_language as usize)
                            .interact()?;

                        lock.preferred_language = match input {
                            0 => Language::CPP,
                            1 => Language::Python,
                            _ => unreachable!()
                        }
                    },
                    PrefKey::CPPCompiler => {
                        let input = Select::with_theme(&ColorfulTheme::default())
                            .with_prompt("Select a C++ compiler")
                            .items(&["g++", "clang"])
                            .default(lock.cpp_compiler as usize)
                            .interact()?;

                        lock.cpp_compiler = match input {
                            0 => CPPCompiler::GCC,
                            1 => CPPCompiler::Clang,
                            _ => unreachable!()
                        }
                    }
                }
            }
            let status = StatusSpinner::new("Saving...", &multi);
            prefs.save().await?;
            status.finish("Saved", true);
        },
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
                    style(Cow::Borrowed("Not set")).bright().color256(215).bold()
                }
            );
            println!(
                "{} {}",
                style("Preferred language:").dim(),
                style(match lock.preferred_language {
                    Language::CPP => "C++",
                    Language::Python => "Python"
                }).bright().yellow().bold(),
            );
            println!(
                "{} {}",
                style("C++ compiler:").dim(),
                style(match lock.cpp_compiler {
                    CPPCompiler::GCC => "g++",
                    CPPCompiler::Clang => "clang"
                }).bright().magenta().bold(),
            );
        }
    }
    Ok(())
}
