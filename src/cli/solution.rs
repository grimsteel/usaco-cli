use std::{borrow::Cow, process::Stdio, io::ErrorKind, path::Path};
use super::{problem::get_problem, status_spinner::StatusSpinner};
use crate::{
    http_client::{Division, HttpClient, IoMode},
    preferences::{DataStore, Language, CPPCompiler},
};
use clap::Subcommand;
use console::style;
use indicatif::MultiProgress;
use log::{info, warn, error};
use tokio::{
    io::{BufReader, AsyncBufReadExt, AsyncWriteExt},
    select,
    fs::{create_dir_all, try_exists, write, read_to_string, remove_file, metadata},
    process::Command as ProcessCommand,
};
use directories::ProjectDirs;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scaffold solutions directory
    Scaffold {
        /// Do not create a git repository
        #[arg(short, long)]
        no_git: bool,
    },
    /// Bootstrap an initial solution file
    Create {
        /// Problem ID. Will prompt if not given and if current problem is not set.
        problem_id: Option<u64>,
    },
    /// Test a solution using sample data
    Test {
        /// Problem ID. Will prompt if not given and if current problem is not set.
        problem_id: Option<u64>,
    }
}

/// check if file2 is newer than file1
pub async fn file_newer<T: AsRef<Path>>(file1: T, file2: T) -> std::io::Result<bool> {
    // get info for both files
    let file1_modified = match metadata(file1).await {
        Ok(m) => m.modified()?,
        Err(e) => {
            // if either one does not exist, return false
            return if e.kind() == ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(e)
            };
        }
    };
    let file2_modified = match metadata(file2).await {
        Ok(m) => m.modified()?,
        Err(e) => {
            return if e.kind() == ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(e)
            };
        }
    };
    Ok(file2_modified > file1_modified)
}

/// windows uses py/py3
pub fn get_python_executable() -> std::io::Result<Option<&'static str>> {
    for name in ["python3", "python2", "python", "py3", "py"] {
        match ProcessCommand::new(name)
            .arg("-V")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn() {
            Ok(_) => return Ok(Some(name)),
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    continue;
                } else {
                    return Err(e);
                }
            }
        }
    }
    Ok(None)
}

