use clap::Subcommand;
use console::style;
use indicatif::MultiProgress;
use tokio::{process::Command as ProcessCommand, fs::{create_dir_all, try_exists, write}};
use log::{info, warn};
use super::{status_spinner::StatusSpinner, problem::get_problem};
use crate::{http_client::{HttpClient, Division}, preferences::DataStore};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scaffold solutions directory
    Scaffold {
        /// Do not create a git repository
        #[arg(short, long)]
        no_git: bool
    },
    /// Bootstrap an initial solution file
    Create {
        /// Problem ID. Will prompt 
        problem_id: Option<u64>
    }
}

pub async fn handle(command: Command, client: HttpClient, store: &DataStore, multi: MultiProgress) -> super::Result {
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
                    let output = ProcessCommand::new("git")
                        .arg("init")
                        .output()
                        .await?;

                    info!("Git: {}", String::from_utf8_lossy(&output.stdout));

                    // if .gitignore doesn't exist already, create it
                    let gitignore_path = dir.join(".gitignore");
                    if !try_exists(&gitignore_path).await? {
                        write(&gitignore_path, "bin/\n").await?;
                    }
                }

                status.finish("Scaffolded successfully!", true);

            },
            Command::Create { problem_id } => {
                let lang_str = lock.preferred_language.to_str();
                get_problem(problem_id, &client, store, &multi, |problem| async move {
                    let filename = format!("{}.{}", problem.id, lang_str);
                    let mut problem_dir = dir.join(problem.division.to_str());
                    // make sure dir exists
                    create_dir_all(&problem_dir).await?;
                    problem_dir.push(filename);
                    if try_exists(&problem_dir).await? {
                        warn!("Solution file {} already exists; skipping", problem_dir.display());
                    } else {
                        
                    }
                    Ok(())
                }).await?;
            }
        }
    } else {
        // prompt user to setup solutions dir
        println!("{}", style("The solutions directory is not set!").bold().red());
        println!(
            "Run {} to configure it.",
            style("`usaco preferences set solutions-directory`").yellow().italic()
        );
    }

    Ok(())
}
