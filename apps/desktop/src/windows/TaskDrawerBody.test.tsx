import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TaskDrawerBody } from './TaskDrawerBody';

describe('TaskDrawerBody', () => {
  it('shows a concise empty state without workflow coaching', () => {
    render(
      <TaskDrawerBody
        tasks={[]}
        agentTasks={[]}
        agentRuns={[]}
        proposals={[]}
        projection={null}
        onRefresh={vi.fn()}
        renderCard={() => null}
      />,
    );

    expect(screen.getByText('当前没有后台工作。')).toBeTruthy();
    expect(screen.queryByText(/审批请求和后台执行会显示在这里/)).toBeNull();
  });

  it('uses neutral pending labels for queued work', () => {
    render(
      <TaskDrawerBody
        tasks={[
          {
            id: 'task-1',
            title: 'Hook task',
            kind: 'review',
            source: 'claude',
            status: 'pending',
            current_request: 'Inspect the latest diff',
            created_at: '2026-06-02T00:00:00Z',
            last_event_at: '2026-06-02T00:01:00Z',
            artifact: {},
          } as any,
        ]}
        agentTasks={[]}
        agentRuns={[]}
        proposals={[]}
        projection={null}
        onRefresh={vi.fn()}
        renderCard={(item) => (
          <div key={item.key}>
            <span>{item.actor}</span>
            <span>{item.title}</span>
          </div>
        )}
      />,
    );

    expect(screen.getByText('1 待处理')).toBeTruthy();
    expect(screen.getByText('待处理 (1)')).toBeTruthy();
    expect(screen.getAllByText('待处理').length).toBeGreaterThan(0);
    expect(screen.queryByText('待审阅')).toBeNull();
  });
});
