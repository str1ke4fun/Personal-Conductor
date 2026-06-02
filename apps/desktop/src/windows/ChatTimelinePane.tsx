import { Fragment, useEffect, useRef, useState } from 'react';
import { ContentBlock, GoalSeed, ToolCallRecord, parseContentBlocks } from '../ipc/invoke';
import { ToolUseCard } from './ToolUseCard';
import { ToolRunSummary } from './ToolRunSummary';
import { ThinkingBlock } from './ThinkingBlock';
import { MessageAvatar } from './MessageAvatar';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import { CapabilityRequestCard } from '../components/cards/CapabilityRequestCard';
import { PlanCard } from '../components/cards/PlanCard';
import { PermissionCard } from '../components/cards/PermissionCard';
import { CommandRunCard } from '../components/cards/CommandRunCard';
import { CompletionSummaryCard } from '../components/cards/CompletionSummaryCard';
import { BlockedCard } from '../components/cards/BlockedCard';
import type { DisplayMessage, ProjectedRunState, StreamToolState, ToolCardStatus } from './useChatSession';
import { isToolId, normalizeToolId } from './toolIds';

interface ChatTimelinePaneProps {
  messages: DisplayMessage[];
  sending: boolean;
  streamTokens: string[];
  toolStates: Map<string, StreamToolState>;
  thinkingContent: string | null;
  projectedRuns?: ProjectedRunState[];
  endRef: React.RefObject<HTMLDivElement | null>;
  onRetry?: (msg: DisplayMessage) => void;
  onApproveProposal?: (proposalId: string) => void;
  onRejectProposal?: (proposalId: string) => void;
  onApprovePlan?: (plan: {
    title: string;
    steps: Array<{ title: string; detail?: string }>;
    writeScope?: string[];
    diffPreview?: string;
  }) => Promise<void> | void;
  onRejectPlan?: (plan: {
    title: string;
    steps: Array<{ title: string; detail?: string }>;
    writeScope?: string[];
    diffPreview?: string;
  }) => Promise<void> | void;
  onCreateGoal?: (goalSeed: GoalSeed) => Promise<void> | void;
  canCreateGoal?: boolean;
  sessionKind?: 'chat' | 'goal';
  /** Turn-level runtime status fields */
  turnStartedAt?: number | null;
  currentPhase?: string | null;
  toolRunCount?: number;
  activeToolCount?: number;
}

interface ContentBlocksRendererProps {
  blocks: ContentBlock[];
  toolStates: Map<string, StreamToolState>;
  onApproveProposal?: (proposalId: string) => void;
  onRejectProposal?: (proposalId: string) => void;
  onApprovePlan?: (plan: {
    title: string;
    steps: Array<{ title: string; detail?: string }>;
    writeScope?: string[];
    diffPreview?: string;
  }) => Promise<void> | void;
  onRejectPlan?: (plan: {
    title: string;
    steps: Array<{ title: string; detail?: string }>;
    writeScope?: string[];
    diffPreview?: string;
  }) => Promise<void> | void;
  onCreateGoal?: (goalSeed: GoalSeed) => Promise<void> | void;
  canCreateGoal: boolean;
  allowPlanVerdicts: boolean;
}

/** Classify a tool name to determine which card type to render. */
function classifyTool(toolName: string, input: Record<string, any>): 'permission' | 'command' | 'plan' | 'generic' {
  if (isToolId(toolName, 'bash.execute', 'bash.cancel')) {
    return 'command';
  }
  if (isToolId(toolName, 'file.write', 'file.edit')) {
    return 'permission';
  }
  // If input contains plan-like structure
  if (input.plan || input.steps) {
    return 'plan';
  }
  return 'generic';
}

function isPermissionStatus(status: ToolCardStatus): boolean {
  return status === 'awaiting_approval' || status === 'approved' || status === 'denied' || status === 'blocked';
}

/** Minimum consecutive generic tool_use blocks to trigger aggregated view. */
const AGGREGATION_THRESHOLD = 3;

