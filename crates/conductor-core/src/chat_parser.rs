#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatIntent {
    ListTasks {
        filter: TaskFilter,
    },
    UpdateTask {
        index: Option<IndexRef>,
        task_id: Option<String>,
        action: TaskAction,
    },
    Query {
        kind: QueryKind,
    },
    Unknown {
        original: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskFilter {
    All,
    Pending,
    ByTime { minutes: u32 },
}

impl TaskFilter {
    pub fn get_minutes(&self) -> u32 {
        match self {
            TaskFilter::All => u32::MAX,
            TaskFilter::Pending => u32::MAX,
            TaskFilter::ByTime { minutes } => *minutes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexRef {
    Position(usize),
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskAction {
    Pass,
    Skip,
    Reject,
    Snooze { minutes: u32 },
    Start,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryKind {
    WhatToFocus,
    Status,
    Help,
}

pub fn parse(input: &str) -> ChatIntent {
    let input = input.trim();
    if input.is_empty() {
        return ChatIntent::Unknown {
            original: input.to_string(),
        };
    }

    let lowered = input.to_lowercase();
    let chinese_lower = input.chars().collect::<String>();

    if is_query_what_to_focus(&lowered, &chinese_lower) {
        return ChatIntent::Query {
            kind: QueryKind::WhatToFocus,
        };
    }

    if is_query_status(&lowered, &chinese_lower) {
        return ChatIntent::Query {
            kind: QueryKind::Status,
        };
    }

    if is_help(&lowered, &chinese_lower) {
        return ChatIntent::Query {
            kind: QueryKind::Help,
        };
    }

    if is_list_pending(&lowered, &chinese_lower) {
        return ChatIntent::ListTasks {
            filter: TaskFilter::Pending,
        };
    }

    if is_list_all(&lowered, &chinese_lower) {
        return ChatIntent::ListTasks {
            filter: TaskFilter::All,
        };
    }

    if let Some((index, action)) = parse_task_action(input, &lowered) {
        return ChatIntent::UpdateTask {
            index: Some(index),
            task_id: None,
            action,
        };
    }

    if let Some(filter) = parse_time_filter(input, &lowered) {
        return ChatIntent::ListTasks {
            filter: TaskFilter::ByTime { minutes: filter },
        };
    }

    if let Some(action) = parse_global_action(&lowered) {
        return ChatIntent::UpdateTask {
            index: None,
            task_id: None,
            action,
        };
    }

    ChatIntent::Unknown {
        original: input.to_string(),
    }
}

fn is_query_what_to_focus(lowered: &str, chinese: &str) -> bool {
    lowered.contains("what should")
        || lowered.contains("look at")
        || lowered.contains("focus")
        || lowered.contains("what to do")
        || lowered.contains("next task")
        || chinese.contains("现在该看什么")
        || chinese.contains("现在看什么")
        || chinese.contains("有什么要")
        || chinese.contains("看哪个")
        || chinese.contains("下一个")
        || chinese.contains("推荐")
}

fn is_query_status(lowered: &str, chinese: &str) -> bool {
    lowered.contains("status")
        || lowered.contains("how many")
        || lowered.contains("count")
        || chinese.contains("状态")
        || chinese.contains("多少")
        || chinese.contains("有几")
        || chinese.contains("多少个")
}

fn is_help(lowered: &str, chinese: &str) -> bool {
    lowered.contains("help")
        || lowered.contains("commands")
        || lowered.contains("usage")
        || lowered.contains("?")
        || chinese.contains("帮助")
        || chinese.contains("怎么用")
        || chinese.contains("命令")
}

fn parse_time_filter(input: &str, lowered: &str) -> Option<u32> {
    if lowered.ends_with("分钟") || lowered.ends_with("min") || lowered.ends_with("m") {
        let num_str = extract_number(input)?;
        if lowered.ends_with("min") {
            return parse_minutes_number(&num_str);
        }
        if lowered.ends_with("m") && !lowered.ends_with("min") {
            if lowered
                .trim_end_matches(|c: char| c.is_numeric() || c == ' ' || c == 'm')
                .is_empty()
            {
                return parse_minutes_number(&num_str);
            }
        }
        if lowered.ends_with("分钟") {
            return parse_chinese_number(&num_str);
        }
    }

    if lowered.contains("半小时") || lowered.contains("半小時") {
        return Some(30);
    }

    if lowered.contains("小时") || lowered.contains("小時") {
        let num_str = extract_number_before(input, &["小时", "小時"])?;
        let hours = parse_chinese_number(&num_str).unwrap_or(1);
        return Some(hours * 60);
    }

    // "N分钟内能做什么" / "N分钟内可以做什么" — "分钟" followed by context words
    if let Some(pos) = lowered.find("分钟") {
        let before = &input[..pos];
        if let Some(num_str) = extract_number(before).or_else(|| {
            parse_chinese_single_number(
                before
                    .chars()
                    .rev()
                    .take(2)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
                    .as_str(),
            )
            .map(|n| n.to_string())
        }) {
            return parse_chinese_number(&num_str);
        }
    }

    // "N minutes" / "for N minutes" — English full word
    if let Some(pos) = lowered.find(" minutes") {
        let before = &input[..pos];
        if let Some(num_str) = extract_number(before) {
            return parse_minutes_number(&num_str);
        }
    }

    // "半小时能做什么" — "半小时" with trailing context
    // Already handled above by the `contains("半小时")` check

    // "N小时能做什么" — "小时" with trailing context words
    if let Some(pos) = lowered.find("小时") {
        let before = &input[..pos];
        if let Some(num_str) = extract_number(before).or_else(|| {
            parse_chinese_single_number(
                before
                    .chars()
                    .rev()
                    .take(2)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
                    .as_str(),
            )
            .map(|n| n.to_string())
        }) {
            let hours = parse_chinese_number(&num_str).unwrap_or(1);
            return Some(hours * 60);
        }
    }

    None
}

/// Parse time-budget intent from user messages. Returns the budget in minutes.
/// Matches patterns like:
/// - "我有N分钟" / "我有N个小时"
/// - "N分钟内能做什么" / "半小时能做什么"
/// - "give me tasks for N minutes" / "what can I do in N min"
pub fn parse_time_budget(input: &str) -> Option<u32> {
    let lowered = input.to_lowercase();
    parse_time_filter(input, &lowered)
}

fn extract_number(input: &str) -> Option<String> {
    let mut num_chars = String::new();
    for c in input.chars() {
        if c.is_ascii_digit() {
            num_chars.push(c);
        }
    }
    if num_chars.is_empty() {
        None
    } else {
        Some(num_chars)
    }
}

fn extract_number_before(input: &str, suffixes: &[&str]) -> Option<String> {
    for suffix in suffixes {
        if let Some(pos) = input.find(suffix) {
            let before = &input[..pos];
            return extract_number(before).or_else(|| {
                parse_chinese_single_number(
                    before
                        .chars()
                        .rev()
                        .take(2)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                        .as_str(),
                )
                .map(|n| n.to_string())
            });
        }
    }
    None
}

fn parse_minutes_number(s: &str) -> Option<u32> {
    s.parse().ok()
}

fn parse_chinese_number(s: &str) -> Option<u32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Some(1);
    }

    let chars: Vec<char> = trimmed.chars().collect();

    let mut result: u32 = 0;
    let mut has_value = false;

    for c in chars {
        match c {
            '0'..='9' => {
                has_value = true;
                result = result * 10 + (c as u32 - '0' as u32);
            }
            _ => {}
        }
    }

    if has_value {
        Some(result)
    } else {
        None
    }
}

fn parse_chinese_single_number(s: &str) -> Option<u32> {
    match s {
        "一" | "1" => Some(1),
        "二" | "两" | "2" => Some(2),
        "三" | "3" => Some(3),
        "四" | "4" => Some(4),
        "五" | "5" => Some(5),
        "六" | "6" => Some(6),
        "七" | "7" => Some(7),
        "八" | "8" => Some(8),
        "九" | "9" => Some(9),
        _ => None,
    }
}

fn is_list_pending(lowered: &str, chinese: &str) -> bool {
    lowered.contains("pending")
        || lowered.contains("todo")
        || lowered.contains("list")
        || chinese.contains("待办")
        || chinese.contains("列表")
        || chinese.contains("有哪些")
}

fn is_list_all(lowered: &str, chinese: &str) -> bool {
    lowered.contains("all tasks")
        || lowered.contains("list all")
        || chinese.contains("全部任务")
        || chinese.contains("所有任务")
}

fn parse_task_action(input: &str, lowered: &str) -> Option<(IndexRef, TaskAction)> {
    let index = parse_index(input, lowered).unwrap_or(IndexRef::Position(0));

    if lowered.contains("过了") || lowered.contains("通过") || lowered.contains("pass") {
        return Some((index, TaskAction::Pass));
    }

    if lowered.contains("跳过") || lowered.contains("skip") {
        return Some((index, TaskAction::Skip));
    }

    if lowered.contains("不要了") || lowered.contains("拒绝") || lowered.contains("reject") {
        return Some((index, TaskAction::Reject));
    }

    if lowered.contains("推后") || lowered.contains("推迟") || lowered.contains("延期") {
        let minutes = parse_snooze_time(input, lowered).unwrap_or(60);
        return Some((index, TaskAction::Snooze { minutes }));
    }

    if lowered.contains("开始") || lowered.contains("做") || lowered.contains("start") {
        return Some((index, TaskAction::Start));
    }

    None
}

fn parse_index(input: &str, lowered: &str) -> Option<IndexRef> {
    if lowered.contains("第一") || lowered.contains("1") || lowered.contains("first") {
        return Some(IndexRef::Position(0));
    }

    if lowered.contains("第二") || lowered.contains("2") || lowered.contains("second") {
        return Some(IndexRef::Position(1));
    }

    if lowered.contains("第三") || lowered.contains("3") || lowered.contains("third") {
        return Some(IndexRef::Position(2));
    }

    if lowered.contains("第四") || lowered.contains("4") || lowered.contains("fourth") {
        return Some(IndexRef::Position(3));
    }

    if lowered.contains("最后") || lowered.contains("last") {
        return Some(IndexRef::Last);
    }

    for (i, c) in input.chars().enumerate() {
        if c.is_ascii_digit() {
            if let Ok(num) = c.to_string().parse::<usize>() {
                if num > 0 {
                    return Some(IndexRef::Position(num - 1));
                }
            }
            let num_str: String = input
                .chars()
                .skip(i)
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(num) = num_str.parse::<usize>() {
                if num > 0 {
                    return Some(IndexRef::Position(num - 1));
                }
            }
            break;
        }
    }

    None
}

fn parse_snooze_time(input: &str, lowered: &str) -> Option<u32> {
    parse_time_filter(input, lowered)
}

fn parse_global_action(lowered: &str) -> Option<TaskAction> {
    if lowered.contains("pass all") || lowered.contains("all passed") {
        return Some(TaskAction::Pass);
    }
    None
}

pub fn resolve_index(index_ref: &IndexRef, total: usize) -> Option<usize> {
    match index_ref {
        IndexRef::Position(pos) => {
            if *pos < total {
                Some(*pos)
            } else {
                None
            }
        }
        IndexRef::Last => {
            if total > 0 {
                Some(total - 1)
            } else {
                None
            }
        }
    }
}

pub fn match_task_by_kind<'a>(tasks: &'a [&'a str], kind_hint: &str) -> Option<usize> {
    let hint_lower = kind_hint.to_lowercase();

    let keywords = [
        "ppt", "文档", "doc", "pdf", "代码", "code", "review", "分析", "api",
    ];

    for keyword in &keywords {
        if hint_lower.contains(keyword) {
            for (i, task_kind) in tasks.iter().enumerate() {
                if task_kind.to_lowercase().contains(keyword) {
                    return Some(i);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_what_to_focus() {
        let inputs = vec![
            "现在该看什么",
            "现在看什么",
            "What should I focus on?",
            "有什么要看的吗",
            "推荐下一个",
            "下一个任务",
        ];
        for input in inputs {
            let intent = parse(input);
            assert!(
                matches!(
                    intent,
                    ChatIntent::Query {
                        kind: QueryKind::WhatToFocus
                    }
                ),
                "Failed for: {input}"
            );
        }
    }

    #[test]
    fn test_parse_time_filter() {
        let intent = parse("我有 20 分钟");
        match intent {
            ChatIntent::ListTasks {
                filter: TaskFilter::ByTime { minutes },
            } => {
                assert_eq!(minutes, 20);
            }
            _ => panic!("Expected ListTasks with ByTime filter"),
        }

        let intent = parse("20min");
        match intent {
            ChatIntent::ListTasks {
                filter: TaskFilter::ByTime { minutes },
            } => {
                assert_eq!(minutes, 20);
            }
            _ => panic!("Expected ListTasks with ByTime filter"),
        }

        let intent = parse("半小时");
        match intent {
            ChatIntent::ListTasks {
                filter: TaskFilter::ByTime { minutes },
            } => {
                assert_eq!(minutes, 30);
            }
            _ => panic!("Expected ListTasks with ByTime filter"),
        }

        let intent = parse("1小时");
        match intent {
            ChatIntent::ListTasks {
                filter: TaskFilter::ByTime { minutes },
            } => {
                assert_eq!(minutes, 60);
            }
            _ => panic!("Expected ListTasks with ByTime filter"),
        }
    }

    #[test]
    fn test_parse_pass_action() {
        let inputs = vec!["第一项过了", "第一个过了", "1 passed", "first passed"];
        for input in inputs {
            let intent = parse(input);
            match &intent {
                ChatIntent::UpdateTask { index, action, .. } => {
                    assert_eq!(index.as_ref().unwrap(), &IndexRef::Position(0));
                    assert_eq!(action, &TaskAction::Pass);
                }
                _ => panic!("Failed for: {input}"),
            }
        }
    }

    #[test]
    fn test_parse_skip_action() {
        let inputs = vec!["跳过第二个", "跳过 2", "skip second"];
        for input in inputs {
            let intent = parse(input);
            match &intent {
                ChatIntent::UpdateTask { index, action, .. } => {
                    assert_eq!(index.as_ref().unwrap(), &IndexRef::Position(1));
                    assert_eq!(action, &TaskAction::Skip);
                }
                _ => panic!("Failed for: {input}"),
            }
        }
    }

    #[test]
    fn test_parse_reject_action() {
        let inputs = vec!["这个不要了", "拒绝第一个", "reject 1"];
        for input in inputs {
            let intent = parse(input);
            match &intent {
                ChatIntent::UpdateTask { index, action, .. } => {
                    assert!(matches!(index.as_ref().unwrap(), IndexRef::Position(0)));
                    assert_eq!(action, &TaskAction::Reject);
                }
                _ => panic!("Failed for: {input}"),
            }
        }
    }

    #[test]
    fn test_parse_snooze_action() {
        let intent = parse("PPT 那个推后");
        match &intent {
            ChatIntent::UpdateTask {
                index,
                action: TaskAction::Snooze { minutes },
                ..
            } => {
                assert!(matches!(index.as_ref().unwrap(), IndexRef::Position(0)));
                assert_eq!(*minutes, 60);
            }
            _ => panic!("Expected UpdateTask with Snooze action"),
        }

        let intent = parse("第一项推后 30 分钟");
        match &intent {
            ChatIntent::UpdateTask {
                index,
                action: TaskAction::Snooze { minutes },
                ..
            } => {
                assert_eq!(*minutes, 30);
            }
            _ => panic!("Expected UpdateTask with Snooze action"),
        }
    }

    #[test]
    fn test_parse_last_index() {
        let inputs = vec!["最后那个过了", "last one passed"];
        for input in inputs {
            let intent = parse(input);
            match &intent {
                ChatIntent::UpdateTask { index, action, .. } => {
                    assert_eq!(index.as_ref().unwrap(), &IndexRef::Last);
                    assert_eq!(action, &TaskAction::Pass);
                }
                _ => panic!("Failed for: {input}"),
            }
        }
    }

    #[test]
    fn test_resolve_index() {
        assert_eq!(resolve_index(&IndexRef::Position(0), 5), Some(0));
        assert_eq!(resolve_index(&IndexRef::Position(4), 5), Some(4));
        assert_eq!(resolve_index(&IndexRef::Position(5), 5), None);
        assert_eq!(resolve_index(&IndexRef::Last, 5), Some(4));
        assert_eq!(resolve_index(&IndexRef::Last, 0), None);
    }

    #[test]
    fn test_parse_pending_list() {
        let intent = parse("列出待办任务");
        assert!(matches!(
            intent,
            ChatIntent::ListTasks {
                filter: TaskFilter::Pending
            }
        ));

        let intent = parse("list pending");
        assert!(matches!(
            intent,
            ChatIntent::ListTasks {
                filter: TaskFilter::Pending
            }
        ));
    }

    #[test]
    fn test_parse_help() {
        let inputs = vec!["help", "帮助", "怎么用", "?"];
        for input in inputs {
            let intent = parse(input);
            assert!(
                matches!(
                    intent,
                    ChatIntent::Query {
                        kind: QueryKind::Help
                    }
                ),
                "Failed for: {input}"
            );
        }
    }

    #[test]
    fn test_parse_unknown() {
        let intent = parse("随便说点什么");
        assert!(matches!(intent, ChatIntent::Unknown { .. }));
    }

    #[test]
    fn test_parse_time_budget_chinese_basic() {
        // "我有N分钟"
        assert_eq!(parse_time_budget("我有20分钟"), Some(20));
        assert_eq!(parse_time_budget("我有 30 分钟"), Some(30));
        assert_eq!(parse_time_budget("我有5分钟"), Some(5));
    }

    #[test]
    fn test_parse_time_budget_chinese_with_context() {
        // "N分钟内能做什么"
        assert_eq!(parse_time_budget("20分钟内能做什么"), Some(20));
        assert_eq!(parse_time_budget("30分钟内可以做什么"), Some(30));
        assert_eq!(parse_time_budget("15分钟能做什么"), Some(15));
    }

    #[test]
    fn test_parse_time_budget_half_hour() {
        // "半小时" variants
        assert_eq!(parse_time_budget("半小时"), Some(30));
        assert_eq!(parse_time_budget("半小时能做什么"), Some(30));
        assert_eq!(parse_time_budget("我有半小时"), Some(30));
    }

    #[test]
    fn test_parse_time_budget_hour() {
        // "N小时" variants
        assert_eq!(parse_time_budget("1小时"), Some(60));
        assert_eq!(parse_time_budget("2小时"), Some(120));
        assert_eq!(parse_time_budget("1小时能做什么"), Some(60));
        assert_eq!(parse_time_budget("半小时内"), Some(30));
    }

    #[test]
    fn test_parse_time_budget_english() {
        // English patterns
        assert_eq!(parse_time_budget("give me tasks for 20 minutes"), Some(20));
        assert_eq!(parse_time_budget("what can I do in 30 minutes"), Some(30));
        assert_eq!(parse_time_budget("20min"), Some(20));
        assert_eq!(parse_time_budget("tasks for 15 min"), Some(15));
    }

    #[test]
    fn test_parse_time_budget_boundary_values() {
        // Boundary values
        assert_eq!(parse_time_budget("1分钟"), Some(1));
        assert_eq!(parse_time_budget("我有0分钟"), Some(0));
        assert_eq!(parse_time_budget("120分钟"), Some(120));
        // No match
        assert_eq!(parse_time_budget("随便说点什么"), None);
        assert_eq!(parse_time_budget("列出任务"), None);
    }

    #[test]
    fn test_parse_status() {
        let inputs = vec!["现在状态", "有多少任务", "status", "how many tasks"];
        for input in inputs {
            let intent = parse(input);
            assert!(
                matches!(
                    intent,
                    ChatIntent::Query {
                        kind: QueryKind::Status
                    }
                ),
                "Failed for: {input}"
            );
        }
    }
}
