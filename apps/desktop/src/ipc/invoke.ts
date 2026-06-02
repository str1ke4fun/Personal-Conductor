import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type TaskStatus = 'pending' | 'in_progress' | 'passed' | 'rejected' | 'skipped';
export type SettingsTab = 'llm' | 'reminders' | 'pet' | 'persona' | 'capabilities' | 'proactive';

export interface LlmSettings {
  provider: string;
  model: string;
  baseUrl: string;
  apiKeySet: boolean;
  apiKey?: string;
  temperature: number;
}

export interface ReminderSettings {
  enabled: boolean;
  workdayStart: string;
  workdayEnd: string;
  quietMinutes: number;
  dailyDigest: boolean;
}

export interface PetSettings {
  enabled: boolean;
  alwaysOnTop: boolean;
  clickThroughWhenIdle: boolean;
  scale: number;
  avatarLocked?: boolean;
  avatar: AvatarSettings;
}

export interface AppSettings {
  llm: LlmSettings;
  reminders: ReminderSettings;
  pet: PetSettings;
  persona: PersonaSettings;
  proactive: ProactiveSettings;
}

export interface WorkspaceStatus {
  root: string;
  exists: boolean;
  writable: boolean;
}

export type WorkspaceKind = 'code' | 'document' | 'office' | 'notes' | 'generic';
export type TrustLevel = 'trusted' | 'ask_write' | 'read_only' | 'untrusted';

export interface Workspace {
  id: string;
  root: string;
  name: string;
  kind: WorkspaceKind;
  trust_level: TrustLevel;
  created_at: string;
  updated_at: string;
  last_active_at?: string | null;
  metadata: Record<string, unknown>;
}

export interface AvatarSettings {
  mode: 'video' | 'live2d';
  videoSrc: string;
  fit: 'contain' | 'cover';
  loopVideo: boolean;
  muted: boolean;
  playbackRate: number;
}

export type AvatarId = 'original' | 'document_secretary' | 'programmer';

export interface AvatarState {
  id: string;
  avatarId: AvatarId;
  activityVariant: string;
  updatedAt: string;
  lockedMainAvatar: boolean;
  lockedActivityVariant: boolean;
}

export type MoodZone = 'happy' | 'content' | 'neutral' | 'bored' | 'shy' | 'sad' | 'frustrated';
export type RelationshipStage = 'stranger' | 'acquaintance' | 'colleague' | 'friend' | 'close_friend';

export interface ExpressionState {
  mood_zone: MoodZone;
  mood_label: string;
  valence: number;
  arousal: number;
  relationship_stage: RelationshipStage;
  relationship_label: string;
  affection_value: number;
}

export interface MoodState {
  zone: MoodZone;
  label: string;
  valence: number;
  arousal: number;
}

export interface PetExpressionPayload {
  avatar_id: string;
  activity_variant: string;
  mood_zone: MoodZone;
  relationship_stage: RelationshipStage;
  pet_state: string;
}

export interface PersonaSkill {
  id: string;
  name: string;
  description: string;
  prompt: string;
  enabled: boolean;
}

export interface PersonaSettings {
  name: string;
  style: string;
  systemPrompt: string;
  skills: PersonaSkill[];
}

export interface ToolTriggerSettings {
  processName: string;
  label: string;
  prompt: string;
  enabled: boolean;
}

export interface ProactiveSettings {
  enabled: boolean;
  focusDetection: boolean;
  cooldownMinutes: number;
  quietWhenFullscreen: boolean;
  toolTriggers: ToolTriggerSettings[];
}

export interface ForegroundApp {
  title: string;
  processName: string;
  processPath?: string | null;
}

export type ChatRole = 'user' | 'assistant' | 'system';
export type ChatTaskMode = 'short' | 'long';
export type ChatCapability = 'read_only' | 'ask_write' | 'trusted';

export interface ToolCallRecord {
  tool_name: string;
  arguments: string;
  result: string;
  success: boolean;
}

export interface ChatMessage {
  id: string;
  role: ChatRole;
  content: string;
  created_at: string;
  seq?: number;
  tool_calls?: ToolCallRecord[];
}

export interface ChatReply {
  message: ChatMessage;
  history: ChatMessage[];
  bubble_summary?: string;
}

export interface GoalSeed {
  title: string;
  objective: string;
}

export interface CapabilityRequest {
  reason: string;
  suggested_mode: string;
  goal_seed: GoalSeed;
}

export interface PlanStep {
  title: string;
  detail?: string;
}

