pub fn is_interesting(process_name: &str, _title: &str) -> bool {
    const INTERESTING_PROCESSES: &[&str] = &[
        "Code.exe",
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "WindowsTerminal.exe",
        "pwsh.exe",
        "wechat.exe",
        "Lark.exe",
        "Feishu.exe",
        "trae.exe",
    ];
    INTERESTING_PROCESSES
        .iter()
        .any(|process| process_name.eq_ignore_ascii_case(process))
}
