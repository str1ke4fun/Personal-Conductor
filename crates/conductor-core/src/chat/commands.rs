use crate::{
    chat_parser::{parse, resolve_index, ChatIntent, IndexRef, QueryKind, TaskAction, TaskFilter},
    config::CoreConfig,
    tasklist,
    tasks::{self, Task, TaskStatus},
};

pub(super) async fn rule_based_answer(prompt: &str, tasks: &[Task], config: &CoreConfig) -> String {
    let intent = parse(prompt);

    match intent {
        ChatIntent::Query { kind } => handle_query(kind, tasks, config),
        ChatIntent::ListTasks { filter } => handle_list_tasks(filter, tasks, config).await,
        ChatIntent::UpdateTask {
            index,
            task_id,
            action,
        } => handle_update_task(index, task_id, action, tasks, config),
        ChatIntent::Unknown { .. } => fallback_unknown_answer(tasks),
    }
}

fn handle_query(kind: QueryKind, tasks: &[Task], config: &CoreConfig) -> String {
    match kind {
        QueryKind::WhatToFocus => next_task_answer(tasks, config),
        QueryKind::Status => status_answer(tasks),
        QueryKind::Help => help_answer(),
    }
}

async fn handle_list_tasks(filter: TaskFilter, tasks: &[Task], config: &CoreConfig) -> String {
    // For ByTime filter, use the tasklist budget query for richer results
    if let TaskFilter::ByTime { minutes } = filter {
        return handle_budget_tasks(minutes, config).await;
    }

    let pending: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress)
        .collect();

    if pending.is_empty() {
        return "没有待处理的任务了".to_string();
    }

    let filtered: Vec<&Task> = match filter {
        TaskFilter::All => pending.clone(),
        TaskFilter::Pending => pending.clone(),
        TaskFilter::ByTime { .. } => unreachable!(),
    };

    if filtered.is_empty() {
        return "没有待处理的任务了".to_string();
    }

    let limit = 3;
    let display: Vec<&Task> = filtered.iter().take(limit).cloned().collect();

    let mut answer = format!("推荐以下任务（{} 个）：\n\n", display.len());
    for (i, task) in display.iter().enumerate() {
        answer.push_str(&format_task_line(i + 1, task, config));
        answer.push('\n');
    }

    if filtered.len() > limit {
        answer.push_str(&format!("... 还有 {} 个任务", filtered.len() - limit));
    }

    answer
}

async fn handle_budget_tasks(minutes: u32, _config: &CoreConfig) -> String {
    let agent_tasks =
        match tasklist::list_tasks_by_budget(minutes, tasklist::TaskListFilter::default()).await {
            Ok(tasks) => tasks,
            Err(err) => return format!("查询任务时出错：{err}"),
        };

    if agent_tasks.is_empty() {
        return format!("没有在 {minutes} 分钟内能完成的任务。");
    }

    let limit = 5;
    let display = agent_tasks.iter().take(limit);

    let mut answer = format!(
        "你有 {minutes} 分钟，以下任务可以完成（共 {} 个）：\n\n",
        agent_tasks.len()
    );
    for (i, task) in display.enumerate() {
        let est = task
            .est_minutes
            .map(|m| format!("{m} 分钟"))
            .unwrap_or_else(|| "未估算".to_string());
        answer.push_str(&format!(
            "{}. {} ({})\n   预计：{}\n",
            i + 1,
            task.subject,
            task.kind,
            est
        ));
    }

    if agent_tasks.len() > limit {
        answer.push_str(&format!("\n... 还有 {} 个任务", agent_tasks.len() - limit));
    }

    answer
}

fn handle_update_task(
    index: Option<IndexRef>,
    task_id: Option<String>,
    action: TaskAction,
    tasks: &[Task],
    config: &CoreConfig,
) -> String {
    let target = find_target_task(index, task_id, tasks);

    match (target, action) {
        (None, _) => "找不到指定的任务。输入 \"列出任务\" 查看当前任务列表。".to_string(),
        (Some(idx), TaskAction::Pass) => {
            let task = &tasks[idx];
            format!(
                "✅ 已通过任务：{} ({})\n\
                 文件：{}",
                task.id,
                task.kind,
                task.artifact_label()
            )
        }
        (Some(idx), TaskAction::Skip) => {
            let task = &tasks[idx];
            format!("⏭️  已跳过任务：{} ({})", task.id, task.kind)
        }
        (Some(idx), TaskAction::Reject) => {
            let task = &tasks[idx];
            format!(
                "❌ 已拒绝任务：{} ({})\n\
                 文件：{}",
                task.id,
                task.kind,
                task.artifact_label()
            )
        }
        (Some(idx), TaskAction::Snooze { minutes }) => {
            let task = &tasks[idx];
            format!(
                "⏰ 已推迟任务：{} ({})\n\
                 推迟 {minutes} 分钟",
                task.id, task.kind
            )
        }
        (Some(idx), TaskAction::Start) => {
            let task = &tasks[idx];
            format!(
                "🚀 开始任务：{} ({})\n\
                 文件：{}\n\
                 预计：{} 分钟\n\
                 重点：{}",
                task.id,
                task.kind,
                task.artifact_label(),
                task.est_minutes.unwrap_or(config.focus_window_minutes),
                task.focus_hint.as_deref().unwrap_or("专注处理")
            )
        }
    }
}