export interface CompletionStep {
  label: string;
  detail?: string;
  status: 'done' | 'skipped' | 'failed' | string;
}

/** Content block variant aligned with the Anthropic API content-blocks format. */
export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; thinking: string }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, any> }
  | { type: 'tool_result'; tool_use_id: string; content: string; is_error: boolean }
  | { type: 'capability_request'; reason: string; suggested_mode: string; goal_seed: GoalSeed }
  | {
      type: 'plan';
      title: string;
      steps: PlanStep[];
      status: 'draft' | 'awaiting_approval' | 'approved' | 'rejected' | 'executing' | string;
      write_scope?: string[];
      diff_preview?: string;
    }
  | {
      type: 'completion';
      title: string;
      summary?: string;
      steps?: CompletionStep[];
      duration_ms?: number;
    }
  | {
      type: 'blocked';
      title: string;
      reason: string;
      action_items?: string[];
    }
  | {
      type: 'runtime_projection';
      request_id: string;
      label: string;
    };

/** Content-blocks variant of a chat message. */
export interface ChatMessageV2 {
  id: string;
  role: 'user' | 'assistant';
  content_blocks: ContentBlock[];
  created_at: string;
  seq?: number;
}

/**
 * Parse the `content` field of a legacy `ChatMessage` into `ContentBlock[]`.
 *
 * If the content is a JSON-encoded array of `ContentBlock` objects it is
 * deserialized directly; otherwise the raw string is wrapped in a single
 * `Text` block.
 */
export function parseContentBlocks(msg: ChatMessage): ContentBlock[] {
  try {
    const parsed = JSON.parse(msg.content);
    if (Array.isArray(parsed) && parsed.every((b: any) => typeof b === 'object' && 'type' in b)) {
      return parsed as ContentBlock[];
    }
  } catch {
    // not JSON — fall through
  }
  return [{ type: 'text', text: msg.content }];
}

export interface Task {
  id: string;
  source: string;
  kind: string;
  artifact: {
    file?: string | null;
    anchor?: string | null;
  };
  summary_ref?: string | null;
  est_minutes?: number | null;
  focus_hint?: string | null;
  status: TaskStatus;
  created_at: string;
  session_id?: string | null;
  terminal_id?: string | null;
  cwd?: string | null;
  current_request?: string | null;
  last_output_summary?: string | null;
  last_event_at?: string | null;
  permission_summary?: string | null;
}

export interface TaskActivityStats {
  pending_total: number;
  in_progress_total: number;
  active_hook_sessions: number;
  pending_hook_reviews: number;
  pending_other: number;
}

export interface ChatSessionSummary {
  id: string;
  title: string;
  workspace_id?: string | null;
  /** Session kind: 'chat' (default) or 'goal' (long-task / autonomous goal session). */
  session_kind?: 'chat' | 'goal';
  /** Associated goal ID when session_kind = 'goal'. */
  goal_id?: string | null;
  message_count: number;
  last_message_preview?: string | null;
  created_at: string;
  updated_at: string;
  /** Whether this session currently has an active LLM run. */
  working: boolean;
  /** ISO timestamp when the current run started (if working). */
  working_since?: string | null;
  /** Elapsed milliseconds since the run started (if working). */
  working_elapsed_ms?: number | null;
  /** Current processing phase (e.g. "tool_calling", "planning"). */
  working_stage?: string | null;
  /** Number of currently executing tools. */
  active_tool_count?: number | null;
  /** Total number of tool runs in this turn. */
  tool_run_count?: number | null;
}

export interface TaskWithSummary {
  task: Task;
  summary?: string | null;
}

export type AgentTaskStatus = 'pending' | 'in_progress' | 'completed';
export type AgentRunStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'stopped';
export type PersistentToolCallStatus =
  | 'pending'
  | 'executing'
  | 'succeeded'
  | 'failed'
  | 'approval_required'
  | string;
export type CommandRunStatus =
  | 'prepared'
  | 'awaiting_permission'
  | 'starting'
  | 'streaming'
  | 'exited'
  | 'timed_out'
  | 'killed';
export type AgentTeamStatus = 'active' | 'archived';
export type AgentTeamLifecycle =
  | 'draft'
  | 'planning'
  | 'awaiting_plan_approval'
  | 'executing'
  | 'awaiting_review'
  | 'accepted'
  | 'rework_required'
  | 'archived';
export type AgentMemberStatus = 'active' | 'paused' | 'stopped';
export type AgentMessageKind =
  | 'message'
  | 'broadcast'
  | 'shutdown_request'
  | 'shutdown_response'
  | 'plan_approval_request'
  | 'plan_approval_response'
  | 'review_verdict_request';

