use anyhow::{bail, Result};
use sag_protocol::{Command, Operation, Output, Response};
use std::{path::PathBuf, time::Duration};

pub fn socket() -> PathBuf {
    std::env::var_os("SAG_FNOS_CLI_SOCKET").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/apps/sag-desktop-fnos/target/cli.sock"))
}
pub async fn connect() -> Result<sag_client::Client> {
    sag_client::Client::connect_unix(&socket()).await
}
pub fn service(action: &super::ServiceCommand) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let client = connect().await?;
        let timeout = Duration::from_secs(30);
        if matches!(action, super::ServiceCommand::Status) {
            let status = client.execute(std::env::current_dir()?, Command::Status, timeout).await?;
            if matches!(status, Response::Status(status) if status.connected) {
                println!("{}", serde_json::to_string_pretty(client.info())?);
            } else { println!("SAG 后台服务未运行"); }
            return Ok(());
        }
        client.call(Operation::Shutdown, timeout).await?;
        if matches!(action, super::ServiceCommand::Stop) {
            println!("SAG 后台服务已停止");
            return Ok(());
        }
        client.execute(std::env::current_dir()?, Command::List, timeout).await?;
        println!("{}", serde_json::to_string_pretty(client.info())?);
        if matches!(action, super::ServiceCommand::Serve) {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let sender = std::sync::Mutex::new(Some(sender));
            ctrlc::set_handler(move || { if let Some(sender) = sender.lock().unwrap().take() { let _ = sender.send(()); } })?;
            receiver.await?;
            if !matches!(client.call(Operation::Shutdown, timeout).await?, Output::Unit) { bail!("停止服务响应无效"); }
        }
        Ok(())
    })
}