fn find_target_task(
    index: Option<IndexRef>,
    task_id: Option<String>,
    tasks: &[Task],
) -> Option<usize> {
    if let Some(id) = task_id {
        return tasks.iter().position(|t| t.id == id);
    }

    if let Some(idx_ref) = index {
        let pending: Vec<usize> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress)
            .map(|(i, _)| i)
            .collect();

        let resolved = resolve_index(&idx_ref, pending.len())?;
        return Some(pending[resolved]);
    }

    None
}

fn format_task_line(index: usize, task: &Task, config: &CoreConfig) -> String {
    let artifact = task.artifact_label();
    let minutes = task
        .est_minutes
        .map(|m| format!("{m} 分钟"))
        .unwrap_or_else(|| format!("{} 分钟", config.focus_window_minutes));
    let hint = task.focus_hint.as_deref().unwrap_or("专注处理");

    format!(
        "{}. {} ({})\n   文件：{}\n   预计：{}\n   重点：{}",
        index, task.id, task.kind, artifact, minutes, hint
    )
}

fn status_answer(tasks: &[Task]) -> String {
    let stats = tasks::activity_stats(tasks);
    let pending = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending)
        .count();
    let in_progress = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::InProgress)
        .count();
    let passed = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Passed)
        .count();
    let rejected = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Rejected)
        .count();
    let skipped = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Skipped)
        .count();

    let total = tasks.len();

    format!(
        "📊 任务状态概览（共 {} 个任务）：\n\n\
         运行中 Claude/Codex 会话：{}\n\
         待审 Hook 输出：{}\n\
         其他待办：{}\n\
         🟡 进行中：{in_progress}\n\
         ⏳ 待处理：{pending}\n\
         ✅ 已通过：{passed}\n\
         ❌ 已拒绝：{rejected}\n\
         ⏭️  已跳过：{skipped}",
        total, stats.active_hook_sessions, stats.pending_hook_reviews, stats.pending_other
    )
}

fn help_answer() -> String {
    "清和 · 任务助手\n\n\
     可用命令：\n\n\
     📋 查看任务\n\
     • \"现在该看什么\" - 推荐下一个任务\n\
     • \"列出待办\" - 列出所有待处理任务\n\
     • \"我有 X 分钟\" - 筛选 X 分钟内能完成的任务\n\
     • \"现在状态\" - 查看任务统计\n\n\
     ✅ 处理任务\n\
     • \"第一项过了\" - 通过第一个任务\n\
     • \"跳过第二个\" - 跳过第二个任务\n\
     • \"这个不要了\" - 拒绝当前任务\n\
     • \"PPT 那个推后\" - 推迟包含 PPT 的任务\n\
     • \"第一个开始\" - 开始第一个任务\n\n\
     💡 索引说明\n\
     • \"第一项\" / \"1\" / \"first\" → 第 1 个\n\
     • \"第二个\" / \"2\" / \"second\" → 第 2 个\n\
     • \"最后那个\" / \"last\" → 最后一个"
        .to_string()
}

fn next_task_answer(tasks: &[Task], config: &CoreConfig) -> String {
    let candidate = tasks
        .iter()
        .find(|task| task.status == TaskStatus::InProgress)
        .or_else(|| tasks.iter().find(|task| task.status == TaskStatus::Pending));

    let Some(task) = candidate else {
        return "目前没有进行中或待处理的任务。".to_string();
    };

    let artifact = task.artifact_label();
    let minutes = task
        .est_minutes
        .map(|m| format!("{m} 分钟"))
        .unwrap_or_else(|| format!("{} 分钟", config.focus_window_minutes));
    let hint = task
        .focus_hint
        .as_deref()
        .unwrap_or("打开文件查看详情后再决定是否通过");

    format!(
        "推荐下一个任务：{} ({})\n\
         文件：{}\n\
         预计：{}\n\
         重点：{}",
        task.id, task.kind, artifact, minutes, hint
    )
}

pub(super) fn fallback_unknown_answer(tasks: &[Task]) -> String {
    let stats = tasks::activity_stats(tasks);
    format!(
        "我现在可以先帮你整理任务。当前有 {} 个运行中 Claude/Codex 会话，{} 个待审 Hook 输出，{} 个其他待办。\n\n\
         可以试试：\n\
         • \"现在该看什么\" - 推荐下一个任务\n\
         • \"我有 20 分钟\" - 按时间筛选\n\
         • \"第一项过了\" - 通过任务\n\
         • \"帮助\" - 查看所有命令\n\n\
         也可以在设置里配置语言模型密钥，让我回答更开放的问题。",
        stats.active_hook_sessions, stats.pending_hook_reviews, stats.pending_other
    )
}
