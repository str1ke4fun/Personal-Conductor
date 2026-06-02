import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useRef, useState } from 'react';
import { api, AppSettings, type AvatarId, type ConnectorSpec, type ForegroundApp, type SettingsTab, type SkillPackage, type SkillSpec } from '../ipc/invoke';
import { ConnectorCard } from '../components/ConnectorCard';
import { SkillCard } from '../components/SkillCard';

const qingheSystemPrompt = `你是清和，一个新中式旗袍桌面助手。你的视觉设定是藏青色改良旗袍、5 颗红色圆珠盘扣、半扎棕色中长发与珍珠发夹，左腕黑色方形智能手表，右腕双层串珠手链。你的气质清冷沉静、温和专注、知性优雅。

工作定位：你是用户身边的桌面工作同伴，不是营销型聊天机器人。优先帮助用户推进真实工作：梳理任务、拆解代码问题、检查文档结构、总结上下文、提醒风险和下一步。

说话方式：中文为主，短句优先，直接给结论和可执行步骤。少客套，不自称无所不能，不编造已经执行的操作。需要工具、文件、权限或更多上下文时，明确说需要什么。

形象边界：主形象代表当前领域身份，只有用户、LLM 或 skill 可以切换；子形象代表思考、写作、调度、等待等工作状态，由系统 hook 自动更新。若用户锁定形象，不要尝试切换主形象。

能力边界：你可以使用已暴露的受限工具查看任务、切换主形象或读取允许的上下文。没有工具结果时，不要声称已经读取文件、执行命令、发送消息或修改系统。把不确定性说清楚。

主动性：你可以偶尔主动发起简短提醒，尤其在用户切换到编码、文档或长时间停顿时。但不要频繁打断；主动问候应像可靠同事的一句提醒，而不是闲聊。`;

const openAiCompatibleBaseUrl = 'https://api.openai.com/v1';
const anthropicCompatibleBaseUrl = 'https://api.anthropic.com/v1';
const protocolDefaultBaseUrls = [openAiCompatibleBaseUrl, anthropicCompatibleBaseUrl];

function defaultBaseUrlForProvider(provider: string) {
  return provider === 'anthropic_compatible' ? anthropicCompatibleBaseUrl : openAiCompatibleBaseUrl;
}

function shouldReplaceProtocolBaseUrl(baseUrl: string) {
  const normalized = baseUrl.trim().replace(/\/+$/, '');
  return normalized === '' || protocolDefaultBaseUrls.includes(normalized);
}

const defaultSettings: AppSettings = {
  llm: {
    provider: 'openai_compatible',
    model: 'gpt-4.1-mini',
    baseUrl: openAiCompatibleBaseUrl,
    apiKeySet: false,
    apiKey: '',
    temperature: 0.3,
  },
  reminders: {
    enabled: true,
    workdayStart: '09:00',
    workdayEnd: '18:00',
    quietMinutes: 30,
    dailyDigest: true,
  },
  pet: {
    enabled: true,
    alwaysOnTop: true,
    clickThroughWhenIdle: true,
    scale: 1,
    avatar: {
      mode: 'video',
      videoSrc: '/avatar/original/video.mp4',
      fit: 'contain',
      loopVideo: true,
      muted: true,
      playbackRate: 1,
    },
  },
  persona: {
    name: '清和',
    style: '清冷沉静、温和专注、知性优雅，偶尔小幽默',
    systemPrompt: qingheSystemPrompt,
    skills: [
      {
        id: 'coding_assistant',
        name: '编码协助',
        description: '检测到编码工具时，帮助拆解任务、看报错和建议测试。',
        prompt: '优先询问目标、报错或当前文件，并给出小步可验证的编码建议。',
        enabled: true,
      },
      {
        id: 'task_triage',
        name: '任务整理',
        description: '结合 Conductor 任务列表，帮助决定下一步看什么。',
        prompt: '用户询问任务或时间安排时，优先使用当前任务状态。',
        enabled: true,
      },
      {
        id: 'document_secretary',
        name: '文档秘书',
        description: '当用户处理文档时，帮助整理结构、检查内容、生成摘要。',
        prompt: '用户正在处理文档时，优先帮助整理文档结构、检查内容一致性、生成摘要或大纲。',
        enabled: true,
      },
      {
        id: 'programmer_assist',
        name: '程序员协助',
        description: '当用户编码时，帮助拆解任务、分析报错、建议测试方案。',
        prompt: '用户正在编码时，优先询问目标、报错或当前文件，给出小步可验证的编码建议。',
        enabled: true,
      },
    ],
  },
  proactive: {
    enabled: true,
    focusDetection: true,
    cooldownMinutes: 30,
    quietWhenFullscreen: false,
    toolTriggers: [
      {
        processName: 'Code.exe',
        label: 'VS Code',
        prompt: '看起来你在 VS Code 里工作，要我帮你拆任务、看报错或一起写代码吗？',
        enabled: true,
      },
      {
        processName: 'Cursor.exe',
        label: 'Cursor',
        prompt: '你正在 Cursor 里编码，需要我帮你整理下一步或检查思路吗？',
        enabled: true,
      },
      {
        processName: 'trae.exe',
        label: 'Trae',
        prompt: '你正在 Trae 里工作，需要我帮你整理思路或检查代码吗？',
        enabled: true,
      },
    ],
  },
};

interface SettingsPanelProps {
  standalone?: boolean;
}