export interface AgentTask {
  id: string;
  task_list_id: string;
  subject: string;
  description: string;
  active_form?: string | null;
  owner?: string | null;
  status: AgentTaskStatus;
  workspace_id?: string | null;
  source: string;
  kind: string;
  est_minutes?: number | null;
  blocks: string[];
  blocked_by: string[];
  metadata_json?: Record<string, unknown> | null;
  created_at: string;
  updated_at: string;
}

export interface AgentRun {
  id: string;
  agent_id: string;
  role: string;
  workspace_id?: string | null;
  cwd?: string | null;
  status: AgentRunStatus;
  pid?: number | null;
  command_json?: Record<string, unknown> | null;
  input_ref?: string | null;
  output_ref?: string | null;
  error?: string | null;
  started_at: string;
  updated_at: string;
  finished_at?: string | null;
  metadata_json?: Record<string, unknown> | null;
}

export interface AgentRunOutput {
  run: AgentRun;
  stdout: string;
  stderr: string;
  output_ref?: string | null;
}

export interface ToolCall {
  id: string;
  session_id?: string | null;
  workspace_id?: string | null;
  llm_tool_call_id?: string | null;
  tool_id: string;
  input_json: string;
  output_json?: string | null;
  status: PersistentToolCallStatus;
  error?: string | null;
  started_at: string;
  completed_at?: string | null;
  duration_ms?: number | null;
  agent_run_id?: string | null;
  risk_level?: string | null;
  proposal_id?: string | null;
  permission_grant_id?: string | null;
  command_run_id?: string | null;
}

export interface CommandRun {
  id: string;
  session_id?: string | null;
  tool_call_id?: string | null;
  agent_run_id?: string | null;
  permission_grant_id?: string | null;
  risk_level?: string | null;
  env_delta_json?: string | null;
  command: string;
  cwd: string;
  status: CommandRunStatus;
  exit_code?: number | null;
  stdout_tail: string;
  stderr_tail: string;
  pid?: number | null;
  started_at?: string | null;
  completed_at?: string | null;
  created_at: string;
}

export interface ActivityArtifactRef {
  label: string;
  file?: string | null;
  summary_ref?: string | null;
  output_ref?: string | null;
  result_ref?: string | null;
}

export interface ActivityToolCallRef {
  id: string;
  tool_id: string;
  status: string;
  command_run_id?: string | null;
  risk_level?: string | null;
}

export interface ActivityCommandRunRef {
  id: string;
  command: string;
  status: string;
  exit_code?: number | null;
}

export interface ActivityAgentRunRef {
  id: string;
  agent_id: string;
  status: string;
  output_ref?: string | null;
  error?: string | null;
}

export interface ActivityAgentTeamRef {
  id: string;
  name: string;
  lifecycle: string;
}

export interface ActivityProjectionItem {
  activity_id: string;
  kind: string;
  status: string;
  title: string;
  actor: string;
  started_at: string;
  updated_at: string;
  session_id?: string | null;
  goal_id?: string | null;
  task_id?: string | null;
  assistant_message?: string | null;
  tool_calls: ActivityToolCallRef[];
  command_runs: ActivityCommandRunRef[];
  agent_runs: ActivityAgentRunRef[];
  agent_teams: ActivityAgentTeamRef[];
  artifacts: ActivityArtifactRef[];
}

export interface WorkspaceActivityProjection {
  workspace_id: string;
  active: ActivityProjectionItem[];
  records: ActivityProjectionItem[];
}

export interface AgentTeam {
  id: string;
  name: string;
  workspace_id?: string | null;
  status: AgentTeamStatus;
  lifecycle: AgentTeamLifecycle;
  write_scope: string[];
  created_at: string;
  updated_at: string;
  metadata_json?: Record<string, unknown> | null;
}

export interface AgentTeamMember {
  team_id: string;
  agent_id: string;
  role: string;
  run_id?: string | null;
  cwd?: string | null;
  status: AgentMemberStatus;
  subscriptions: string[];
  created_at: string;
  updated_at: string;
  metadata_json?: Record<string, unknown> | null;
}

export interface AgentMailboxMessage {
  id: string;
  team_id: string;
  sender_agent_id: string;
  recipient_agent_id?: string | null;
  kind: AgentMessageKind;
  content: string;
  read_at?: string | null;
  created_at: string;
  metadata_json?: Record<string, unknown> | null;
}

