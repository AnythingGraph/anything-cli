use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use plan_ir::QueryRequest;
use runtime::{resolve_workspace_root, ReasoningRuntime, RuntimeConfig};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "ag", about = "AnythingGraph thin reasoning layer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate playbooks in a directory
    Validate {
        #[arg(long, default_value = "playbooks")]
        playbooks: PathBuf,
    },
    /// Compile and optionally execute a structured query test
    Test {
        #[arg(long, default_value = "playbooks")]
        playbooks: PathBuf,
        #[arg(long, default_value = "bindings")]
        bindings: PathBuf,
        #[arg(long, default_value = "profiles/local.yaml")]
        profile: PathBuf,
        #[arg(long, default_value = "simple-crm-access")]
        playbook_id: String,
        #[arg(long, default_value = "Alex Anderson")]
        by_name: String,
        #[arg(long, default_value = "owns_account")]
        relationship: String,
        #[arg(long)]
        execute: bool,
    },
    /// Start the Rust reasoning HTTP service
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "8787")]
        port: u16,
        #[arg(long, default_value = "playbooks")]
        playbooks: PathBuf,
        #[arg(long, default_value = "bindings")]
        bindings: PathBuf,
        #[arg(long, default_value = "profiles/local.yaml")]
        profile: PathBuf,
    },
    /// Print MCP HTTP config for Cursor / Claude
    McpConfig {
        #[arg(long, default_value = "http://127.0.0.1:3334/mcp")]
        mcp_url: String,
        #[arg(long, default_value = "http://127.0.0.1:8787")]
        reasoning_url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cli = Cli::parse();
    let workspace_root = resolve_workspace_root();

    match cli.command {
        Commands::Validate { playbooks } => {
            let config = RuntimeConfig {
                playbooks_dir: workspace_root.join(playbooks),
                bindings_dir: workspace_root.join("bindings"),
                profile_path: None,
            };
            let runtime = ReasoningRuntime::bootstrap(config).await?;
            runtime.validate_all_playbooks().await?;
            let ids = runtime.list_playbook_ids().await;
            println!("Validated {} playbook(s): {}", ids.len(), ids.join(", "));
        }
        Commands::Test {
            playbooks,
            bindings,
            profile,
            playbook_id,
            by_name,
            relationship,
            execute,
        } => {
            let config = RuntimeConfig {
                playbooks_dir: workspace_root.join(playbooks),
                bindings_dir: workspace_root.join(bindings),
                profile_path: Some(workspace_root.join(profile)),
            };
            let runtime = ReasoningRuntime::bootstrap(config).await?;
            let request = QueryRequest {
                playbook_id,
                subject_id: None,
                binding_name: Some("postgres".into()),
                resolve: plan_ir::ResolveEntityRequest {
                    entity: "crm_user".into(),
                    by_name: Some(by_name),
                    by_identifier: None,
                },
                count: Some(plan_ir::CountRelationshipRequest {
                    relationship,
                    object_entity: Some("crm_account".into()),
                }),
                list: None,
            };
            let plan = runtime.compile_plan(&request).await?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
            if execute {
                let proof = runtime.execute_plan(&plan).await?;
                println!("{}", serde_json::to_string_pretty(&proof)?);
            }
        }
        Commands::Serve {
            host,
            port,
            playbooks,
            bindings,
            profile,
        } => {
            let mut command = Command::new("cargo");
            command
                .arg("run")
                .arg("-p")
                .arg("reasoning-service")
                .arg("--release")
                .env("AG_REASONING_HOST", host)
                .env("AG_REASONING_PORT", port.to_string())
                .env("AG_WORKSPACE_ROOT", &workspace_root)
                .env("AG_PLAYBOOKS_DIR", workspace_root.join(playbooks))
                .env("AG_BINDINGS_DIR", workspace_root.join(bindings))
                .env("AG_PROFILE_PATH", workspace_root.join(profile))
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let status = command.status().context("failed to start reasoning-service")?;
            if !status.success() {
                anyhow::bail!("reasoning-service exited with {status}");
            }
        }
        Commands::McpConfig {
            mcp_url,
            reasoning_url,
        } => {
            let snippet = serde_json::json!({
                "mcpServers": {
                    "anythinggraph-thin": {
                        "url": mcp_url,
                        "env": {
                            "AG_REASONING_URL": reasoning_url
                        }
                    }
                }
            });
            println!("{}", serde_json::to_string_pretty(&snippet)?);
        }
    }

    Ok(())
}