/** Convert a persisted tool_use + tool_result pair into a StreamToolState for ToolRunSummary. */
function toStreamToolState(
  block: ContentBlock & { type: 'tool_use' },
  resultMap: Map<string, ContentBlock & { type: 'tool_result' }>,
  liveStates: Map<string, StreamToolState>,
): StreamToolState {
  const live = liveStates.get(block.id);
  const persisted = resultMap.get(block.id);
  const status: ToolCardStatus = live?.status ?? (persisted ? (persisted.is_error ? 'error' : 'success') : 'pending');
  const result = live?.result ?? (persisted ? { output: persisted.content, error: persisted.is_error ? persisted.content : undefined } : undefined);
  return {
    tool_use_id: block.id,
    tool_name: block.name,
    status,
    input: block.input,
    result,
    duration_ms: live?.duration_ms,
    proposal_id: live?.proposal_id,
  };
}

export function ContentBlocksRenderer({
  blocks,
  toolStates,
  onApproveProposal,
  onRejectProposal,
  onApprovePlan,
  onRejectPlan,
  onCreateGoal,
  canCreateGoal,
  allowPlanVerdicts,
}: ContentBlocksRendererProps) {
  const resultMap = new Map<string, ContentBlock & { type: 'tool_result' }>();
  for (const block of blocks) {
    if (block.type === 'tool_result') {
      resultMap.set(block.tool_use_id, block);
    }
  }

  // Segment blocks: each segment is either a single non-tool block or a
  // consecutive run of tool_use blocks (tool_results are skipped via resultMap).
  type Segment =
    | { kind: 'block'; block: ContentBlock; index: number }
    | { kind: 'tool_run'; items: Array<{ block: ContentBlock & { type: 'tool_use' }; index: number }> };

  const segments: Segment[] = [];
  let currentRun: Array<{ block: ContentBlock & { type: 'tool_use' }; index: number }> = [];

  const flushRun = () => {
    if (currentRun.length > 0) {
      segments.push({ kind: 'tool_run', items: currentRun });
      currentRun = [];
    }
  };

  for (let i = 0; i < blocks.length; i++) {
    const block = blocks[i];
    if (block.type === 'tool_result') continue; // paired with tool_use
    if (block.type === 'tool_use') {
      currentRun.push({ block: block as ContentBlock & { type: 'tool_use' }, index: i });
      continue;
    }
    flushRun();
    segments.push({ kind: 'block', block, index: i });
  }
  flushRun();

  // Render each segment
  return (
    <>
      {segments.map((seg, si) => {
        if (seg.kind === 'block') {
          const block = seg.block;
          if (block.type === 'thinking') {
            return <ThinkingBlock key={`t-${si}`} thinking={block.thinking} />;
          }
          if (block.type === 'capability_request') {
            return (
              <CapabilityRequestCard
                key={`cr-${si}`}
                reason={block.reason}
                suggestedMode={block.suggested_mode}
                goalSeed={block.goal_seed}
                canCreateGoal={canCreateGoal}
                onCreateGoal={onCreateGoal}
              />
            );
          }
          if (block.type === 'plan') {
            return (
              <PlanCard
                key={`plan-${si}`}
                title={block.title}
                steps={block.steps}
                status={(block.status as 'draft' | 'awaiting_approval' | 'approved' | 'rejected' | 'executing') ?? 'draft'}
                writeScope={block.write_scope ?? []}
                diffPreview={block.diff_preview}
                onApprove={allowPlanVerdicts && onApprovePlan ? () => onApprovePlan({
                  title: block.title,
                  steps: block.steps,
                  writeScope: block.write_scope ?? [],
                  diffPreview: block.diff_preview,
                }) : undefined}
                onReject={allowPlanVerdicts && onRejectPlan ? () => onRejectPlan({
                  title: block.title,
                  steps: block.steps,
                  writeScope: block.write_scope ?? [],
                  diffPreview: block.diff_preview,
                }) : undefined}
              />
            );
          }
          if (block.type === 'completion') {
            return (
              <CompletionSummaryCard
                key={`completion-${si}`}
                title={block.title}
                summary={block.summary}
                steps={block.steps}
                durationMs={block.duration_ms}
              />
            );
          }
          if (block.type === 'blocked') {
            return (
              <BlockedCard
                key={`blocked-${si}`}
                title={block.title}
                reason={block.reason}
                actionItems={block.action_items}
              />
            );
          }
          if (block.type === 'text') {
            return <div key={`c-${si}`} className="chat-message-content"><MarkdownRenderer content={block.text} /></div>;
          }
          return null;
        }

        // tool_run segment: split into "prominent" (command/permission) rendered
        // individually, and "generic" candidates for aggregation.
        const prominent: Array<{ block: ContentBlock & { type: 'tool_use' }; index: number }> = [];
        const generic: Array<{ block: ContentBlock & { type: 'tool_use' }; index: number }> = [];

        for (const item of seg.items) {
          const live = toolStates.get(item.block.id);
          const persisted = resultMap.get(item.block.id);
          const status: ToolCardStatus = live?.status ?? (persisted ? (persisted.is_error ? 'error' : 'success') : 'pending');
          const toolCategory = classifyTool(item.block.name, item.block.input);
          if (toolCategory === 'command' || toolCategory === 'permission' || isPermissionStatus(status)) {
            prominent.push(item);
          } else {
            generic.push(item);
          }
        }

        const useAggregation = generic.length >= AGGREGATION_THRESHOLD;
        const aggregatedStates = useAggregation
          ? generic.map((g) => toStreamToolState(g.block, resultMap, toolStates))
          : [];

        return (
          <Fragment key={`tr-${si}`}>
            {/* Prominent tools always render individually */}
            {prominent.map((item) => {
              const live = toolStates.get(item.block.id);
              const persisted = resultMap.get(item.block.id);
              const status: ToolCardStatus = live?.status ?? (persisted ? (persisted.is_error ? 'error' : 'success') : 'pending');
              const result = live?.result ?? (persisted ? { output: persisted.content, error: persisted.is_error ? persisted.content : undefined } : undefined);
              const toolCategory = classifyTool(item.block.name, item.block.input);

              if (toolCategory === 'command') {
                return (
                  <CommandRunCard
                    key={item.index}
                    command={item.block.input.command ?? ''}
                    cwd={item.block.input.cwd}
                    status={status}
                    stdout={result?.stdout ?? (typeof result?.output === 'string' ? result.output : undefined)}
                    stderr={result?.stderr ?? result?.error}
                    exitCode={result?.exit_code}
                    durationMs={live?.duration_ms}
                  />
                );
              }

              return (
                    <PermissionCard
                      key={item.index}
                      toolName={normalizeToolId(item.block.name)}
                      summary={item.block.input.path ?? item.block.input.file_path ?? JSON.stringify(item.block.input).slice(0, 80)}
                      detail={JSON.stringify(item.block.input, null, 2)}
                      status={status}
                  proposalId={live?.proposal_id}
                  riskLevel={item.block.input.risk_level}
                  onApprove={onApproveProposal}
                  onReject={onRejectProposal}
                />
              );
            })}

            {/* Aggregated view for many generic tools */}
            {useAggregation && (
              <ToolRunSummary
                toolStates={aggregatedStates}
                mode="persisted"
                onApprove={onApproveProposal}
                onReject={onRejectProposal}
              />
            )}

            {/* Individual cards when below threshold */}
            {!useAggregation && generic.map((item) => {
              const live = toolStates.get(item.block.id);
              const persisted = resultMap.get(item.block.id);
              const status: ToolCardStatus = live?.status ?? (persisted ? (persisted.is_error ? 'error' : 'success') : 'pending');
              const result = live?.result ?? (persisted ? { output: persisted.content, error: persisted.is_error ? persisted.content : undefined } : undefined);
              return (
                <ToolUseCard
                  key={item.index}
                  toolId={item.block.name}
                  input={item.block.input}
                  status={status}
                  result={result}
                  durationMs={live?.duration_ms}
                  proposalId={live?.proposal_id}
                  onApprove={onApproveProposal}
                  onReject={onRejectProposal}
                />
              );
            })}
          </Fragment>
        );
      })}
    </>
  );
}