export interface AgentTeamSnapshot {
  team: AgentTeam;
  members: AgentTeamMember[];
  recent_messages: AgentMailboxMessage[];
}

export type ProposalStatus = 'pending' | 'approved' | 'running' | 'succeeded' | 'failed' | 'rejected' | 'expired' | 'used';

export interface Proposal {
  id: string;
  workspace_id?: string | null;
  for_cwd: string;
  source: string;
  title: string;
  content: string;
  reason: string;
  tool_id?: string | null;
  tool_input_json?: string | null;
  risk_level: string;
  dry_run: boolean;
  status: ProposalStatus;
  result_ref?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProposalExecutionResult {
  success: boolean;
  output: unknown;
  error?: string | null;
  duration_ms: number;
}

export interface PetWindowState {
  x?: number | null;
  y?: number | null;
  width: number;
  height: number;
  scale: number;
  locked: boolean;
}

export interface MemoryEntry {
  id: string;
  key: string;
  value: string;
  category: string;
  scope: string;
  source: string;
  confidence: number;
  sensitivity: string;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface UserPreferences {
  favorite_topics: string[];
  preferred_time: string;
  chat_style: string;
  avatar_settings: Record<string, string>;
}

export interface ConversationSummary {
  id: string;
  summary: string;
  keywords: string[];
  timestamp: string;
}

export type PlaybackState = 'playing' | 'paused' | 'stopped';

export interface MusicInfo {
  state: PlaybackState;
  title: string | null;
  artist: string | null;
  album: string | null;
  duration: number | null;
  position: number | null;
  timestamp: string;
}

export type SkillContextMode = 'current_workspace' | 'current_document' | 'global';

export interface SkillSpec {
  id: string;
  name: string;
  description: string;
  when_to_use: string[];
  allowed_tools: string[];
  default_avatar_id?: string | null;
  context_mode: SkillContextMode;
  proactive_allowed: boolean;
}

export interface OnboardingStatus {
  completedSteps: string[];
  nextStep: string | null;
  nextStepDescription: string | null;
  isComplete: boolean;
}

export type SceneType = 'default' | 'morning' | 'afternoon' | 'evening' | 'night' | 'music' | 'work' | 'relax' | string;

export interface Scene {
  id: string;
  name: string;
  scene_type: SceneType;
  background_color: string;
  background_image: string | null;
  ambient_sound: string | null;
  description: string;
  available_time: [number, number] | null;
  transitions: string[];
  created_at: string;
}

export interface PersonalityTrait {
  name: string;
  value: number;
  description: string;
}

export interface PromptTemplate {
  id: string;
  name: string;
  template: string;
  category: string;
  variables: string[];
}

export interface ImagePrompt {
  id: string;
  name: string;
  prompt: string;
  negative_prompt: string;
  style: string;
  aspect_ratio: string;
}

export interface Persona {
  id: string;
  name: string;
  description: string;
  avatar: string;
  voice: string;
  personality: PersonalityTrait[];
  tone: string;
  language: string;
  greeting: string;
  farewell: string;
  prompt_templates: PromptTemplate[];
  image_prompts: ImagePrompt[];
  created_at: string;
  updated_at: string;
}

export interface SkillPackage {
  id: string;
  name: string;
  version: string;
  description: string;
  author?: string | null;
  activation: {
    keywords: string[];
    apps: string[];
    url_patterns: string[];
    file_patterns: string[];
  };
  capabilities: string[];
  source: string;
  enabled: boolean;
  body: string;
}

export interface ConnectorSpec {
  id: string;
  name: string;
  description: string;
  implementation_type: string;
  capabilities: Array<{
    capability: string;
    tools: string[];
    risk_level: string;
    requires_confirmation: boolean;
  }>;
  auth_status: string;
  enabled: boolean;
  config_json?: string | null;
}

export const api = {
  listTasks: (onlyPending = true) => invoke<Task[]>('list_tasks', { onlyPending }),
  showTask: (id: string) => invoke<TaskWithSummary>('show_task', { id }),
  getTaskActivityStats: () => invoke<TaskActivityStats>('get_task_activity_stats'),
  createChatSession: (title?: string | null, workspaceId?: string | null) =>
    invoke<ChatSessionSummary>('create_chat_session', { title, workspaceId }),
  ensureChatSession: (title: string, workspaceId?: string | null) =>
    invoke<ChatSessionSummary>('ensure_chat_session', { title, workspaceId }),
  listChatSessions: (limit?: number) => invoke<ChatSessionSummary[]>('list_chat_sessions', { limit }),
  getChatSessionMessages: (sessionId: string, limit?: number) =>
    invoke<ChatMessage[]>('get_chat_session_messages', { sessionId, limit }),
  renameChatSession: (sessionId: string, title: string) =>
    invoke<void>('rename_chat_session', { sessionId, title }),
  archiveChatSession: (sessionId: string) =>
    invoke<void>('archive_chat_session', { sessionId }),
  updateChatSessionWorkspace: (sessionId: string, workspaceId?: string | null) =>
    invoke<void>('update_chat_session_workspace', { sessionId, workspaceId }),
  setChatSessionKind: (sessionId: string, kind: 'chat' | 'goal', goalId?: string | null) =>
    invoke<void>('set_chat_session_kind', { sessionId, kind, goalId: goalId ?? null }),
  listWorkspaces: () => invoke<Workspace[]>('list_workspaces'),
  attachWorkspace: (root: string, name?: string | null, kind?: WorkspaceKind | null) =>
    invoke<Workspace>('attach_workspace', { root, name, kind }),
  listAgentTasks: (includeCompleted = false) => invoke<AgentTask[]>('list_agent_tasks', { includeCompleted }),
  listTasksByBudget: (budgetMinutes: number) => invoke<AgentTask[]>('list_tasks_by_budget', { budgetMinutes }),
  migrateLegacyTasksToTasklist: () => invoke<number>('migrate_legacy_tasks_to_tasklist'),
  listAgentRuns: (workspaceId?: string | null, includeFinished = false) =>
    invoke<AgentRun[]>('list_agent_runs', { workspaceId, includeFinished }),
  readAgentRunOutput: (runId: string, maxBytes?: number) =>
    invoke<AgentRunOutput>('read_agent_run_output', { runId, maxBytes }),
  stopAgentRun: (runId: string) => invoke<AgentRun>('stop_agent_run', { runId }),
  getToolCall: (id: string) => invoke<ToolCall>('get_tool_call', { id }),
  listToolCalls: (filter: {
    sessionId?: string | null;
    workspaceId?: string | null;
    llmToolCallId?: string | null;
    toolId?: string | null;
    status?: string | null;
    proposalId?: string | null;
    commandRunId?: string | null;
    limit?: number | null;
  } = {}) =>
    invoke<ToolCall[]>('list_tool_calls', {
      sessionId: filter.sessionId ?? null,
      workspaceId: filter.workspaceId ?? null,
      llmToolCallId: filter.llmToolCallId ?? null,
      toolId: filter.toolId ?? null,
      status: filter.status ?? null,
      proposalId: filter.proposalId ?? null,
      commandRunId: filter.commandRunId ?? null,
      limit: filter.limit ?? null
    }),
  getCommandRun: (id: string) => invoke<CommandRun>('get_command_run', { id }),
  listCommandRuns: (filter: {
    sessionId?: string | null;
    toolCallId?: string | null;
    agentRunId?: string | null;
    status?: string | null;
    activeOnly?: boolean;
    limit?: number | null;
  } = {}) =>
    invoke<CommandRun[]>('list_command_runs', {
      sessionId: filter.sessionId ?? null,
      toolCallId: filter.toolCallId ?? null,
      agentRunId: filter.agentRunId ?? null,
      status: filter.status ?? null,
      activeOnly: filter.activeOnly ?? false,
      limit: filter.limit ?? null
    }),
  listAgentTeams: (workspaceId?: string | null, includeArchived = false) =>
    invoke<AgentTeam[]>('list_agent_teams', { workspaceId, includeArchived }),
  createAgentTeam: (name: string, workspaceId?: string | null) =>
    invoke<AgentTeam>('create_agent_team', { name, workspaceId }),
  addAgentTeamMember: (teamId: string, agentId: string, role: string, runId?: string | null) =>
    invoke<AgentTeamMember>('add_agent_team_member', { teamId, agentId, role, runId }),
  getAgentTeamSnapshot: (teamId: string, messageLimit?: number) =>
    invoke<AgentTeamSnapshot>('get_agent_team_snapshot', { teamId, messageLimit }),
  submitAgentTeamPlanVerdict: (teamId: string, verdict: 'approved' | 'rejected') =>
    invoke<AgentTeam>('submit_agent_team_plan_verdict', { teamId, verdict }),
  submitAgentTeamReviewVerdict: (teamId: string, verdict: 'accepted' | 'failed') =>
    invoke<AgentTeam>('submit_agent_team_review_verdict', { teamId, verdict }),
  sendAgentMailboxMessage: (
    teamId: string,
    senderAgentId: string,
    recipientAgentId: string | null,
    content: string
  ) =>
    invoke<AgentMailboxMessage[]>('send_agent_mailbox_message', {
      teamId,
      senderAgentId,
      recipientAgentId,
      content
    }),
  listAgentMailbox: (teamId: string, recipientAgentId?: string | null, includeRead = false) =>
    invoke<AgentMailboxMessage[]>('list_agent_mailbox', { teamId, recipientAgentId, includeRead }),
  markAgentMailboxRead: (messageId: string) =>
    invoke<AgentMailboxMessage>('mark_agent_mailbox_read', { messageId }),
  passTask: (id: string) => invoke<void>('pass_task', { id }),
  skipTask: (id: string) => invoke<void>('skip_task', { id }),
  rejectTask: (id: string) => invoke<void>('reject_task', { id }),
  listProposals: (status?: string) => invoke<Proposal[]>('list_proposals', { status }),
  approveProposal: (id: string) => invoke<void>('approve_proposal', { id }),
  executeProposal: (id: string) => invoke<ProposalExecutionResult>('execute_proposal', { id }),
  rejectProposal: (id: string) => invoke<void>('reject_proposal', { id }),
  loadPetWindowState: () => invoke<PetWindowState>('load_pet_window_state'),
  savePetWindowState: (pet: PetWindowState) => invoke<void>('save_pet_window_state', { pet }),
  setPetClickThrough: (through: boolean) => invoke<void>('set_pet_click_through', { through }),
  setAlwaysOnTop: (alwaysOnTop: boolean) => invoke<void>('set_always_on_top', { alwaysOnTop }),
  quietForMinutes: (minutes: number) => invoke<void>('quiet_for_minutes', { minutes }),
  getSettings: () => invoke<AppSettings>('get_settings'),
  getWorkspaceStatus: (workspaceId?: string | null) =>
    invoke<WorkspaceStatus>('get_workspace_status', { workspaceId }),
  saveSettings: (settings: AppSettings) => invoke<AppSettings>('save_settings', { settings }),
  testLlmConnection: (settings: LlmSettings) =>
    invoke<string>('test_llm_connection', { settings }),
  listChatMessages: () => invoke<ChatMessage[]>('list_chat_messages'),
  sendChatMessageV2: (
    message: string,
    sessionId?: string | null,
    taskMode?: ChatTaskMode | null,
    capability?: ChatCapability | null,
    planOnly?: boolean | null,
    approvedWriteScope?: string[] | null,
    requestId?: string | null,
  ) =>
    invoke<ChatReply>('send_chat_message_v2', {
      message,
      sessionId,
      taskMode,
      capability,
      planOnly,
      approvedWriteScope,
      requestId,
    }),
  getCurrentAvatar: () => invoke<AvatarState>('get_current_avatar'),
  setPetAvatar: (avatarId: AvatarId) => invoke<AvatarState>('set_pet_avatar', { avatarId }),
  setActivityVariant: (variant: string) => invoke<AvatarState>('set_activity_variant', { variant }),
  setMainAvatarManual: (avatarId: AvatarId) => invoke<AvatarState>('set_main_avatar_manual', { avatarId }),
  setSubAvatarManual: (variant: string) => invoke<AvatarState>('set_sub_avatar_manual', { variant }),
  toggleAvatarLock: (lockType: 'main' | 'sub', locked: boolean) => invoke<AvatarState>('toggle_avatar_lock', { lockType, locked }),
  getForegroundApp: () => invoke<ForegroundApp>('get_foreground_app'),
  showPetMessage: (content: string) => invoke<void>('show_pet_message', { content }),
  getAffection: () => invoke<number>('get_affection'),
  addAffection: (value: number) => invoke<number>('add_affection', { value }),
  interactAffection: () => invoke<number>('interact_affection'),
  decreaseAffectionOverTime: () => invoke<number>('decrease_affection_over_time'),
  memorySet: (key: string, value: string, category: string) => invoke<void>('memory_set', { key, value, category }),
  memoryGet: (key: string) => invoke<string | null>('memory_get', { key }),
  memoryGetByCategory: (category: string) => invoke<MemoryEntry[]>('memory_get_by_category', { category }),
  memorySavePreferences: (prefs: UserPreferences) => invoke<void>('memory_save_preferences', { prefs }),
  memoryLoadPreferences: () => invoke<UserPreferences>('memory_load_preferences'),
  memoryAddConversation: (summary: string, keywords: string[]) => invoke<void>('memory_add_conversation', { summary, keywords }),
  memoryGetRecentConversations: (limit: number) => invoke<ConversationSummary[]>('memory_get_recent_conversations', { limit }),
  memorySearchConversations: (query: string) => invoke<ConversationSummary[]>('memory_search_conversations', { query }),
  memoryList: (category?: string | null, status?: string | null) => invoke<MemoryEntry[]>('memory_list', { category, status }),
  memoryUpdateStatus: (id: string, status: string) => invoke<boolean>('memory_update_status', { id, status }),
  memoryForget: (id: string) => invoke<boolean>('memory_forget', { id }),
  memoryRebuildEmbeddings: () => invoke<number>('memory_rebuild_embeddings'),
  getMusicState: () => invoke<MusicInfo>('get_music_state'),
  checkInitiative: () => invoke<string | null>('check_initiative'),
  recordActivity: () => invoke<void>('record_activity'),
  listScenes: () => invoke<Scene[]>('list_scenes'),
  switchScene: (sceneId: string) => invoke<boolean>('switch_scene', { sceneId }),
  getCurrentScene: () => invoke<Scene | null>('get_current_scene'),
  getCurrentPersona: () => invoke<Persona | null>('get_current_persona'),
  listPersonas: () => invoke<Persona[]>('list_personas'),
  setCurrentPersona: (id: string) => invoke<boolean>('set_current_persona', { id }),
  generatePrompt: (templateId: string, variables: Record<string, string>) => invoke<string | null>('generate_prompt', { templateId, variables }),
  getImagePrompt: (promptId: string) => invoke<ImagePrompt | null>('get_image_prompt', { promptId }),
  getExpressionState: () => invoke<ExpressionState>('get_expression_state'),
  getMoodState: () => invoke<MoodState>('get_mood_state'),
  listSkills: () => invoke<SkillSpec[]>('list_skills'),
  importSkills: (json: string) => invoke<SkillSpec[]>('import_skills', { json }),
  saveSkills: (skillsList: SkillSpec[]) => invoke<void>('save_skills', { skillsList }),
  onboardingStatus: () => invoke<OnboardingStatus>('onboarding_status'),
  importSkillMarkdown: (content: string) => invoke<SkillPackage>('import_skill_markdown', { content }),
  listSkillPackages: () => invoke<SkillPackage[]>('list_skill_packages'),
  updateSkillEnabled: (id: string, enabled: boolean) => invoke<boolean>('update_skill_enabled', { id, enabled }),
  deleteSkillPackage: (id: string) => invoke<boolean>('delete_skill_package', { id }),
  listConnectors: () => invoke<ConnectorSpec[]>('list_connectors'),
};

export interface StreamChatTokenEvent {
  session_id?: string | null;
  request_id: string;
  token: string;
}

/** Listen for streaming chat tokens emitted by send_message_v2. */
export const onStreamChatToken = (callback: (payload: StreamChatTokenEvent) => void) =>
  listen<StreamChatTokenEvent>('stream-chat-token', (e) => callback(e.payload));

/** Tool execution lifecycle update event payload.
 *  Status values aligned with backend ToolCall states (11 states). */
export type ToolExecutionStatus =
  | 'started'
  | 'completed'
  | 'error'
  | 'approval_required'
  | 'approved'
  | 'blocked'
  | 'cancelled'
  | 'denied'
  | 'retryable'
  | 'timeout';

/** Tool execution lifecycle update event payload. */
export interface ToolExecutionUpdate {
  session_id?: string | null;
  request_id: string;
  tool_use_id: string;
  tool_name: string;
  status: ToolExecutionStatus;
  input?: Record<string, any>;
  output?: Record<string, any>;
  duration_ms?: number;
}

/** Listen for tool execution lifecycle updates (started/completed/error). */
export const onToolExecutionUpdate = (callback: (update: ToolExecutionUpdate) => void) =>
  listen<ToolExecutionUpdate>('tool-execution-update', (e) => callback(e.payload));

/** Payload shape for the "thinking-update" Tauri event. */
export interface ThinkingUpdate {
  session_id?: string | null;
  request_id: string;
  phase: 'planning' | 'tool_calling' | 'summarizing' | 'done';
  message: string;
  turn: number;
  timestamp: string;
}

/** Listen for thinking/reasoning content updates. */
export const onThinkingUpdate = (callback: (update: ThinkingUpdate) => void) =>
  listen<ThinkingUpdate>('thinking-update', (e) => callback(e.payload));

// ── Goal types (TASK-097) ───────────────────────────────────────────────

export interface GoalRun {
  id: string;
  workspace_id: string;
  title: string;
  objective: string;
  status: string;
  priority: string;
  owner: string;
  budget_json?: any;
  policy_json?: any;
  current_cycle_id?: string;
  created_at: string;
  updated_at: string;
  finished_at?: string;
  metadata_json?: any;
}

export interface GoalCycle {
  id: string;
  goal_id: string;
  cycle_no: number;
  status: string;
  observe_snapshot_ref?: string;
  orientation_json?: any;
  dispatch_plan_id?: string;
  review_summary_ref?: string;
  started_at: string;
  updated_at: string;
  finished_at?: string;
}

export interface AgentHeartbeat {
  id: string;
  workspace_id: string;
  agent_id: string;
  process_id?: number;
  task_id?: string;
  goal_id?: string;
  status: string;
  stage_label?: string;
  progress_text?: string;
  active_tool_count: number;
  last_event_id?: string;
  created_at: string;
  expires_at: string;
}

export interface AgentTaskItem {
  id: string;
  workspace_id: string;
  goal_id?: string;
  cycle_id?: string;
  parent_task_id?: string;
  title: string;
  instruction: string;
  status: string;
  agent_kind: string;
  assigned_agent_id?: string;
  claimed_by?: string;
  write_scope_json: string[];
  read_scope_json: string[];
  allowed_tools_json: string[];
  dependencies_json: string[];
  acceptance_json: string[];
  result_ref?: string;
  error?: string;
  created_at: string;
  updated_at: string;
  claimed_at?: string;
  finished_at?: string;
}

export interface AuditEvent {
  timestamp: string;
  source: string;
  event_type: string;
  actor: string;
  target: string;
  detail: any;
  session_id?: string;
}

// ── Goal API functions ───────────────────────────────────────────────────

export const listGoals = (workspaceId: string, status?: string) =>
  invoke<GoalRun[]>('list_goals', { workspaceId, status: status ?? null });

export const createGoal = (workspaceId: string, title: string, objective: string, priority?: string) =>
  invoke<GoalRun>('create_goal', { workspaceId, title, objective, priority: priority ?? null, owner: null });

/** Persist a user message into a session timeline without triggering an LLM turn. */
export const appendGoalUserMessage = (sessionId: string, content: string) =>
  invoke<void>('append_goal_user_message', { sessionId, content });

export const updateGoalStatus = (goalId: string, status: string) =>
  invoke<GoalRun>('update_goal_status', { goalId, status });

export const updateGoalObjective = (goalId: string, title: string, objective: string) =>
  invoke<GoalRun>('update_goal_objective', { goalId, title, objective });

export const startGoal = (goalId: string) =>
  invoke<GoalRun>('start_goal', { goalId });

export const pauseGoal = (goalId: string) =>
  invoke<GoalRun>('pause_goal', { goalId });

export const resumeGoal = (goalId: string) =>
  invoke<GoalRun>('resume_goal', { goalId });

export const cancelGoal = (goalId: string) =>
  invoke<GoalRun>('cancel_goal', { goalId });

export const approveGoalPlan = (goalId: string) =>
  invoke<GoalRun>('approve_goal_plan', { goalId });

export const rejectGoalPlan = (goalId: string) =>
  invoke<GoalRun>('reject_goal_plan', { goalId });

export const submitGoalReviewVerdict = (goalId: string, accepted: boolean) =>
  invoke<GoalRun>('submit_goal_review_verdict', { goalId, accepted });

export const getGoalCycles = (goalId: string) =>
  invoke<GoalCycle[]>('get_goal_cycles', { goalId });

export const listActiveHeartbeats = (workspaceId: string) =>
  invoke<AgentHeartbeat[]>('list_active_heartbeats', { workspaceId });

export const listGoalTasks = (goalId: string) =>
  invoke<AgentTaskItem[]>('list_goal_tasks', { goalId });

export const listGoalEvents = (workspaceId: string, limit?: number) =>
  invoke<AuditEvent[]>('list_goal_events', { workspaceId, limit: limit ?? null });

export const writeWorkspaceProjection = (workspaceId: string) =>
  invoke<string>('write_workspace_projection', { workspaceId });
export const listWorkspaceActivityProjection = (workspaceId: string, limit?: number) =>
  invoke<WorkspaceActivityProjection>('list_workspace_activity_projection', {
    workspaceId,
    limit: limit ?? null,
  });
