# Bug说明：Goal会话双 `Working` 与任务结果插位

日期：2026-06-01

状态：只读定位完成，未修复

## 1. 问题概述

最新 Goal 会话存在两类前端展示问题：

1. 同一轮任务执行中，前端会同时出现两个 `Working` 状态块。
2. 任务完成后，最终结果不是接在这块任务流的底部，而是插到了该任务块的上方，看起来像“结果跑到任务最顶端”。

这两个问题都与 Goal 首次发送链路、后台任务投影链路，以及时间线渲染顺序有关。

## 2. 现象说明

### 2.1 双 `Working`

在 Goal 模式下首次发送消息时，用户会看到：

- 一个当前会话自己的流式运行块
- 一个标记为 `Background goal task` 的投影运行块

两者都会显示 `Working` 计时和运行状态。

### 2.2 最终结果插到任务块上方

后台 Goal task 执行完成后，最终 assistant 结果会以普通消息插入时间线；
但原来的 live task block 不会立刻被同位置替换，而是短时间继续留在下面。

结果就是：

- 上面先出现最终结果消息
- 下面还挂着刚才那块任务执行中的投影块

视觉上等同于“结果被输出到了这块任务的最顶端”。

## 3. 影响范围

### 3.1 双 `Working` 的影响范围

主要发生在：

- Workbench 中
- Session 已切换到 `goal`
- 当前 session 还没有 `goal_id`
- 用户发送首条 Goal 请求时

### 3.2 结果插位的影响范围

主要发生在：

- 后台 Goal task 通过 projected run 投影到可见会话时
- 任务执行完成、结果被 append 回 goal 会话时

## 4. 复现路径

### 4.1 双 `Working`

1. 打开 workbench。
2. 新建或切到一个 `goal` 会话。
3. 保证当前 session 尚未绑定 `goal_id`。
4. 输入一条需要进入 Goal 执行的请求并发送。
5. 观察时间线，会同时出现两个运行块。

### 4.2 结果插位

1. 让 Goal task 在后台执行并产生 projected run。
2. 等待任务完成。
3. 观察时间线，最终结果会先作为普通 assistant message 出现。
4. 原 projected run 会在下方短暂停留，而不是被原地替换。

## 5. 只读定位结论

## 5.1 根因一：Goal 首次发送被双重派发

文件：

- `apps/desktop/src/windows/AgentWorkspacePanel.tsx`

关键位置：

- `handleSend()` 约 266-289 行

现状逻辑：

1. 如果当前是 `goal` 模式，且 session 还没有 `goal_id`，前端会先：
   - `createGoal(...)`
   - `updateGoalStatus(...)`
   - `setChatSessionKind(activeSessionId, 'goal', goal.id)`
2. 但这段逻辑结束后，代码仍然会继续执行：
   - `return chat.sendMessage(goalOptions);`

这意味着首条 Goal 请求并没有只走“创建 Goal -> 后台执行”这一条链路，而是同时又触发了一次当前会话自己的前台发送。

前端 `useChatSession` 对这两类运行是分开存的：

- 当前 request：`sending + turnStartedAt + toolStates`
- 其他 request：`projectedRuns[requestId]`

相关文件：

- `apps/desktop/src/windows/useChatSession.ts`

关键位置：

- `stream-chat-token` 监听：约 381-407 行
- `tool-execution-update` 监听：约 409-505 行
- `thinking-update` 监听：约 507-535 行

判断逻辑是：

- `state.activeRequestId === payload.request_id`：归为当前前台运行
- 否则：归为 `projectedRuns`

因此首次 Goal 发送一旦同时产生两个不同的 `request_id`，前端就会稳定渲染两个运行块。

## 5.2 根因二：最终结果不是更新 live block，而是追加为新消息

文件：

- `apps/desktop/src-tauri/src/worker.rs`

关键位置：

- `execute_goal_task_via_chat()` 约 704-793 行

现状逻辑：

1. 后台 task 启动时，会先向 goal session append 一条 `[Goal Task Started] ...`。
2. 后台执行期间，通过 `send_message_v2_with_session_projection(...)` 把 thinking / tool / token 投影到可见会话。
3. task 完成后，又额外执行：
   - `append_assistant_message_and_notify(app, goal_session_id, &projection)`