pub async fn handle(
    command: Command,
    client: HttpClient,
    store: &DataStore,
    multi: MultiProgress,
    dirs: ProjectDirs,
) -> super::Result {
    let lock = store.read()?;
    if let Some(dir) = &lock.solutions_dir {
        match command {
            Command::Scaffold { no_git } => {
                let status = StatusSpinner::new("Scaffolding solutions directory...", &multi);

                // Create the src and bin dirs
                let mut src_dir = dir.join("src");
                let bin_dir = dir.join("bin");
                create_dir_all(&src_dir).await?;
                create_dir_all(bin_dir).await?;
                // create division folders
                for division in Division::get_all() {
                    src_dir.push(division);
                    create_dir_all(&src_dir).await?;
                    src_dir.pop();
                }

                if !no_git {
                    let output = ProcessCommand::new("git").arg("init").output().await?;

                    info!("Git: {}", String::from_utf8_lossy(&output.stdout));

                    // if .gitignore doesn't exist already, create it
                    let gitignore_path = dir.join(".gitignore");
                    if !try_exists(&gitignore_path).await? {
                        write(&gitignore_path, "bin/\n").await?;
                    }
                }

                status.finish("Scaffolded successfully!", true);
            }
            Command::Create { problem_id } => {
                let lang = lock.preferred_language;
                get_problem(problem_id, &client, store, &multi, |problem| async move {
                    let filename = format!("{}.{}", problem.id, lang.to_str());
                    let mut problem_dir = dir.join("src").join(problem.division.to_str());
                    // make sure dir exists
                    create_dir_all(&problem_dir).await?;
                    problem_dir.push(filename);
                    if try_exists(&problem_dir).await? {
                        println!(
                            "{} {} {}",
                            style("Solution file").yellow(),
                            style(problem_dir.display()).magenta().bold(),
                            style("already exists; skipping").yellow()
                        );
                    } else {
                        let code = match lang {
                            Language::CPP => {
                                format!(
                                    r##"#include <bits/stdc++.h>
using namespace std;

int main() {{
  ios::sync_with_stdio(false);
  cin.tie(nullptr);
{}{}
  
  return 0;
}}"##,
                                    match problem.input {
                                        IoMode::Stdio => Cow::Borrowed(""),
                                        IoMode::File(filename) => Cow::Owned(format!(r#"  freopen("{}", "r", stdin);
"#, filename))
                                    },
                                    match problem.output {
                                        IoMode::Stdio => Cow::Borrowed(""),
                                        IoMode::File(filename) => Cow::Owned(format!(r#"  freopen("{}", "w", stdout);
"#, filename))
                                    },
                                )
                            },
                            Language::Python => {
                                format!(
                                    r#"import sys

{}{}

"#,
                                    match problem.input {
                                        IoMode::Stdio => Cow::Borrowed(""),
                                        IoMode::File(filename) => Cow::Owned(format!(r#"sys.stdin = open("{}", "r")
"#, filename))
                                    },
                                    match problem.output {
                                        IoMode::Stdio => Cow::Borrowed(""),
                                        IoMode::File(filename) => Cow::Owned(format!(r#"sys.stdout = open("{}", "w")
"#, filename))
                                    },
                                )
                            }
                        };
                        write(&problem_dir, &code).await?;
                        println!(
                            "{} {} {} {}",
                            style("Successfully bootstrapped").green(),
                            style(format!("problem {}", problem.id)).bold().cyan(),
                            style("at").green(),
                            style(problem_dir.display()).yellow().bold(),
                        );
                    }
                    Ok(())
                })
                .await?;
            },
            Command::Test { problem_id } => {
                let lang = lock.preferred_language;
                let compiler = lock.cpp_compiler;
                let multi_2 = multi.clone();
                let cache_dir = dirs.cache_dir();
                get_problem(problem_id, &client, store, &multi, |problem| async move {
                    let filename = format!("{}.{}", problem.id, lang.to_str());
                    let problem_file = dir.join("src").join(problem.division.to_str()).join(filename);
                    // problem file for python, out file for cpp
                    let mut run_file = problem_file.clone();
                    
                    if try_exists(&problem_file).await? {
                        // compile
                        if lang == Language::CPP {
                            let status = StatusSpinner::new("Compiling solution...", &multi_2);
                            
                            // make sure the output dir exists
                            let mut out_file = dir.join("bin").join(problem.division.to_str());
                            create_dir_all(&out_file).await?;
                            out_file.push(problem.id.to_string());

                            // if run file is newer than source file, no compilation needed
                            if file_newer(&problem_file, &out_file).await? {
                                status.finish("Compilation skipped", true);
                            } else {
                                // compile
                                let mut command = ProcessCommand::new(match compiler {
                                    CPPCompiler::GCC => "g++",
                                    CPPCompiler::Clang => "clang"
                                })
                                    .arg("-Wall")
                                    .arg("-g")
                                    .arg("-o")
                                    .arg(&out_file)
                                    .arg(problem_file)
                                    .stdin(Stdio::piped())
                                    .stdout(Stdio::piped())
                                    .stderr(Stdio::piped())
                                    .spawn()?;

                                let stdout = command.stdout.take().unwrap();
                                let stderr = command.stderr.take().unwrap();

                                // print output
                                tokio::spawn(async move {
                                    let mut stdout = BufReader::new(stdout).lines();
                                    let mut stderr = BufReader::new(stderr).lines();
                                    loop {
                                        select! {
                                            Ok(Some(line)) = stdout.next_line() => {
                                                info!("Comp: {}", line);
                                            },
                                            Ok(Some(line)) = stderr.next_line() => {
                                                warn!("Comp: {}", line);
                                            },
                                            else => { break; }
                                        }
                                    }
                                });

                                if command.wait().await?.success() {
                                    status.finish("Finished compiling", true);
                                } else {
                                    status.finish("Compilation failed", false);
                                    return Ok(());
                                }
                            }

                            run_file = out_file;
                        }

                        // test solution
                        let status = StatusSpinner::new("Testing solution...", &multi_2);
                        let in_file_name = if let IoMode::File(filename) = &problem.input {
                            Some(cache_dir.join(filename))
                        } else {
                            None
                        };
                        let out_file_name = if let IoMode::File(filename) = &problem.output {
                            Some(cache_dir.join(filename))
                        } else {
                            None
                        };
                        // figure out what python executable to use
                        let python_exec = if lang == Language::Python {
                            if let Some(exec) = get_python_executable()? {
                                Some(exec)
                            } else {
                                status.finish("Compilation failed", false);
                                return Ok(());
                            }
                        } else {
                            None
                        };
                        for (i, test_case) in problem.test_cases.iter().enumerate() {
                            // write input file
                            if let Some(in_file_name) = &in_file_name {
                                write(in_file_name, &test_case.input).await?;
                            }

                            let mut command = match lang {
                                Language::CPP => ProcessCommand::new(&run_file),
                                Language::Python => {
                                    let mut c = ProcessCommand::new(python_exec.unwrap());
                                    c.arg(&run_file);
                                    c
                                }
                            };

                            // spawn the process for each test case
                            let mut child = command
                                .stdin(Stdio::piped())
                                .stderr(Stdio::piped())
                                .stdout(Stdio::piped())
                                .current_dir(&cache_dir)
                                .spawn()?;

                            // write test case to stdin
                            if problem.input == IoMode::Stdio {
                                let mut stdin = child.stdin.take().unwrap();
                                stdin.write_all(&test_case.input.as_bytes()).await?;
                                stdin.flush().await?;
                            }

                            let stderr = child.stderr.take().unwrap();
                            
                            // print stderr (for debugging)
                            tokio::spawn(async move {
                                let mut stderr = BufReader::new(stderr).lines();
                                loop {
                                    select! {
                                        Ok(Some(line)) = stderr.next_line() => {
                                            warn!("Run {}: {}", i + 1, line);
                                        },
                                        else => { break; }
                                    }
                                }
                            });

                            // get output, either by reading output file or stdout
                            let out = child.wait_with_output().await?;
                            let out = if let Some(out_file_name) = &out_file_name {
                                Cow::Owned(read_to_string(&out_file_name).await?)
                            } else {
                                String::from_utf8_lossy(&out.stdout)
                            };
                            // TODO: show diffs
                            if out.trim() == test_case.output.trim() {
                                info!("Case {} passed", i + 1);
                            } else {
                                error!("Case {} failed", i + 1);
                            }
                        }

                        // clean up
                        if let Some(in_file_name) = &in_file_name {
                            remove_file(in_file_name).await?;
                        }
                        if let Some(out_file_name) = &out_file_name {
                            remove_file(out_file_name).await?;
                        }
                        
                        status.finish("Finished testing", true);
                        
                    } else {
                        error!("Solution file {} does not exist", &problem_file.display());
                    }

                    Ok(())
                }).await?;
            }
        }
    } else {
        // prompt user to setup solutions dir
        println!(
            "{}",
            style("The solutions directory is not set!").bold().red()
        );
        println!(
            "Run {} to configure it.",
            style("`usaco preferences set solutions-directory`")
                .yellow()
                .italic()
        );
    }

    Ok(())
}
