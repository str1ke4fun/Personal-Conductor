use anyhow::Result;
use tokio::process::Command;

pub fn build_command(command: &str, working_dir: Option<&str>) -> Result<Command> {
    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/C", command]);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}