也就是说，最终结果不是写回当前 live run block，而是作为一条新的 assistant message 落库并通知前端刷新。

## 5.3 根因三：时间线渲染顺序固定为 `messages` 在前，`projectedRuns` 在后

文件：

- `apps/desktop/src/windows/ChatTimelinePane.tsx`

关键位置：

- `messages.map(...)`：约 584-620 行
- `projectedRuns.map(...)`：约 621-643 行
- `sending && <LiveRunBlock />`：约 644-656 行

当前顺序是：

1. 先渲染历史/已入库消息 `messages`
2. 再渲染后台投影块 `projectedRuns`
3. 最后才渲染当前前台发送块 `sending`

因此只要最终结果以普通 assistant message 的形式进入 `messages`，它天然就会出现在 projected run 上面。

## 5.4 根因四：`reply_stored` 与 projected run 清理不是原子切换

文件：

- `crates/conductor-core/src/chat/send_v2.rs`
- `apps/desktop/src/windows/useChatSession.ts`

后端时序：

1. `thinking-update phase=done`
2. 落最终 assistant message 到 DB
3. 记录 `reply_stored`

参考位置：

- `crates/conductor-core/src/chat/send_v2.rs` 约 1045-1124 行

前端处理：

1. `reply_stored` 到来时会刷新消息，并尝试清理已结束的 `projectedRuns`
2. 但 projected run 只有在收到 `thinking-update phase=done` 后才会被写入 `finishedAt`
3. 即使已完成，也还有一个默认约 5 秒的清理保留窗口

参考位置：

- `apps/desktop/src/windows/useChatSession.ts` 约 170-178 行
- `apps/desktop/src/windows/useChatSession.ts` 约 362-379 行
- `apps/desktop/src/windows/useChatSession.ts` 约 507-535 行
- `apps/desktop/src/windows/useChatSession.ts` 约 621-634 行

这会导致：

- 最终结果消息已经进入 `messages`
- 但旧的 projected run 还没被立刻移除

所以视觉上形成“结果在上，任务块在下”的插位效果。

## 6. 建议修复方向

## 6.1 修复双 `Working`

建议优先检查 `AgentWorkspacePanel.handleSend()`：

- 当 `goal` 模式下首次创建 `goal_id` 时，不应继续走一次 `chat.sendMessage(...)`
- 应明确只保留一种执行入口

可选方向：

1. 首次 Goal 发送只负责建 Goal，不直接再发当前 chat request。
2. 或者保留当前 chat request，但禁止再由后台 goal task 对同一轮做第二次 projected run。

目标约束：

- 同一轮用户动作只产生一个可见运行态
- 时间线中最多出现一个 `Working`

## 6.2 修复结果插位

建议在“最终结果入库”和“projected run 收尾”之间做同位置收口，而不是简单追加新消息。

可选方向：

1. projected run 完成时，把最终结果合并进当前 live block，再收起该 block。
2. 或者在 `reply_stored` 刷新消息前，先确保对应 projected run 已被立刻清除。
3. 或者调整时间线模型，把 projected run 视作某条消息的临时占位，而不是独立渲染在 `messages` 后面。

目标约束：

- 最终结果应出现在该任务流的尾部
- 不允许出现“结果在上、运行块在下”的短暂错位

## 7. 派工建议

建议拆成两个独立任务：

1. `Goal 首次发送去重`
   - 目标：消除双 `Working`
   - 风险面：Goal 首次建链、session kind 切换、首条 user message 如何归档

2. `Projected run 收尾与结果归位`
   - 目标：让最终结果与任务 live block 同位置收口
   - 风险面：`reply_stored`、`thinking-update done`、消息刷新时序、滚动定位

## 8. 验收建议

### 8.1 双 `Working`

- Goal 模式首次发送后，前端只能出现一个运行块
- 不能同时存在 `sending` 与 `Background goal task` 两块都在 `Working`

### 8.2 结果归位

- 任务完成后，最终结果必须出现在该任务流末尾
- 不应先在上方出现结果、再在下方残留旧运行块
- projected run 收尾后，时间线顺序应保持稳定，不抖动、不倒挂