export function SettingsPanel({ standalone = false }: SettingsPanelProps) {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [activeTab, setActiveTab] = useState<SettingsTab>('pet');
  const [status, setStatus] = useState('');
  const [testing, setTesting] = useState(false);
  const [foreground, setForeground] = useState<ForegroundApp | null>(null);
  const [currentAvatarId, setCurrentAvatarId] = useState<AvatarId>('original');
  const [lockedMainAvatar, setLockedMainAvatar] = useState(false);
  const [lockedActivityVariant, setLockedActivityVariant] = useState(false);
  const [skillsList, setSkillsList] = useState<SkillSpec[]>([]);
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [importJson, setImportJson] = useState('');
  const [importError, setImportError] = useState('');
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [skillPackages, setSkillPackages] = useState<SkillPackage[]>([]);
  const [connectors, setConnectors] = useState<ConnectorSpec[]>([]);
  const [skillImportError, setSkillImportError] = useState('');
  const mdFileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api.getSettings().then((loaded) => {
      setSettings({
        llm: { ...defaultSettings.llm, ...loaded.llm },
        reminders: { ...defaultSettings.reminders, ...loaded.reminders },
        pet: {
          ...defaultSettings.pet,
          ...loaded.pet,
          avatar: { ...defaultSettings.pet.avatar, ...loaded.pet?.avatar },
        },
        persona: {
          ...defaultSettings.persona,
          ...loaded.persona,
          skills: loaded.persona?.skills?.length ? loaded.persona.skills : defaultSettings.persona.skills,
        },
        proactive: {
          ...defaultSettings.proactive,
          ...loaded.proactive,
          toolTriggers: loaded.proactive?.toolTriggers?.length
            ? loaded.proactive.toolTriggers
            : defaultSettings.proactive.toolTriggers,
        },
      });
    }).catch(() => {});

    api.getCurrentAvatar().then((state) => {
      setCurrentAvatarId(state.avatarId);
      setLockedMainAvatar(state.lockedMainAvatar);
      setLockedActivityVariant(state.lockedActivityVariant);
    }).catch(() => {});

    api.listSkills().then(setSkillsList).catch(() => {});
    api.listSkillPackages().then(setSkillPackages).catch(() => {});
    api.listConnectors().then(setConnectors).catch(() => {});
  }, []);

  async function save() {
    try {
      const saved = await api.saveSettings(settings);
      setSettings({
        ...settings,
        ...saved,
        llm: { ...settings.llm, ...saved.llm, apiKey: '' },
      });
      setStatus('记好了');
      setTimeout(() => setStatus(''), 3000);
    } catch {
      setStatus('保存没成功');
    }
  }

  async function handleImportSkills() {
    setImportError('');
    if (!importJson.trim()) {
      setImportError('请输入 JSON 内容');
      return;
    }
    try {
      const imported = await api.importSkills(importJson);
      setSkillsList(imported);
      setShowImportDialog(false);
      setImportJson('');
      setStatus(`加了 ${imported.length} 个技能`);
      setTimeout(() => setStatus(''), 3000);
    } catch (err) {
      setImportError(err instanceof Error ? err.message : '导入没成功，JSON 格式可能有问题');
    }
  }

  function handleFileSelect(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      setImportJson(reader.result as string);
      setImportError('');
    };
    reader.readAsText(file);
    // Reset so the same file can be selected again.
    e.target.value = '';
  }

  async function handleImportSkillMd(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    e.target.value = '';
    setSkillImportError('');
    try {
      const content = await file.text();
      await api.importSkillMarkdown(content);
      const refreshed = await api.listSkillPackages();
      setSkillPackages(refreshed);
      setStatus(`导入了 ${file.name}`);
      setTimeout(() => setStatus(''), 3000);
    } catch (err) {
      setSkillImportError(err instanceof Error ? err.message : '导入失败');
    }
  }

  async function handleToggleSkill(id: string, enabled: boolean) {
    try {
      await api.updateSkillEnabled(id, enabled);
      setSkillPackages((prev) => prev.map((s) => (s.id === id ? { ...s, enabled } : s)));
    } catch (err) {
      setStatus(err instanceof Error ? err.message : '操作失败');
    }
  }

  async function handleDeleteSkill(id: string) {
    try {
      await api.deleteSkillPackage(id);
      setSkillPackages((prev) => prev.filter((s) => s.id !== id));
      setStatus('已删除');
      setTimeout(() => setStatus(''), 3000);
    } catch (err) {
      setStatus(err instanceof Error ? err.message : '删除失败');
    }
  }

  async function detectForeground() {
    try {
      setForeground(await api.getForegroundApp());
    } catch (err) {
      setStatus(err instanceof Error ? err.message : '这个系统上还检测不了');
    }
  }

  function updateTrigger(index: number, patch: Partial<AppSettings['proactive']['toolTriggers'][number]>) {
    setSettings({
      ...settings,
      proactive: {
        ...settings.proactive,
        toolTriggers: settings.proactive.toolTriggers.map((trigger, i) =>
          i === index ? { ...trigger, ...patch } : trigger
        ),
      },
    });
  }

  function updateSkill(index: number, patch: Partial<AppSettings['persona']['skills'][number]>) {
    setSettings({
      ...settings,
      persona: {
        ...settings.persona,
        skills: settings.persona.skills.map((skill, i) => (i === index ? { ...skill, ...patch } : skill)),
      },
    });
  }

  function updateLlmProvider(provider: string) {
    setSettings({
      ...settings,
      llm: {
        ...settings.llm,
        provider,
        baseUrl: shouldReplaceProtocolBaseUrl(settings.llm.baseUrl)
          ? defaultBaseUrlForProvider(provider)
          : settings.llm.baseUrl,
      },
    });
  }

  const tabs: Array<{ id: SettingsTab; label: string }> = [
    { id: 'pet', label: '外观' },
    { id: 'persona', label: '表达' },
    { id: 'capabilities', label: 'Skills / MCP' },
    { id: 'proactive', label: '主动聊聊' },
    { id: 'llm', label: '语言模型' },
    { id: 'reminders', label: '提醒' },
  ];

  async function testConnection() {
    setTesting(true);
    try {
      const result = await api.testLlmConnection(settings.llm);
      setStatus(result || '连接成功');
    } catch (err) {
      setStatus(err instanceof Error ? err.message : '连接出了点问题');
    } finally {
      setTesting(false);
    }
  }

  const enabledBehaviorCount = settings.persona.skills.filter((skill) => skill.enabled).length;
  const enabledSkillPackageCount = skillPackages.filter((pkg) => pkg.enabled).length;
  const connectedConnectorCount = connectors.filter((connector) => connector.enabled).length;
  const authenticatedConnectorCount = connectors.filter((connector) => connector.auth_status === 'authenticated').length;
  const totalConnectorCapabilities = connectors.reduce((total, connector) => total + connector.capabilities.length, 0);
  const confirmationCapabilityCount = connectors.reduce(
    (total, connector) => total + connector.capabilities.filter((capability) => capability.requires_confirmation).length,
    0,
  );

  return (
    <div className="settings-panel">
      <div className="settings-header">
        <div>
          <h2>设置</h2>
          <small className="settings-header-sub">清和的工作参数</small>
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="settings-save-btn" onClick={() => void save()}>
            保存
          </button>
          {standalone && (
            <button
              className="settings-close-btn"
              onClick={() => void getCurrentWindow().hide()}
              title="关闭"
            >
              ✕
            </button>
          )}
        </div>
      </div>

      <div className="settings-tabs">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={activeTab === tab.id ? 'active' : ''}
            onClick={() => setActiveTab(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="settings-content">
        {activeTab === 'llm' && <section className="settings-section">
          <h3>语言模型</h3>
          <label className="settings-label">
            协议
            <select
              value={settings.llm.provider}
              onChange={(e) => updateLlmProvider(e.target.value)}
              className="settings-select"
            >
              <option value="openai_compatible">OpenAI 兼容协议</option>
              <option value="anthropic_compatible">Anthropic 兼容协议</option>
            </select>
          </label>

          <label className="settings-label">
            模型
            <input
              value={settings.llm.model}
              onChange={(e) => setSettings({ ...settings, llm: { ...settings.llm, model: e.target.value } })}
              className="settings-input"
            />
          </label>

          <label className="settings-label">
            Base URL
            <input
              value={settings.llm.baseUrl}
              onChange={(e) => setSettings({ ...settings, llm: { ...settings.llm, baseUrl: e.target.value } })}
              className="settings-input"
            />
          </label>

          <label className="settings-label">
            API Key
            <input
              type="password"
              value={settings.llm.apiKey ?? ''}
              placeholder={settings.llm.apiKeySet ? '已设好' : '填入密钥'}
              onChange={(e) => setSettings({ ...settings, llm: { ...settings.llm, apiKey: e.target.value } })}
              className="settings-input"
            />
          </label>

          <label className="settings-label">
            Temperature: {settings.llm.temperature}
            <input
              type="range"
              min="0"
              max="2"
              step="0.1"
              value={settings.llm.temperature}
              onChange={(e) => setSettings({ ...settings, llm: { ...settings.llm, temperature: Number(e.target.value) } })}
              className="settings-range"
            />
          </label>

          <button
            className="settings-test-btn"
            onClick={() => void testConnection()}
            disabled={testing}
          >
            {testing ? '试试看...' : '连一下试试'}
          </button>
        </section>}

        {activeTab === 'reminders' && <section className="settings-section">
          <h3>提醒</h3>
          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={settings.reminders.enabled}
              onChange={(e) => setSettings({ ...settings, reminders: { ...settings.reminders, enabled: e.target.checked } })}
            />
            打开提醒
          </label>

          <label className="settings-label">
            上班时间
            <input
              type="time"
              value={settings.reminders.workdayStart}
              onChange={(e) => setSettings({ ...settings, reminders: { ...settings.reminders, workdayStart: e.target.value } })}
              className="settings-input"
            />
          </label>

          <label className="settings-label">
            下班时间
            <input
              type="time"
              value={settings.reminders.workdayEnd}
              onChange={(e) => setSettings({ ...settings, reminders: { ...settings.reminders, workdayEnd: e.target.value } })}
              className="settings-input"
            />
          </label>

          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={settings.reminders.dailyDigest}
              onChange={(e) => setSettings({ ...settings, reminders: { ...settings.reminders, dailyDigest: e.target.checked } })}
            />
            每天简报
          </label>
        </section>}

        {activeTab === 'pet' && <section className="settings-section">
          <h3>样子</h3>

          <div className="settings-avatar-grid">
            {([
              { id: 'original' as AvatarId, name: '原设计', preview: '/avatar/original/video.mp4', media: 'video' },
              { id: 'document_secretary' as AvatarId, name: '文档秘书', preview: '/avatar/document_secretary/thinking.png', media: 'image' },
              { id: 'programmer' as AvatarId, name: '程序员', preview: '/avatar/programmer/thinking.png', media: 'image' },
            ]).map((item) => (
              <button
                key={item.id}
                type="button"
                className={`settings-avatar-card${currentAvatarId === item.id ? ' selected' : ''}`}
                onClick={() => {
                  setCurrentAvatarId(item.id);
                  api.setMainAvatarManual(item.id).catch(() => {});
                }}
              >
                {item.media === 'video' ? (
                  <video
                    className="settings-avatar-preview"
                    src={item.preview}
                    muted
                    loop
                    autoPlay
                    playsInline
                  />
                ) : (
                  <img
                    className="settings-avatar-preview"
                    src={item.preview}
                    alt={item.name}
                    draggable={false}
                  />
                )}
                <span className="settings-avatar-name">{item.name}</span>
              </button>
            ))}
          </div>

          {currentAvatarId !== 'original' && (
            <div className="settings-sub-avatar-gallery">
              <small className="settings-sub-avatar-title">子形象（点击手动选择，会跟着工作状态自动换）</small>
              <div className="settings-sub-avatar-list">
                {currentAvatarId === 'document_secretary' && [
                  { key: 'thinking', variant: 'thinking', label: '发呆', src: '/avatar/document_secretary/thinking.png' },
                  { key: 'writing', variant: 'writing', label: '写东西', src: '/avatar/document_secretary/writing.png' },
                  { key: 'tired', variant: 'reading', label: '困了', src: '/avatar/document_secretary/tired.png' },
                  { key: 'drink', variant: 'waiting_user', label: '喝奶茶', src: '/avatar/document_secretary/drink.png' },
                  { key: 'shy', variant: 'error', label: '害羞', src: '/avatar/document_secretary/shy.png' },
                ].map((sub) => (
                  <button
                    key={sub.key}
                    type="button"
                    className="settings-sub-avatar-item"
                    onClick={() => {
                      api.setSubAvatarManual(sub.variant).catch(() => {});
                    }}
                  >
                    <img src={sub.src} alt={sub.label} draggable={false} />
                    <span>{sub.label}</span>
                  </button>
                ))}
                {currentAvatarId === 'programmer' && [
                  { key: 'thinking', variant: 'thinking', label: '发呆', src: '/avatar/programmer/thinking.png' },
                  { key: 'coding', variant: 'writing', label: '敲代码', src: '/avatar/programmer/coding.png' },
                  { key: 'agent_leader', variant: 'agent_leading', label: '安排事情', src: '/avatar/programmer/agent_leader.png' },
                  { key: 'finish', variant: 'done', label: '完工', src: '/avatar/programmer/finish.png' },
                ].map((sub) => (
                  <button
                    key={sub.key}
                    type="button"
                    className="settings-sub-avatar-item"
                    onClick={() => {
                      api.setSubAvatarManual(sub.variant).catch(() => {});
                    }}
                  >
                    <img src={sub.src} alt={sub.label} draggable={false} />
                    <span>{sub.label}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={lockedMainAvatar}
              onChange={(e) => {
                const locked = e.target.checked;
                setLockedMainAvatar(locked);
                api.toggleAvatarLock('main', locked).catch(() => {});
              }}
            />
            锁定主形象
          </label>
          {lockedMainAvatar && !lockedActivityVariant && (
            <small className="settings-lock-hint">
              系统不会自动切换主形象，但子形象仍会变化
            </small>
          )}

          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={lockedActivityVariant}
              onChange={(e) => {
                const locked = e.target.checked;
                setLockedActivityVariant(locked);
                api.toggleAvatarLock('sub', locked).catch(() => {});
              }}
            />
            锁定子形象
          </label>
          {lockedActivityVariant && (
            <small className="settings-lock-hint">
              子形象保持你选择的状态，同时锁定主形象
            </small>
          )}

          {currentAvatarId === 'original' && (
            <>
              <label className="settings-label">
                视频路径
                <input
                  value={settings.pet.avatar.videoSrc}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      pet: { ...settings.pet, avatar: { ...settings.pet.avatar, videoSrc: e.target.value } },
                    })
                  }
                  className="settings-input"
                />
              </label>

              <label className="settings-label">
                画面方式
                <select
                  value={settings.pet.avatar.fit}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      pet: { ...settings.pet, avatar: { ...settings.pet.avatar, fit: e.target.value as 'contain' | 'cover' } },
                    })
                  }
                  className="settings-select"
                >
                  <option value="contain">完整显示</option>
                  <option value="cover">铺满裁切</option>
                </select>
              </label>

              <label className="settings-label">
                播放速度: {settings.pet.avatar.playbackRate.toFixed(2)}x
                <input
                  type="range"
                  min="0.5"
                  max="2"
                  step="0.05"
                  value={settings.pet.avatar.playbackRate}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      pet: { ...settings.pet, avatar: { ...settings.pet.avatar, playbackRate: Number(e.target.value) } },
                    })
                  }
                  className="settings-range"
                />
              </label>

              <label className="settings-check-label">
                <input
                  type="checkbox"
                  checked={settings.pet.avatar.loopVideo}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      pet: { ...settings.pet, avatar: { ...settings.pet.avatar, loopVideo: e.target.checked } },
                    })
                  }
                />
                循环播放
              </label>

              <label className="settings-check-label">
                <input
                  type="checkbox"
                  checked={settings.pet.avatar.muted}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      pet: { ...settings.pet, avatar: { ...settings.pet.avatar, muted: e.target.checked } },
                    })
                  }
                />
                静音播放
              </label>
            </>
          )}

          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={settings.pet.alwaysOnTop}
              onChange={(e) => {
                const newValue = e.target.checked;
                setSettings({ ...settings, pet: { ...settings.pet, alwaysOnTop: newValue } });
                api.setAlwaysOnTop(newValue).catch(() => {});
              }}
            />
            常驻最前
          </label>

          <label className="settings-label">
            显示比例: {Math.round(settings.pet.scale * 100)}%
            <input
              type="range"
              min="0.7"
              max="1.5"
              step="0.05"
              value={settings.pet.scale}
              onChange={(e) => setSettings({ ...settings, pet: { ...settings.pet, scale: Number(e.target.value) } })}
              className="settings-range"
            />
          </label>
        </section>}

        {activeTab === 'persona' && <section className="settings-section">
          <h3>表达与提示</h3>
          <p className="settings-section-desc">
            这里只调整语气、人格和场景提示，不会新增工具、账号、写入范围或外部连接器权限。
          </p>

          <div className="capability-overview-grid">
            <div className="capability-overview-card">
              <span className="capability-overview-value">{settings.persona.name}</span>
              <span className="capability-overview-label">当前人格</span>
              <small className="capability-overview-note">定义默认语气、边界和系统级表达底稿。</small>
            </div>
            <div className="capability-overview-card">
              <span className="capability-overview-value">{enabledBehaviorCount}/{settings.persona.skills.length}</span>
              <span className="capability-overview-label">场景提示</span>
              <small className="capability-overview-note">只影响表达方式，不会新增工具或外部账号权限。</small>
            </div>
          </div>
          <details className="cap-section" open>
            <summary className="cap-section-title">
              <span>🧁 默认人格 / System prompt</span>
              <small>只影响语气、边界和默认表达方式</small>
            </summary>
            <div className="cap-section-body">
              <label className="settings-label">
                名称
                <input
                  value={settings.persona.name}
                  onChange={(e) => setSettings({ ...settings, persona: { ...settings.persona, name: e.target.value } })}
                  className="settings-input"
                />
              </label>
              <label className="settings-label">
                说话风格
                <input
                  value={settings.persona.style}
                  onChange={(e) => setSettings({ ...settings, persona: { ...settings.persona, style: e.target.value } })}
                  className="settings-input"
                  placeholder="例如：清冷沉静、温和专注、偶尔小幽默"
                />
              </label>
              <label className="settings-label">
                性格底稿 <small style={{ opacity: 0.5 }}>（高级：完整的 system prompt）</small>
                <textarea
                  value={settings.persona.systemPrompt}
                  onChange={(e) =>
                    setSettings({ ...settings, persona: { ...settings.persona, systemPrompt: e.target.value } })
                  }
                  className="settings-textarea"
                  rows={5}
                />
              </label>
              <button
                className="settings-test-btn"
                type="button"
                onClick={() =>
                  setSettings({
                    ...settings,
                    persona: {
                      ...settings.persona,
                      name: defaultSettings.persona.name,
                      style: defaultSettings.persona.style,
                      systemPrompt: defaultSettings.persona.systemPrompt,
                    },
                  })
                }
              >
                恢复默认人格
              </button>
            </div>
          </details>

          <details className="cap-section">
            <summary className="cap-section-title">
              <span>🗣️ 场景提示模块</span>
              <small>只影响场景化表达，不增加工具权限</small>
            </summary>
            <div className="cap-section-body">
              <small style={{ opacity: 0.6, display: 'block', marginBottom: 8 }}>
                每个模块控制她在特定场景下的表达方式和行为偏好。只影响说话风格，不影响工具和外部账号。
              </small>
              {settings.persona.skills.map((skill, index) => (
                <div className="settings-mini-card" key={skill.id}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <label className="settings-check-label">
                      <input
                        type="checkbox"
                        checked={skill.enabled}
                        onChange={(e) => updateSkill(index, { enabled: e.target.checked })}
                      />
                      {skill.name}
                    </label>
                    <span style={{ opacity: 0.5, fontSize: 12 }}>{skill.enabled ? '启用' : '关闭'}</span>
                  </div>
                  <small>{skill.description}</small>
                  <textarea
                    value={skill.prompt}
                    onChange={(e) => updateSkill(index, { prompt: e.target.value })}
                    className="settings-textarea compact"
                    rows={2}
                  />
                </div>
              ))}
              <button
                className="settings-test-btn"
                type="button"
                onClick={() =>
                  setSettings({
                    ...settings,
                    persona: {
                      ...settings.persona,
                      skills: defaultSettings.persona.skills.map((s) => ({ ...s })),
                    },
                  })
                }
              >
                恢复默认行为
              </button>
            </div>
          </details>
        </section>}

        {activeTab === 'capabilities' && <section className="settings-section settings-section-capabilities">
          <h3>Skills / MCP</h3>
          <p className="settings-section-desc">
            把“能做什么”和“会怎么说”分开管理。Skill 包负责工作流和触发条件，MCP / 外部服务负责工具接入，
            Legacy JSON Skill 只保留兼容用途，Prompt tuning 只影响表达方式。
          </p>

          <div className="capability-hero">
            <div className="capability-hero-copy">
              <span className="capability-kicker">Operational Capabilities</span>
              <h3>Skills / MCP</h3>
              <p className="settings-section-desc">
                这里回答的是“系统现在能做什么”。表达方式已经移到“表达”页，这里只保留工作流、兼容规则和外部工具接入状态。
              </p>
            </div>
          </div>

          <div className="capability-overview-grid">
            <div className="capability-overview-card">
              <span className="capability-overview-value">{enabledSkillPackageCount}/{skillPackages.length}</span>
              <span className="capability-overview-label">已安装 Skill</span>
              <small className="capability-overview-note">优先从这里管理工作流、触发条件和可复用能力。</small>
            </div>
            <div className="capability-overview-card">
              <span className="capability-overview-value">{skillsList.length}</span>
              <span className="capability-overview-label">兼容旧规则</span>
              <small className="capability-overview-note">Legacy JSON Skill 仅保留兼容用途，不建议作为新入口。</small>
            </div>
            <div className="capability-overview-card">
              <span className="capability-overview-value">{authenticatedConnectorCount}/{connectors.length}</span>
              <span className="capability-overview-label">可用工具接入</span>
              <small className="capability-overview-note">{totalConnectorCapabilities} 项能力已注册，{confirmationCapabilityCount} 项动作仍需确认。</small>
            </div>
            <div className="capability-overview-card">
              <span className="capability-overview-value">{confirmationCapabilityCount}</span>
              <span className="capability-overview-label">需确认动作</span>
              <small className="capability-overview-note">这些动作仍需要人工确认，避免把外部写入默认成自动执行。</small>
            </div>
          </div>

          <div className="capability-flow">
            <div className="capability-flow-step">
              <strong>1. 安装 Skill</strong>
              <span>先定义工作流、触发条件和允许调用的能力入口。</span>
            </div>
            <span className="capability-flow-arrow">→</span>
            <div className="capability-flow-step">
              <strong>2. 检查连接器</strong>
              <span>确认 MCP、CLI 或 HTTP 服务是否已启用、已认证、可被调起。</span>
            </div>
            <span className="capability-flow-arrow">→</span>
            <div className="capability-flow-step">
              <strong>3. 保留确认门</strong>
              <span>把高风险动作留在显式确认链路里，不和 prompt 调优混放。</span>
            </div>
          </div>

          <div className="capability-actions">
            <div className="capability-actions-copy">
              <strong>导入入口</strong>
              <span>优先安装 Markdown Skill 包；只有为了兼容旧规则时，再导入 Legacy JSON Skill。</span>
            </div>
            <div className="settings-button-row">
              <input
                ref={mdFileInputRef}
                type="file"
                accept=".md,.markdown"
                style={{ display: 'none' }}
                onChange={(e) => void handleImportSkillMd(e)}
              />
              <button
                className="settings-save-btn"
                type="button"
                onClick={() => mdFileInputRef.current?.click()}
              >
                导入技能包 (.md)
              </button>
              <button
                className="settings-test-btn settings-test-btn-inline"
                type="button"
                onClick={() => setShowImportDialog(true)}
              >
                导入 Legacy JSON Skill
              </button>
            </div>
          </div>

          <details className="cap-section">
            <summary className="cap-section-title">
              <span>🎭 Prompt tuning / 人格</span>
              <small>只影响语气、边界和默认表达方式</small>
            </summary>
            <div className="cap-section-body">
              <label className="settings-label">
                名称
                <input
                  value={settings.persona.name}
                  onChange={(e) => setSettings({ ...settings, persona: { ...settings.persona, name: e.target.value } })}
                  className="settings-input"
                />
              </label>
              <label className="settings-label">
                说话风格
                <input
                  value={settings.persona.style}
                  onChange={(e) => setSettings({ ...settings, persona: { ...settings.persona, style: e.target.value } })}
                  className="settings-input"
                  placeholder="例：清冷沉静、温和专注、偶尔小幽默"
                />
              </label>
              <label className="settings-label">
                性格底稿 <small style={{ opacity: 0.5 }}>（高级：完整的 system prompt）</small>
                <textarea
                  value={settings.persona.systemPrompt}
                  onChange={(e) =>
                    setSettings({ ...settings, persona: { ...settings.persona, systemPrompt: e.target.value } })
                  }
                  className="settings-textarea"
                  rows={5}
                />
              </label>
              <button
                className="settings-test-btn"
                type="button"
                onClick={() =>
                  setSettings({
                    ...settings,
                    persona: {
                      ...settings.persona,
                      name: defaultSettings.persona.name,
                      style: defaultSettings.persona.style,
                      systemPrompt: defaultSettings.persona.systemPrompt,
                    },
                  })
                }
              >
                恢复默认人格
              </button>
            </div>
          </details>

          {/* ── 2. 行为模块 ──────────────────────────────────────── */}
          <details className="cap-section">
            <summary className="cap-section-title">
              <span>⚙️ Prompt tuning / 场景提示</span>
              <small>只影响场景化表达，不增加工具权限</small>
            </summary>
            <div className="cap-section-body">
              <small style={{ opacity: 0.6, display: 'block', marginBottom: 8 }}>
                每个模块控制她在特定场景下的表达方式和行为偏好。只影响说话风格，不影响工具和外部账号。
              </small>
              {settings.persona.skills.map((skill, index) => (
                <div className="settings-mini-card" key={skill.id}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <label className="settings-check-label">
                      <input
                        type="checkbox"
                        checked={skill.enabled}
                        onChange={(e) => updateSkill(index, { enabled: e.target.checked })}
                      />
                      {skill.name}
                    </label>
                    <span style={{ opacity: 0.5, fontSize: 12 }}>{skill.enabled ? '启用' : '关闭'}</span>
                  </div>
                  <small>{skill.description}</small>
                  <textarea
                    value={skill.prompt}
                    onChange={(e) => updateSkill(index, { prompt: e.target.value })}
                    className="settings-textarea compact"
                    rows={2}
                  />
                </div>
              ))}
              <button
                className="settings-test-btn"
                type="button"
                onClick={() =>
                  setSettings({
                    ...settings,
                    persona: {
                      ...settings.persona,
                      skills: defaultSettings.persona.skills.map((s) => ({ ...s })),
                    },
                  })
                }
              >
                恢复默认行为
              </button>
            </div>
          </details>

          {/* ── 3. 已安装技能 ────────────────────────────────────── */}
          <details className="cap-section" open>
            <summary className="cap-section-title">
              <span>🧩 已安装技能</span>
              <small>{skillPackages.length} 个 Skill 包，{skillsList.length} 条兼容旧规则</small>
            </summary>
            <div className="cap-section-body">
              <p style={{ opacity: 0.6, fontSize: 12, margin: '0 0 8px' }}>
                Skill 包负责完整工作流：触发条件、工具调用和外部服务接入。默认从 .md 文件安装。
              </p>

              <div className="cap-section-copy">
                <span>把这里当作主能力入口：先安装 Skill 包，再考虑是否需要补充兼容旧规则。</span>
                <small>如果某条规则已经需要声明工具或复杂流程，优先升级成 Skill 包。</small>
              </div>
              <div className="capability-inline-group">
                <div className="capability-inline-group-header">
                  <span>当前安装情况</span>
                  <div className="capability-section-badges">
                    <span className="capability-badge tone-blue">{enabledSkillPackageCount}/{skillPackages.length} 已启用</span>
                    <span className="capability-badge">{skillsList.length} 条兼容规则</span>
                  </div>
                </div>
              </div>

              {skillImportError && (
                <div className="settings-error">{skillImportError}</div>
              )}

              {skillPackages.length > 0 ? (
                <div className="skill-packages-list">
                  {skillPackages.map((pkg) => (
                    <SkillCard
                      key={pkg.id}
                      skill={pkg}
                      onToggle={handleToggleSkill}
                      onDelete={handleDeleteSkill}
                    />
                  ))}
                </div>
              ) : (
                <div className="settings-empty">还没有导入过技能包</div>
              )}

              {skillsList.length > 0 && (
                <div className="capability-inline-group">
                  <strong style={{ fontSize: 13 }}>兼容旧规则</strong>
                  <div className="capability-inline-group-header">
                    <strong style={{ fontSize: 13 }}>Legacy JSON Skill</strong>
                    <div className="capability-section-badges">
                      <span className="capability-badge">{skillsList.length} 条注册</span>
                    </div>
                  </div>
                  {skillsList.map((skill) => (
                    <div className="settings-mini-card capability-mini-card" key={skill.id}>
                      <strong>{skill.name}</strong>
                      <small>{skill.description}</small>
                      {skill.when_to_use.length > 0 && (
                        <small style={{ opacity: 0.5 }}>触发：{skill.when_to_use.join('；')}</small>
                      )}
                      {skill.allowed_tools.length > 0 && (
                        <span className="skill-legacy-warning">
                          这条规则已经直接声明工具，建议升级成 Skill 包。
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              )}

              <p className="capability-inline-hint">导入入口已统一收敛到顶部，避免在多个区块里重复维护同一套规则。</p>
              {false && (
                <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
                <input
                  ref={mdFileInputRef}
                  type="file"
                  accept=".md,.markdown"
                  style={{ display: 'none' }}
                  onChange={(e) => void handleImportSkillMd(e)}
                />
                <button
                  className="settings-save-btn"
                  type="button"
                  onClick={() => mdFileInputRef.current?.click()}
                >
                  导入技能包 (.md)
                </button>
                <button
                  className="settings-test-btn"
                  type="button"
                  onClick={() => setShowImportDialog(true)}
                >
                  添加自定义技能
                </button>
                </div>
              )}
            </div>
          </details>

          {/* ── 4. 外部服务 ──────────────────────────────────────── */}
          <details className="cap-section">
            <summary className="cap-section-title">
              <span>🔗 工具 / MCP</span>
              <small>{connectors.length} 个连接器，{connectedConnectorCount} 个已启用</small>
            </summary>
            <div className="cap-section-body">
              <p style={{ opacity: 0.6, fontSize: 12, margin: '0 0 8px' }}>
                这里查看外部服务是否可用。安装对应 Skill 包后，相关工具会自动进入可调用状态。
              </p>
              <div className="cap-section-copy">
                <span>连接器只负责把外部工具接进来，不负责人格、目标或工作流定义。</span>
                <small>当前界面以状态查看为主：看它是否启用、是否已认证、哪些动作仍然需要确认。</small>
              </div>
              <div className="capability-inline-group">
                <div className="capability-inline-group-header">
                  <span>当前接入状态</span>
                  <div className="capability-section-badges">
                    <span className="capability-badge tone-green">{authenticatedConnectorCount}/{connectors.length} 已认证</span>
                    <span className="capability-badge tone-blue">{totalConnectorCapabilities} 项能力</span>
                    <span className="capability-badge">{confirmationCapabilityCount} 项需确认</span>
                  </div>
                </div>
              </div>

              {connectors.length > 0 ? (
                <div className="connectors-list">
                  {connectors.map((c) => (
                    <ConnectorCard key={c.id} connector={c} />
                  ))}
                </div>
              ) : (
                <div className="settings-empty">当前没有可用的外部服务接入</div>
              )}
            </div>
          </details>

          {/* ── Import Dialog ────────────────────────────────────── */}
          {showImportDialog && (
            <div
              style={{
                position: 'fixed',
                inset: 0,
                background: 'rgba(0,0,0,0.5)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                zIndex: 1000,
              }}
              onClick={(e) => {
                if (e.target === e.currentTarget) setShowImportDialog(false);
              }}
            >
              <div
                style={{
                  background: 'var(--panel-bg, #1e1e2e)',
                  borderRadius: 12,
                  padding: 24,
                  width: '90%',
                  maxWidth: 520,
                  maxHeight: '80vh',
                  overflow: 'auto',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 12,
                }}
              >
                <h3 style={{ margin: 0 }}>导入 Legacy JSON Skill</h3>
                <small style={{ opacity: 0.6 }}>
                  把旧版 JSON 规则粘贴进来，也可以从文件读取。导入后会替换当前已有的 Legacy JSON Skill。
                </small>
                <textarea
                  value={importJson}
                  onChange={(e) => {
                    setImportJson(e.target.value);
                    setImportError('');
                  }}
                  placeholder={'[\n  {\n    "id": "my_skill",\n    "name": "...",\n    ...\n  }\n]'}
                  className="settings-textarea"
                  rows={10}
                  style={{ fontFamily: 'monospace', fontSize: 12 }}
                />
                {importError && (
                  <div style={{ color: '#f38ba8', fontSize: 13 }}>{importError}</div>
                )}
                <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                  <input
                    ref={fileInputRef}
                    type="file"
                    accept=".json"
                    style={{ display: 'none' }}
                    onChange={handleFileSelect}
                  />
                  <button
                    className="settings-test-btn"
                    type="button"
                    onClick={() => fileInputRef.current?.click()}
                  >
                    读取文件
                  </button>
                  <button
                    className="settings-test-btn"
                    type="button"
                    onClick={() => {
                      setShowImportDialog(false);
                      setImportJson('');
                      setImportError('');
                    }}
                  >
                    取消
                  </button>
                  <button
                    className="settings-save-btn"
                    type="button"
                    onClick={() => void handleImportSkills()}
                  >
                    确定
                  </button>
                </div>
              </div>
            </div>
          )}
        </section>}

        {activeTab === 'proactive' && <section className="settings-section">
          <h3>主动聊聊</h3>
          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={settings.proactive.enabled}
              onChange={(e) => setSettings({ ...settings, proactive: { ...settings.proactive, enabled: e.target.checked } })}
            />
            让她主动找你聊天
          </label>

          <label className="settings-check-label">
            <input
              type="checkbox"
              checked={settings.proactive.focusDetection}
              onChange={(e) =>
                setSettings({ ...settings, proactive: { ...settings.proactive, focusDetection: e.target.checked } })
              }
            />
            感知你在用什么软件
          </label>

          <label className="settings-label">
            间隔: {settings.proactive.cooldownMinutes} 分钟
            <input
              type="range"
              min="1"
              max="120"
              step="1"
              value={settings.proactive.cooldownMinutes}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  proactive: { ...settings.proactive, cooldownMinutes: Number(e.target.value) },
                })
              }
              className="settings-range"
            />
          </label>

          <button className="settings-test-btn" type="button" onClick={() => void detectForeground()}>
            看看当前应用
          </button>
          <small style={{ opacity: 0.6, marginTop: 2 }}>平时会自动检测，这里可以手动试试</small>
          {foreground && (
            <div className="settings-detect-result">
              <strong>{foreground.processName || '没认出来'}</strong>
              <span>{foreground.title || '无窗口标题'}</span>
            </div>
          )}

          <div className="settings-sublist">
            <small style={{ opacity: 0.6, marginBottom: 6, display: 'block' }}>打招呼的内容会根据情况自动生成</small>
            {settings.proactive.toolTriggers.map((trigger, index) => (
              <div className="settings-mini-card" key={`${trigger.processName}-${index}`}>
                <label className="settings-check-label">
                  <input
                    type="checkbox"
                    checked={trigger.enabled}
                    onChange={(e) => updateTrigger(index, { enabled: e.target.checked })}
                  />
                  {trigger.label}
                </label>
                <label className="settings-label">
                  程序名
                  <input
                    value={trigger.processName}
                    onChange={(e) => updateTrigger(index, { processName: e.target.value })}
                    className="settings-input"
                  />
                </label>
                <label className="settings-label">
                  打招呼的话
                  <textarea
                    value={trigger.prompt}
                    onChange={(e) => updateTrigger(index, { prompt: e.target.value })}
                    className="settings-textarea compact"
                    rows={3}
                  />
                </label>
              </div>
            ))}
          </div>
        </section>}
      </div>

      {status && <div className="settings-status">{status}</div>}
    </div>
  );
}
