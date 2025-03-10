use anyhow::{Result, anyhow};
use log::info;
use rand::{Rng as _, distr::Alphanumeric};
use tokio::process;

pub async fn cp_from_container(
    container_manager: impl AsRef<str>,
    container_id: impl AsRef<str>,
    from: impl AsRef<str>,
    to: impl AsRef<str>,
) -> Result<()> {
    info!("Moving from container manager");
    if !process::Command::new(container_manager.as_ref())
        .args([
            "cp",
            "--overwrite",
            format!("{}:{}", container_id.as_ref(), from.as_ref()).as_ref(),
            to.as_ref(),
        ])
        .status()
        .await?
        .success()
    {
        return Err(anyhow!("Failed to copy from local server container"));
    }
    Ok(())
}

pub fn gen_key() -> String {
    #[cfg(debug_assertions)]
    const KEY_LEN: usize = 1;
    #[cfg(not(debug_assertions))]
    const KEY_LEN: usize = 16;
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(KEY_LEN)
        .map(char::from)
        .collect()
}
