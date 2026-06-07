import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow, PhysicalSize } from '@tauri-apps/api/window';
import { register } from '@tauri-apps/plugin-global-shortcut';
import { useEffect, useRef, useState } from 'react';
import { api, type PetWindowState } from '../ipc/invoke';

export const DEFAULT_PET_STATE: PetWindowState = {
  x: null,
  y: null,
  width: 320,
  height: 420,
  scale: 1,
  locked: false,
};

export const SCALES = [0.75, 1, 1.25, 1.5];

export async function openPanelWindow(panel: string) {
  const label = panel;
  const title = panel === 'workbench' ? '工作台' : '偏好设置';
  const width = panel === 'workbench' ? 1100 : 450;
  const height = panel === 'workbench' ? 720 : 700;

  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    try {
      await existing.show();
      await existing.setFocus();
      return;
    } catch {
      try { await existing.destroy(); } catch { /* already gone */ }
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
    await new Promise<void>((resolve) => setTimeout(resolve, 150));
    const retry = await WebviewWindow.getByLabel(label);
    if (retry) {
      await retry.show().catch(() => {});
      await retry.setFocus().catch(() => {});
    }
  }
}

export function usePetWindowController() {
  const [windowState, setWindowState] = useState<PetWindowState>(DEFAULT_PET_STATE);
  const [menuOpen, setMenuOpen] = useState(false);
  const [quietEndTime, setQuietEndTime] = useState<number | null>(null);
  const saveTimer = useRef<number>();
  const windowStateRef = useRef<PetWindowState>(DEFAULT_PET_STATE);
  const menuRef = useRef<HTMLElement>(null);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    windowStateRef.current = windowState;
  }, [windowState]);

  // Close menu on click outside
  useEffect(() => {
    if (!menuOpen) return;
    function handleClickOutside(event: globalThis.MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [menuOpen]);

  // Close menu on ESC key
  useEffect(() => {
    if (!menuOpen) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') setMenuOpen(false);
    }
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [menuOpen]);

  // Register global shortcuts
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

  // Load saved window state
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

  // Persist on move/resize
  useEffect(() => {
    const cleanups: Array<Promise<() => void>> = [
      appWindow.onMoved(() => scheduleSave()),
      appWindow.onResized(() => scheduleSave()),
    ];
    return () => {
      cleanups.forEach((c) => c.then((d) => d()).catch(() => {}));
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
    };
  }, []);

  // Quiet mode countdown
  useEffect(() => {
    if (quietEndTime === null) return;
    const interval = setInterval(() => {
      if (Date.now() > quietEndTime) setQuietEndTime(null);
    }, 1000);
    return () => clearInterval(interval);
  }, [quietEndTime]);

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

  async function drag(event: React.MouseEvent) {
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

  function getStatusTooltip(state: string): string {
    const STATE_LABELS: Record<string, string> = {
      idle: '在呢',
      working: '忙碌中',
      update: '有动静了',
      quiet: '安静一会儿',
      new_task: '新任务来了',
    };
    const base = STATE_LABELS[state] ?? state;
    if (state === 'quiet' && quietEndTime && quietEndTime > Date.now()) {
      const remaining = Math.ceil((quietEndTime - Date.now()) / 60000);
      return `${base} (还剩 ${remaining} 分钟)`;
    }
    return base;
  }

  return {
    windowState,
    menuOpen,
    setMenuOpen,
    menuRef,
    drag,
    setScale,
    setLocked,
    hidePet,
    closePet,
    startQuietMode,
    getStatusTooltip,
  };
}
