use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示清和", true, None::<&str>)?;
    let panel = MenuItem::with_id(
        app,
        "panel",
        "打开任务面板",
        true,
        Some("CmdOrCtrl+Shift+L"),
    )?;
    let chat = MenuItem::with_id(app, "chat", "对话", true, Some("CmdOrCtrl+Shift+C"))?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quiet = MenuItem::with_id(app, "quiet", "专注模式 30 分钟", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &panel, &chat, &settings, &quiet, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("pet") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "panel" => {
                if let Some(win) = app.get_webview_window("pet") {
                    let _ = win.show();
                    let _ = win.set_focus();
                    let _ = app.emit("navigate_to", "tasks");
                }
            }
            "chat" => {
                if let Some(win) = app.get_webview_window("pet") {
                    let _ = win.show();
                    let _ = win.set_focus();
                    let _ = app.emit("navigate_to", "chat");
                }
            }
            "settings" => {
                if let Some(win) = app.get_webview_window("pet") {
                    let _ = win.show();
                    let _ = win.set_focus();
                    let _ = app.emit("navigate_to", "settings");
                }
            }
            "quiet" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::quiet_for_minutes(app, 30).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