export function LegacyToolCallsList({ toolCalls }: { toolCalls: ToolCallRecord[] }) {
  const [expanded, setExpanded] = useState(false);
  if (!toolCalls || toolCalls.length === 0) return null;

  return (
    <div className="tool-calls-container">
      <button className="tool-calls-toggle" onClick={() => setExpanded(!expanded)}>
        {expanded ? '收起' : '展开'}调用记录 ({toolCalls.length})
      </button>
      {expanded && (
        <div className="tool-calls-list">
          {toolCalls.map((tc, i) => {
            let input: Record<string, any> = {};
            try {
              const parsed = JSON.parse(tc.arguments);
              input = parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : { value: parsed };
            } catch {
              input = tc.arguments ? { arguments: tc.arguments } : {};
            }
            let output: any = tc.result;
            try {
              output = JSON.parse(tc.result);
            } catch {
              // Keep plain text output.
            }

            // Use CommandRunCard for bash commands
            if (isToolId(tc.tool_name, 'bash.execute') || tc.tool_name === 'bash') {
              return (
                <CommandRunCard
                  key={i}
                  command={input.command ?? ''}
                  cwd={input.cwd}
                  status={tc.success ? 'success' : 'error'}
                  stdout={typeof output === 'string' ? output : output?.stdout}
                  stderr={output?.stderr}
                  exitCode={output?.exit_code}
                />
              );
            }

            return (
              <ToolUseCard
                key={i}
                toolId={tc.tool_name}
                input={input}
                status={tc.success ? 'success' : 'error'}
                result={tc.success ? { output } : { output, error: typeof output === 'string' ? output : undefined }}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

const PHASE_LABELS: Record<string, string> = {
  thinking: '思考中',
  tool_use: '调用工具',
  synthesizing: '整理结果',
  reading: '阅读内容',
  planning: '规划中',
  writing: '写入中',
  analyzing: '分析中',
  discovering_tools: '正在查找可用工具',
};

function formatElapsed(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

interface RuntimeStatusBarProps {
  turnStartedAt: number;
  currentPhase: string | null;
  toolRunCount: number;
  activeToolCount: number;
}

function RuntimeStatusBar({ turnStartedAt, currentPhase, toolRunCount, activeToolCount }: RuntimeStatusBarProps) {
  const [elapsed, setElapsed] = useState(() => Date.now() - turnStartedAt);

  useEffect(() => {
    const id = window.setInterval(() => {
      setElapsed(Date.now() - turnStartedAt);
    }, 1000);
    return () => window.clearInterval(id);
  }, [turnStartedAt]);

  const phaseLabel = currentPhase ? (PHASE_LABELS[currentPhase] ?? currentPhase) : null;

  return (
    <div className="runtime-status-bar">
      <span className="runtime-status-timer">
        <span className="runtime-status-dot" />
        正在推进 {formatElapsed(elapsed)}
      </span>
      {phaseLabel && (
        <span className="runtime-status-phase">{phaseLabel}</span>
      )}
      {toolRunCount > 0 && (
        <span className="runtime-status-tools">
          已调用 {toolRunCount} 次工具{activeToolCount > 0 ? ` (${activeToolCount} 个仍在运行)` : ''}
        </span>
      )}
    </div>
  );
}

function EmptyTimelineState() {
  return (
    <div className="chat-empty-state">
      <span className="chat-empty-name">清和</span>
      <p className="chat-empty-line">在这里，随时可以说</p>
      <ul className="chat-empty-hints">
        <li>描述一个问题，或者贴段代码</li>
        <li>切到目标模式，交给她执行一个长任务</li>
        <li>不知道说什么，就说「现在在做什么」</li>
      </ul>
    </div>
  );
}

function countActiveProjectedTools(run?: Pick<ProjectedRunState, 'toolStates'> | null): number {
  if (!run) {
    return 0;
  }
  let activeCount = 0;
  run.toolStates.forEach((toolState) => {
    if (toolState.status === 'running' || toolState.status === 'awaiting_approval') {
      activeCount += 1;
    }
  });
  return activeCount;
}

function LiveRunBlock({
  thinkingContent,
  toolStates,
  streamTokens,
  turnStartedAt,
  currentPhase,
  toolRunCount,
  activeToolCount,
  onApproveProposal,
  onRejectProposal,
  label,
}: {
  thinkingContent: string | null;
  toolStates: Map<string, StreamToolState>;
  streamTokens: string[];
  turnStartedAt?: number | null;
  currentPhase?: string | null;
  toolRunCount?: number;
  activeToolCount?: number;
  onApproveProposal?: (proposalId: string) => void;
  onRejectProposal?: (proposalId: string) => void;
  label?: string;
}) {
  return (
    <div className="chat-message chat-assistant">
      <MessageAvatar />
      <div className="chat-message-body">
        {label ? <div className="chat-message-meta-label">{label}</div> : null}
        {(thinkingContent ?? '').length > 0 ? (
          <ThinkingBlock thinking={thinkingContent ?? ''} />
        ) : null}
        {toolStates.size > 0 && (
          <div className="stream-tools">
            {Array.from(toolStates.values()).map((ts) => {
              const toolCategory = classifyTool(ts.tool_name, ts.input ?? {});

              if (toolCategory === 'command') {
                return (
                  <CommandRunCard
                    key={ts.tool_use_id}
                    command={ts.input?.command ?? ''}
                    cwd={ts.input?.cwd}
                    status={ts.status}
                    stdout={ts.result?.stdout}
                    stderr={ts.result?.stderr ?? ts.result?.error}
                    exitCode={ts.result?.exit_code}
                    durationMs={ts.duration_ms}
                  />
                );
              }

              if (toolCategory === 'permission' || isPermissionStatus(ts.status)) {
                return (
                  <PermissionCard
                    key={ts.tool_use_id}
                    toolName={normalizeToolId(ts.tool_name)}
                    summary={ts.input?.path ?? ts.input?.file_path ?? ''}
                    detail={JSON.stringify(ts.input, null, 2)}
                    status={ts.status}
                    proposalId={ts.proposal_id}
                    onApprove={onApproveProposal}
                    onReject={onRejectProposal}
                  />
                );
              }

              return (
                <ToolUseCard
                  key={ts.tool_use_id}
                  toolId={ts.tool_name}
                  input={ts.input ?? {}}
                  status={ts.status}
                  result={ts.result}
                  durationMs={ts.duration_ms}
                  proposalId={ts.proposal_id}
                  onApprove={onApproveProposal}
                  onReject={onRejectProposal}
                />
              );
            })}
          </div>
        )}
        {streamTokens.length > 0 && (
          <div className="chat-message-content chat-stream-content">
            <MarkdownRenderer content={streamTokens.join('')} />
          </div>
        )}
        {typeof turnStartedAt === 'number' && (
          <RuntimeStatusBar
            turnStartedAt={turnStartedAt ?? 0}
            currentPhase={currentPhase ?? null}
            toolRunCount={toolRunCount ?? 0}
            activeToolCount={activeToolCount ?? 0}
          />
        )}
        {!thinkingContent && toolStates.size === 0 && streamTokens.length === 0 && !turnStartedAt && (
          <div className="chat-message-typing">正在整理结果...</div>
        )}
      </div>
    </div>
  );
}

export function ChatTimelinePane({
  messages,
  sending,
  streamTokens,
  toolStates,
  thinkingContent,
  projectedRuns = [],
  endRef,
  onRetry,
  onApproveProposal,
  onRejectProposal,
  onApprovePlan,
  onRejectPlan,
  onCreateGoal,
  canCreateGoal = false,
  sessionKind = 'chat',
  turnStartedAt,
  currentPhase,
  toolRunCount,
  activeToolCount,
}: ChatTimelinePaneProps) {
  const scrollContainerRef = useRef<HTMLDivElement | null>(null);
  const shouldStickToBottomRef = useRef(true);

  useEffect(() => {
    const node = scrollContainerRef.current;
    if (!node || !shouldStickToBottomRef.current) return;
    if (typeof node.scrollTo === 'function') {
      node.scrollTo({ top: node.scrollHeight, behavior: 'auto' });
    } else {
      node.scrollTop = node.scrollHeight;
    }
  }, [
    messages,
    projectedRuns,
    sending,
    thinkingContent,
    streamTokens.length,
    turnStartedAt,
    currentPhase,
    toolRunCount,
    activeToolCount,
  ]);

  const handleScroll = () => {
    const node = scrollContainerRef.current;
    if (!node) return;
    const distanceFromBottom = node.scrollHeight - node.scrollTop - node.clientHeight;
    shouldStickToBottomRef.current = distanceFromBottom < 48;
  };

  const inlineProjectedRequestIds = new Set(
    messages.flatMap((msg) => {
      if (msg.role !== 'assistant') {
        return [];
      }
      return parseContentBlocks(msg)
        .filter(
          (block): block is ContentBlock & { type: 'runtime_projection'; request_id: string } =>
            block.type === 'runtime_projection',
        )
        .map((block) => block.request_id);
    }),
  );

  return (
    <div
      ref={scrollContainerRef}
      className="chat-messages"
      onScroll={handleScroll}
    >
      {messages.length === 0 && projectedRuns.length === 0 && !sending ? (
        <EmptyTimelineState />
      ) : (
        messages.map((msg) => {
          const blocks = parseContentBlocks(msg);
          const runtimeProjection = blocks.find(
            (block): block is ContentBlock & { type: 'runtime_projection'; request_id: string; label: string } =>
              block.type === 'runtime_projection',
          );
          if (runtimeProjection) {
            const projectedRun = projectedRuns.find((run) => run.requestId === runtimeProjection.request_id);
            return (
              <LiveRunBlock
                key={msg.id}
                label={runtimeProjection.label}
                thinkingContent={projectedRun?.thinkingContent ?? null}
                toolStates={projectedRun?.toolStates ?? new Map()}
                streamTokens={projectedRun?.streamTokens ?? []}
                turnStartedAt={projectedRun?.turnStartedAt ?? null}
                currentPhase={projectedRun?.currentPhase ?? null}
                toolRunCount={projectedRun?.toolRunCount ?? 0}
                activeToolCount={countActiveProjectedTools(projectedRun)}
                onApproveProposal={onApproveProposal}
                onRejectProposal={onRejectProposal}
              />
            );
          }
          const hasContentBlockTools = blocks.some((b) => b.type === 'tool_use');
          return (
            <div key={msg.id} className={`chat-message chat-${msg.role}${msg.error ? ' chat-error' : ''}`}>
              {msg.role === 'assistant' && <MessageAvatar />}
              <div className="chat-message-body">
              {msg.role === 'assistant' ? (
                <ContentBlocksRenderer
                  blocks={blocks}
                  toolStates={toolStates}
                  onApproveProposal={onApproveProposal}
                  onRejectProposal={onRejectProposal}
                  onApprovePlan={onApprovePlan}
                  onRejectPlan={onRejectPlan}
                  onCreateGoal={onCreateGoal}
                  canCreateGoal={canCreateGoal}
                  allowPlanVerdicts={sessionKind !== 'goal'}
                />
              ) : (
                <div className="chat-message-content">{msg.content}</div>
              )}
              {msg.role === 'assistant' && !hasContentBlockTools && msg.tool_calls && msg.tool_calls.length > 0 && (
                <LegacyToolCallsList toolCalls={msg.tool_calls} />
              )}
              <div className="chat-message-time">
                {new Date(msg.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
              </div>
              {msg.error && onRetry && (
                <button className="chat-retry-btn" onClick={() => onRetry(msg)}>
                  重试
                </button>
              )}
              </div>
            </div>
          );
        })
      )}
      {projectedRuns.filter((run) => !inlineProjectedRequestIds.has(run.requestId)).map((run) => {
        return (
          <LiveRunBlock
            key={run.requestId}
            label="Goal 持续执行中"
            thinkingContent={run.thinkingContent}
            toolStates={run.toolStates}
            streamTokens={run.streamTokens}
            turnStartedAt={run.turnStartedAt}
            currentPhase={run.currentPhase}
            toolRunCount={run.toolRunCount}
            activeToolCount={countActiveProjectedTools(run)}
            onApproveProposal={onApproveProposal}
            onRejectProposal={onRejectProposal}
          />
        );
      })}
      {sending && (
        <LiveRunBlock
          thinkingContent={thinkingContent}
          toolStates={toolStates}
          streamTokens={streamTokens}
          turnStartedAt={turnStartedAt}
          currentPhase={currentPhase}
          toolRunCount={toolRunCount}
          activeToolCount={activeToolCount}
          onApproveProposal={onApproveProposal}
          onRejectProposal={onRejectProposal}
        />
      )}
      {false && sending && (
        <div className="chat-message chat-assistant">
          {(thinkingContent ?? '').length > 0 ? (
            <ThinkingBlock thinking={thinkingContent ?? ''} />
          ) : null}
          {toolStates.size > 0 && (
            <div className="stream-tools">
              {Array.from(toolStates.values()).map((ts) => {
                const toolCategory = classifyTool(ts.tool_name, ts.input ?? {});

                if (toolCategory === 'command') {
                  return (
                    <CommandRunCard
                      key={ts.tool_use_id}
                      command={ts.input?.command ?? ''}
                      cwd={ts.input?.cwd}
                      status={ts.status}
                      stdout={ts.result?.stdout}
                      stderr={ts.result?.stderr ?? ts.result?.error}
                      exitCode={ts.result?.exit_code}
                      durationMs={ts.duration_ms}
                    />
                  );
                }

                if (toolCategory === 'permission' || isPermissionStatus(ts.status)) {
                  return (
                    <PermissionCard
                      key={ts.tool_use_id}
                      toolName={normalizeToolId(ts.tool_name)}
                      summary={ts.input?.path ?? ts.input?.file_path ?? ''}
                      detail={JSON.stringify(ts.input, null, 2)}
                      status={ts.status}
                      proposalId={ts.proposal_id}
                      onApprove={onApproveProposal}
                      onReject={onRejectProposal}
                    />
                  );
                }

                return (
                  <ToolUseCard
                    key={ts.tool_use_id}
                    toolId={ts.tool_name}
                    input={ts.input ?? {}}
                    status={ts.status}
                    result={ts.result}
                    durationMs={ts.duration_ms}
                    proposalId={ts.proposal_id}
                    onApprove={onApproveProposal}
                    onReject={onRejectProposal}
                  />
                );
              })}
            </div>
          )}
          {streamTokens.length > 0 && (
            <div className="chat-message-content chat-stream-content">
              <MarkdownRenderer content={streamTokens.join('')} />
            </div>
          )}
          {typeof turnStartedAt === 'number' && (
            <RuntimeStatusBar
              turnStartedAt={turnStartedAt ?? 0}
              currentPhase={currentPhase ?? null}
              toolRunCount={toolRunCount ?? 0}
              activeToolCount={activeToolCount ?? 0}
            />
          )}
          {!thinkingContent && toolStates.size === 0 && streamTokens.length === 0 && !turnStartedAt && (
            <div className="chat-message-typing">稍等...</div>
          )}
        </div>
      )}
      <div ref={endRef as any} />
    </div>
  );
}
