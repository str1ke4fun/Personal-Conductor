use anyhow::Result;
use tokio::process::Command;

pub fn build_command(command: &str, working_dir: Option<&str>) -> Result<Command> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", command]);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}
