import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ChatTimelinePane } from './ChatTimelinePane';
import type { DisplayMessage } from './useChatSession';

function makeMessage(contentBlocks: unknown): DisplayMessage {
  return {
    id: 'msg-1',
    role: 'assistant',
    content: JSON.stringify(contentBlocks),
    created_at: '2026-05-31T00:00:00Z',
  };
}

describe('ChatTimelinePane', () => {
  it('renders capability requests and forwards goal creation', () => {
    const onCreateGoal = vi.fn();
    render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'capability_request',
              reason: 'This should run as a long task.',
              suggested_mode: 'long',
              goal_seed: {
                title: 'Long task seed',
                objective: 'Conversation context goes here.',
              },
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
        onCreateGoal={onCreateGoal}
        canCreateGoal
      />,
    );

    expect(screen.getByText('Long task seed')).toBeTruthy();
    expect(
      screen.queryByText(/持续推进直到产出阶段结果/)
    ).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '转为 Goal' }));

    expect(onCreateGoal).toHaveBeenCalledWith({
      title: 'Long task seed',
      objective: 'Conversation context goes here.',
    });
  });

  it('dismisses capability requests without leaving a follow-up coaching banner', () => {
    render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'capability_request',
              reason: 'This should run as a long task.',
              suggested_mode: 'long',
              goal_seed: {
                title: 'Long task seed',
                objective: 'Conversation context goes here.',
              },
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
        canCreateGoal
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '保持聊天' }));

    expect(screen.queryByText('Long task seed')).toBeNull();
    expect(screen.queryByText(/不会创建 Goal/)).toBeNull();
  });

  it('passes write scope and diff preview through plan approval callbacks', () => {
    const onApprovePlan = vi.fn();
    const { container } = render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'plan',
              title: 'Implementation plan',
              status: 'awaiting_approval',
              steps: [
                { title: 'Patch runtime' },
                { title: 'Verify smoke test', detail: 'Run the example path.' },
              ],
              write_scope: ['crates/conductor-core/src/runtime_api.rs'],
              diff_preview: 'diff --git a/runtime_api.rs b/runtime_api.rs',
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
        onApprovePlan={onApprovePlan}
      />,
    );

    const approveButton = container.querySelector('.plan-btn.approve');
    expect(approveButton).toBeTruthy();
    fireEvent.click(approveButton as HTMLButtonElement);

    expect(onApprovePlan).toHaveBeenCalledWith({
      title: 'Implementation plan',
      steps: [
        { title: 'Patch runtime' },
        { title: 'Verify smoke test', detail: 'Run the example path.' },
      ],
      writeScope: ['crates/conductor-core/src/runtime_api.rs'],
      diffPreview: 'diff --git a/runtime_api.rs b/runtime_api.rs',
    });
  });

  it('renders completion summaries before tool traces', () => {
    const { container } = render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'completion',
              title: '已生成可审阅结果',
              summary: '先给你结论，再展开执行细节。',
              steps: [
                { label: '工具调用', detail: '共 2 个，成功 2 个，失败 0 个', status: 'done' },
              ],
            },
            {
              type: 'text',
              text: '这里是完整结论。',
            },
            {
              type: 'tool_use',
              id: 'tool-1',
              name: 'bash.execute',
              input: {
                command: 'echo hi',
              },
            },
            {
              type: 'tool_result',
              tool_use_id: 'tool-1',
              content: '{"stdout":"hi"}',
              is_error: false,
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
      />,
    );

    const summaryCard = container.querySelector('.completion-card');
    const commandCard = container.querySelector('.cmd-card');
    expect(summaryCard).toBeTruthy();
    expect(commandCard).toBeTruthy();
    expect(summaryCard!.compareDocumentPosition(commandCard!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByText('这里是完整结论。')).toBeTruthy();
  });

  it('renders blocked follow-up requests as dedicated cards', () => {
    render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'blocked',
              title: '需要你确认发布窗口',
              reason: '当前环境没有明确的发布时间，不能继续自动发布。',
              action_items: ['确认发布时间', '提供目标环境'],
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
      />,
    );

    expect(screen.getByText('需要你确认发布窗口')).toBeTruthy();
    expect(screen.getByText('需要你处理：')).toBeTruthy();
    expect(screen.getByText('确认发布时间')).toBeTruthy();
  });

  it('renders goal-mode plan cards as read-only without approval actions', () => {
    const { container } = render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'plan',
              title: 'Goal execution plan',
              status: 'awaiting_approval',
              steps: [{ title: 'Inspect runtime chain' }],
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
        sessionKind="goal"
        onApprovePlan={vi.fn()}
        onRejectPlan={vi.fn()}
      />,
    );

    expect(container.querySelector('.plan-btn.approve')).toBeNull();
    expect(container.querySelector('.plan-btn.deny')).toBeNull();
  });

  it('treats double-underscore tool ids as their dotted frontend variants', () => {
    render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'tool_use',
              id: 'tool-1',
              name: 'bash__execute',
              input: {
                command: 'echo hi',
                cwd: 'I:/personal-agent',
              },
            },
            {
              type: 'tool_result',
              tool_use_id: 'tool-1',
              content: '{"stdout":"hi"}',
              is_error: false,
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
      />,
    );

    expect(screen.getByText('echo hi')).toBeTruthy();
    expect(screen.getByText('Success')).toBeTruthy();
  });

  it('renders projected background runs in the dialogue area', () => {
    render(
      <ChatTimelinePane
        messages={[]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        projectedRuns={[
          {
            requestId: 'bg-1',
            streamTokens: ['partial output'],
            toolStates: new Map(),
            thinkingContent: 'Planning background task',
            turnStartedAt: Date.now(),
            currentPhase: 'planning',
            toolRunCount: 0,
            finishedAt: null,
          },
        ]}
        endRef={{ current: null }}
      />,
    );

    expect(screen.getByText('Goal 持续执行中')).toBeTruthy();
    expect(screen.getByText('partial output')).toBeTruthy();
  });

  it('renders runtime projection placeholders inline without duplicating the projected tail lane', () => {
    const { container } = render(
      <ChatTimelinePane
        messages={[
          makeMessage([
            {
              type: 'runtime_projection',
              request_id: 'bg-1',
              label: 'Goal projection placeholder',
            },
          ]),
        ]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        projectedRuns={[
          {
            requestId: 'bg-1',
            streamTokens: ['partial output'],
            toolStates: new Map(),
            thinkingContent: 'Writing back the final result',
            turnStartedAt: Date.now(),
            currentPhase: 'synthesizing',
            toolRunCount: 0,
            finishedAt: null,
          },
        ]}
        endRef={{ current: null }}
      />,
    );

    expect(screen.getByText('Goal projection placeholder')).toBeTruthy();
    expect(screen.getByText('partial output')).toBeTruthy();
    expect(container.querySelectorAll('.chat-message-meta-label').length).toBe(1);
  });

  it('renders the richer empty state when the timeline is blank', () => {
    render(
      <ChatTimelinePane
        messages={[]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
        canCreateGoal
      />,
    );

    expect(screen.getByText('把当前工作直接交给它')).toBeTruthy();
    expect(screen.getByText('读代码并解释')).toBeTruthy();
    expect(screen.queryByText(/短问题会直接在聊天里完成/)).toBeNull();
  });

  it('does not force-scroll to the bottom after the user scrolls up', () => {
    const { container, rerender } = render(
      <ChatTimelinePane
        messages={[makeMessage([{ type: 'text', text: 'hello' }])]}
        sending={false}
        streamTokens={[]}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
      />,
    );

    const timeline = container.querySelector('.chat-messages') as HTMLDivElement;
    const scrollToMock = vi.fn();
    Object.defineProperty(timeline, 'scrollTo', { value: scrollToMock, configurable: true });
    Object.defineProperty(timeline, 'scrollHeight', { value: 1000, configurable: true });
    Object.defineProperty(timeline, 'clientHeight', { value: 400, configurable: true });
    Object.defineProperty(timeline, 'scrollTop', { value: 100, writable: true, configurable: true });

    fireEvent.scroll(timeline);
    scrollToMock.mockClear();

    rerender(
      <ChatTimelinePane
        messages={[makeMessage([{ type: 'text', text: 'hello' }])]}
        sending={true}
        streamTokens={['streamed']}
        toolStates={new Map()}
        thinkingContent={null}
        endRef={{ current: null }}
      />,
    );

    expect(scrollToMock).not.toHaveBeenCalled();
  });
});
