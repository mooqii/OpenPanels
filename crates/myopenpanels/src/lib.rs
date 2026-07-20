pub mod agent;
pub mod agent_control;
pub mod bridge;
pub mod canvas;
pub mod cli;
pub mod content;
pub mod context_cleanup;
pub mod control;
pub mod error;
pub mod ids;
pub mod model_gateway;
pub mod operations;
pub mod panel;
pub mod paths;
pub mod publishing;
pub mod selection;
pub mod server;
pub mod storage;
pub mod studio;
pub mod tasks;
pub mod trace;
pub mod types;
pub mod typesetting;
pub mod update;
pub mod wiki;
pub mod writing;

pub use cli::run_cli;

#[cfg(test)]
pub(crate) static TASK_BROKER_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
