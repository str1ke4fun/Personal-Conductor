import { listen } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow, PhysicalSize } from '@tauri-apps/api/window';
import { register } from '@tauri-apps/plugin-global-shortcut';
import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { api, type PetWindowState } from '../ipc/invoke';
import type { PetState } from '../live2d/stateMap';
import { AvatarRenderer } from './AvatarRenderer';
import { NotificationBubble, type Notification } from './NotificationBubble';
import { ChatBubble } from './ChatBubble';
import { MoodIndicator } from './MoodIndicator';
import { AffectionBadge } from './AffectionBadge';
import { usePetVisualState } from './usePetVisualState';

const DEFAULT_PET_STATE: PetWindowState = {
  x: null,
  y: null,
  width: 320,
  height: 420,
  scale: 1,
  locked: false,
};

const SCALES = [0.75, 1, 1.25, 1.5];

const STATE_LABELS: Record<PetState, string> = {
  idle: '在呢',
  working: '忙碌中',
  update: '有动静了',
  quiet: '安静一会儿',
  new_task: '新任务来了',
};

export function PetWindow() {
  const visualState = usePetVisualState();
  const state = visualState.petState;
  const moodZone = visualState.moodZone;
  const [windowState, setWindowState] = useState<PetWindowState>(DEFAULT_PET_STATE);
  const [menuOpen, setMenuOpen] = useState(false);
  const [quietEndTime, setQuietEndTime] = useState<number | null>(null);
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [chatMessage, setChatMessage] = useState<string | null>(null);
  const [petInput, setPetInput] = useState('');
  const [petSending, setPetSending] = useState(false);
  const saveTimer = useRef<number>();
  const windowStateRef = useRef<PetWindowState>(DEFAULT_PET_STATE);
  const menuRef = useRef<HTMLElement>(null);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    windowStateRef.current = windowState;
  }, [windowState]);

  useEffect(() => {
    const unlisten = listen<string>('navigate_to', async (event) => {
      await openPanelWindow(event.payload as string);
    });
    return () => {
      unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<Notification>('notification', (event) => {
      setNotifications((prev) => [...prev, event.payload]);
    });
    return () => {
      unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<string | { content: string }>('chat_message', (event) => {
      setChatMessage(typeof event.payload === 'string' ? event.payload : event.payload.content);
    });
    return () => {
      unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ content: string; action?: string }>('pet_message', (event) => {
      setChatMessage(event.payload.content);
      if (event.payload.action === 'open_chat') {
        // 用户点气泡时再打开完整对话窗口，避免主动打断。
      }
    });
    return () => {
      unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  // Close menu on click outside
  useEffect(() => {
    if (!menuOpen) return;

    function handleClickOutside(event: globalThis.MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }

    // Use mousedown so the menu closes before other click handlers fire
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [menuOpen]);

  // Close menu on ESC key
  useEffect(() => {
    if (!menuOpen) return;

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setMenuOpen(false);
      }
    }

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [menuOpen]);

  function removeNotification(id: string) {
    setNotifications((prev) => prev.filter((n) => n.id !== id));
  }

  async function openPanelWindow(panel: string) {
    const label = panel;
    const title = panel === 'workbench' ? '工作台' : '偏好设置';
    const width = panel === 'workbench' ? 1100 : 450;
    const height = panel === 'workbench' ? 720 : 700;

    // Try to bring an existing window to front first.
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
      try {
        await existing.show();
        await existing.setFocus();
        return;
      } catch {
        // Stale reference — destroy and fall through to recreate.
        try { await existing.destroy(); } catch { /* already gone */ }
        // Give Tauri a tick to release the label before recreating.
        await new Promise<void>((resolve) => setTimeout(resolve, 80));
      }
    }

    try {
      const win = new WebviewWindow(label, {
        url: `${panel}.html`,
        title,
        width,
        height,
        alwaysOnTop: false,
      });
      win.once('tauri://created', () => {
        win.show().catch(() => {});
        win.setFocus().catch(() => {});
      });
    } catch {
      // Label still registered — wait for cleanup and retry once.
      await new Promise<void>((resolve) => setTimeout(resolve, 150));
      const retry = await WebviewWindow.getByLabel(label);
      if (retry) {
        await retry.show().catch(() => {});
        await retry.setFocus().catch(() => {});
      }
    }
  }

  useEffect(() => {
    const registerShortcuts = async () => {
      try {
        await register('Ctrl+Shift+W', async () => {
          await openPanelWindow('workbench');
        });

        await register('Ctrl+Shift+Q', async () => {
          await api.quietForMinutes(30);
        });
      } catch {
        // 静默失败：快捷键可能已被其他应用占用
      }
    };

    registerShortcuts();
  }, []);

  useEffect(() => {
    api
      .loadPetWindowState()
      .then((saved) => {
        const next = { ...DEFAULT_PET_STATE, ...saved };
        setWindowState(next);
        return api.setPetClickThrough(false);
      })
      .catch(() => api.setPetClickThrough(false));
  }, []);

  useEffect(() => {
    const cleanups: Array<Promise<() => void>> = [
      appWindow.onMoved(() => scheduleSave()),
      appWindow.onResized(() => scheduleSave()),
    ];
    return () => {
      cleanups.forEach((cleanup) => cleanup.then((dispose) => dispose()).catch(() => {}));
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
    };
  }, []);

  async function snapshotState(overrides: Partial<PetWindowState> = {}): Promise<PetWindowState> {
    const [position, size] = await Promise.all([appWindow.outerPosition(), appWindow.innerSize()]);
    return {
      ...windowStateRef.current,
      x: position.x,
      y: position.y,
      width: size.width,
      height: size.height,
      ...overrides,
    };
  }

  function persist(next: PetWindowState) {
    windowStateRef.current = next;
    setWindowState(next);
    api.savePetWindowState(next).catch(() => {});
  }

  function scheduleSave() {
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      snapshotState().then(persist).catch(() => {});
    }, 250);
  }

  async function drag(event: MouseEvent) {
    if (event.button !== 0 || windowState.locked || menuOpen) return;
    await appWindow.startDragging();
    scheduleSave();
  }

  async function setScale(scale: number) {
    const width = Math.round(DEFAULT_PET_STATE.width * scale);
    const height = Math.round(DEFAULT_PET_STATE.height * scale);
    await appWindow.setSize(new PhysicalSize(width, height));
    const next = await snapshotState({ width, height, scale });
    persist(next);
  }

  async function setLocked(locked: boolean) {
    await api.setPetClickThrough(false);
    const next = await snapshotState({ locked });
    persist(next);
    setMenuOpen(false);
  }

  async function hidePet() {
    const next = await snapshotState();
    persist(next);
    await appWindow.hide();
  }

  async function closePet() {
    const next = await snapshotState();
    persist(next);
    await appWindow.close();
  }

  async function startQuietMode() {
    await api.quietForMinutes(30);
    setQuietEndTime(Date.now() + 30 * 60 * 1000);
    setMenuOpen(false);
  }

  async function sendPetMessage() {
    const content = petInput.trim();
    if (!content || petSending) return;

    setPetInput('');
    setPetSending(true);
    setChatMessage(content);
    try {
      const chitchat = await api.ensureChatSession('闲聊');
      const reply = await api.sendChatMessageV2(content, chitchat.id);
      const bubbleText = reply.bubble_summary ?? reply.message.content;
      setChatMessage(bubbleText);
      appWindow.emit('pet_message', {
        id: reply.message.id,
        content: bubbleText,
        kind: 'assistant',
      }).catch(() => {});
      api.recordActivity().catch(() => {});
    } catch (err) {
      setChatMessage(err instanceof Error ? err.message : '发送没成功，看看配置有没有问题？');
    } finally {
      setPetSending(false);
    }
  }

  const getStatusTooltip = () => {
    const base = STATE_LABELS[state];
    if (state === 'quiet' && quietEndTime && quietEndTime > Date.now()) {
      const remaining = Math.ceil((quietEndTime - Date.now()) / 60000);
      return `${base} (还剩 ${remaining} 分钟)`;
    }
    return base;
  };

  useEffect(() => {
    if (state !== 'quiet' || !quietEndTime) return;

    const interval = setInterval(() => {
      if (Date.now() > quietEndTime) {
        setQuietEndTime(null);
        // Backend will emit the correct pet_state once QUIET_MODE_UNTIL expires
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [state, quietEndTime]);

  return (
    <main
      className={`pet-window pet-${state}${moodZone ? ` mood-${moodZone}` : ''}${windowState.locked ? ' pet-locked' : ''}`}
      onMouseDown={drag}
      onClick={() => { api.recordActivity().catch(() => {}); }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenuOpen((open) => !open);
      }}
      title={getStatusTooltip()}
    >
      <div className="pet-canvas-container">
        <AvatarRenderer visualState={visualState} />
        <div className="pet-expression-bar">
          <MoodIndicator moodZone={moodZone} />
          <AffectionBadge />
        </div>
        <div className="pet-status" />
        {state === 'update' && (
          <div className="pet-badge" onClick={() => void openPanelWindow('workbench')}>
            <span className="pet-badge-dot">●</span>
            <span className="pet-badge-text">来看看进展</span>
          </div>
        )}
        {notifications.map((notification) => (
          <NotificationBubble
            key={notification.id}
            notification={notification}
            onClose={removeNotification}
          />
        ))}
        {chatMessage && (
          <ChatBubble
            content={chatMessage}
            onClose={() => setChatMessage(null)}
            onOpenChat={() => void openPanelWindow('workbench')}
          />
        )}
        <form
          className="pet-inline-chat"
          onSubmit={(event) => {
            event.preventDefault();
            void sendPetMessage();
          }}
          onMouseDown={(event) => event.stopPropagation()}
        >
          <input
            value={petInput}
            onChange={(event) => setPetInput(event.target.value)}
            placeholder="说点什么..."
            disabled={petSending}
          />
          <button type="submit" disabled={petSending || !petInput.trim()} title="发送">
            发送
          </button>
        </form>
      </div>

      {menuOpen && (
        <section className="pet-menu" ref={menuRef} onMouseDown={(event) => event.stopPropagation()}>
          <button type="button" onClick={() => void setLocked(!windowState.locked)}>
            {windowState.locked ? '取消固定' : '固定住'}
          </button>
          <button type="button" onClick={() => void openPanelWindow('workbench')}>
            🖥️ 工作台
          </button>
          <button type="button" onClick={() => void openPanelWindow('settings')}>
            ⚙️ 偏好设置
          </button>
          <button type="button" onClick={() => void startQuietMode()}>
            安静半小时
          </button>
          <div className="pet-menu-scale" aria-label="显示大小">
            {SCALES.map((scale) => (
              <button
                key={scale}
                type="button"
                className={windowState.scale === scale ? 'active' : ''}
                onClick={() => void setScale(scale)}
              >
                {Math.round(scale * 100)}%
              </button>
            ))}
          </div>
          <button type="button" onClick={() => void hidePet()}>
            隐藏
          </button>
          <button type="button" onClick={() => void closePet()}>
            退出
          </button>
        </section>
      )}
    </main>
  );
}
