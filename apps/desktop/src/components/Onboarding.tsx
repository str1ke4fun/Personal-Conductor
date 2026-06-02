import { useEffect, useState, useCallback } from 'react';
import { api } from '../ipc/invoke';

interface Props {
  onDismiss: () => void;
}

const STEPS = [
  { id: 'greeting', title: '你好呀' },
  { id: 'api_key', title: '配置 API Key' },
  { id: 'features', title: '我能做什么' },
] as const;

export function Onboarding({ onDismiss }: Props) {
  const [step, setStep] = useState(0);
  const [apiKey, setApiKey] = useState('');
  const [saving, setSaving] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [llmConfigured, setLlmConfigured] = useState(false);

  useEffect(() => {
    api.onboardingStatus().then((status) => {
      // If LLM is already configured, skip the API key step
      if (status.completedSteps.includes('llm_config')) {
        setLlmConfigured(true);
      }
    }).catch(() => {});
  }, []);

  const handleSaveApiKey = useCallback(async () => {
    const trimmed = apiKey.trim();
    if (!trimmed) return;

    setSaving(true);
    setTestResult(null);
    try {
      const settings = await api.getSettings();
      settings.llm.apiKey = trimmed;
      await api.saveSettings(settings);
      setLlmConfigured(true);
      setTestResult('保存成功！');
      // Auto-advance after a short delay
      setTimeout(() => {
        setStep(2);
        setTestResult(null);
      }, 800);
    } catch (err) {
      setTestResult(`保存失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSaving(false);
    }
  }, [apiKey]);

  const renderStep = () => {
    switch (STEPS[step].id) {
      case 'greeting':
        return (
          <div className="onboarding-step">
            <h2>你好呀</h2>
            <p>欢迎使用 Personal Agent！</p>
            <p>
              我是你的 AI 助手，可以陪你聊天、帮你管理待办、记住重要事情。
            </p>
            <p>接下来花一分钟完成初始配置吧。</p>
            <div className="onboarding-actions">
              <button
                type="button"
                className="onboarding-btn primary"
                onClick={() => setStep(llmConfigured ? 2 : 1)}
              >
                开始
              </button>
            </div>
          </div>
        );

      case 'api_key':
        return (
          <div className="onboarding-step">
            <h2>配置 API Key</h2>
            <p>要和我聊天，需要先配置一个大模型的 API Key。</p>
            <p className="onboarding-hint">
              支持 OpenAI、Anthropic 等兼容接口。填入后可在「偏好设置」中修改。
            </p>
            <input
              type="password"
              className="onboarding-input"
              placeholder="sk-..."
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && apiKey.trim()) void handleSaveApiKey();
              }}
              disabled={saving}
              autoFocus
            />
            {testResult && (
              <p className={testResult.startsWith('保存成功') ? 'onboarding-success' : 'onboarding-error'}>
                {testResult}
              </p>
            )}
            <div className="onboarding-actions">
              <button
                type="button"
                className="onboarding-btn"
                onClick={() => setStep(2)}
              >
                先跳过
              </button>
              <button
                type="button"
                className="onboarding-btn primary"
                onClick={handleSaveApiKey}
                disabled={saving || !apiKey.trim()}
              >
                {saving ? '保存中...' : '保存并继续'}
              </button>
            </div>
          </div>
        );

      case 'features':
        return (
          <div className="onboarding-step">
            <h2>我能做什么</h2>
            <ul className="onboarding-features">
              <li>
                <strong>聊天对话</strong>
                <span>随时和我聊天，我有记忆能力，会记住你说过的事</span>
              </li>
              <li>
                <strong>待办管理</strong>
                <span>帮你创建和跟踪任务，还能把任务交给 Agent 团队执行</span>
              </li>
              <li>
                <strong>主动助手</strong>
                <span>我能感知你的工作状态，在合适的时候提供帮助</span>
              </li>
              <li>
                <strong>个性装扮</strong>
                <span>可以给我换形象、换场景、调整性格风格</span>
              </li>
            </ul>
            <div className="onboarding-actions">
              <button
                type="button"
                className="onboarding-btn primary"
                onClick={onDismiss}
              >
                开始使用
              </button>
            </div>
          </div>
        );
    }
  };

  return (
    <div className="onboarding-overlay" onClick={onDismiss}>
      <div className="onboarding-card" onClick={(e) => e.stopPropagation()}>
        <div className="onboarding-progress">
          {STEPS.map((s, i) => (
            <span key={s.id} className={`dot ${i === step ? 'active' : ''} ${i < step ? 'done' : ''}`} />
          ))}
        </div>
        {renderStep()}
        <button
          type="button"
          className="onboarding-skip"
          onClick={onDismiss}
          title="跳过引导，以后不会再次出现"
        >
          跳过引导
        </button>
      </div>
    </div>
  );
}
