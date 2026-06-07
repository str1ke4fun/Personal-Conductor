import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';
import {
  type AgentRun,
  type AgentTask,
  api,
  listWorkspaceActivityProjection,
  type Proposal,
  type Task,
  type WorkspaceActivityProjection,
} from '../ipc/invoke';
import { type DrawerItem, TaskDrawerBody } from './TaskDrawerBody';
import { LiveDot, type SignalTone } from './PanelKit';

/** A single collapsible drawer row: one-line summary that expands to detail. */
function DrawerCard({ item, tone }: { item: DrawerItem; tone: 'busy' | 'pending' }) {
  const signalTone: SignalTone = tone === 'busy' ? 'running' : 'warn';
  return (
    <article className={`task-card-mini drawer-card drawer-card-${tone} deck-drawer-card`}>
      <div className="drawer-card-summary deck-drawer-card-summary">
        <LiveDot tone={signalTone} pulse={tone === 'busy'} size={8} />
        <span className="drawer-card-line deck-drawer-card-body">
          <span className="drawer-card-actor deck-drawer-card-actor">{item.actor}</span>
          <span className="drawer-card-title deck-drawer-card-title">{item.title}</span>
        </span>
        <span className="drawer-card-time deck-drawer-card-time">{item.time}</span>
        <span className="drawer-card-caret deck-drawer-card-caret">›</span>
      </div>
      <div className="drawer-card-detail deck-drawer-card-detail" onClick={(e) => e.stopPropagation()}>
        {item.detail}
      </div>
    </article>
  );
}

interface TaskDrawerPaneProps {
  workspaceId?: string | null;
  onPendingCountChange?: (count: number) => void;
}

export function TaskDrawerPane({ workspaceId = null, onPendingCountChange }: TaskDrawerPaneProps) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [agentTasks, setAgentTasks] = useState<AgentTask[]>([]);
  const [agentRuns, setAgentRuns] = useState<AgentRun[]>([]);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [projection, setProjection] = useState<WorkspaceActivityProjection | null>(null);

  const refresh = useCallback(async () => {
    const proposalStatuses: Proposal['status'][] = ['pending', 'approved', 'running'];
    const [allTasks, allAgentTasks, allAgentRuns, proposalGroups, nextProjection] = await Promise.all([
      api.listTasks(false),
      api.listAgentTasks(false),
      api.listAgentRuns(null, false),
      Promise.all(proposalStatuses.map((status) => api.listProposals(status))),
      workspaceId ? listWorkspaceActivityProjection(workspaceId, 12) : Promise.resolve(null),
    ]);
    const proposalsById = new Map<string, Proposal>();
    proposalGroups.flat().forEach((proposal) => proposalsById.set(proposal.id, proposal));
    setTasks(allTasks);
    setAgentTasks(allAgentTasks);
    setAgentRuns(allAgentRuns);
    setProposals(Array.from(proposalsById.values()));
    setProjection(nextProjection);
  }, [workspaceId]);

  useEffect(() => {
    void refresh();
    const events = [
      'tasks_changed',
      'agent_runs_changed',
      'proposal-changed',
      'goals_changed',
      'agent_teams_changed',
    ];
    const handles = events.map((evt) => listen(evt, () => void refresh()));
    return () => {
      handles.forEach((h) => h.then((dispose) => dispose()).catch(() => {}));
    };
  }, [refresh]);

  return (
    <TaskDrawerBody
      tasks={tasks}
      agentTasks={agentTasks}
      agentRuns={agentRuns}
      proposals={proposals}
      projection={projection}
      onRefresh={refresh}
      onPendingCountChange={onPendingCountChange}
      renderCard={(item, tone) => <DrawerCard key={item.key} item={item} tone={tone} />}
    />
  );
}
