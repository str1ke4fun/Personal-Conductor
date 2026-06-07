import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { api } from '../ipc/invoke';
import { usePetVisualState } from './usePetVisualState';
import { useResolvedAvatarSrc, AvatarRenderer } from './AvatarRenderer';
import { usePetWindowController, openPanelWindow, SCALES } from './usePetWindowController';
import { PetInlineChat } from './PetInlineChat';
import { NotificationBubble, type Notification } from './NotificationBubble';
import { ChatBubble } from './ChatBubble';
import { MoodIndicator } from './MoodIndicator';
import { AffectionBadge } from './AffectionBadge';
import { PetScene } from './pet/PetScene';
import { MoodAura, MoodFace } from './pet/decorations';

export function PetWindow() {
  const visualState = usePetVisualState();
  const state = visualState.petState;
  const moodZone = visualState.moodZone;
  const resolvedSrc = useResolvedAvatarSrc(visualState);

  const {
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
  } = usePetWindowController();

  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [chatMessage, setChatMessage] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<string>('navigate_to', async (event) => {
      await openPanelWindow(event.payload as string);
    });
    return () => { unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  useEffect(() => {
    const unlisten = listen<Notification>('notification', (event) => {
      setNotifications((prev) => [...prev, event.payload]);
    });
    return () => { unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  useEffect(() => {
    const unlisten = listen<string | { content: string }>('chat_message', (event) => {
      setChatMessage(typeof event.payload === 'string' ? event.payload : event.payload.content);
    });
    return () => { unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ content: string; action?: string }>('pet_message', (event) => {
      setChatMessage(event.payload.content);
    });
    return () => { unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  function removeNotification(id: string) {
    setNotifications((prev) => prev.filter((n) => n.id !== id));
  }

  return (
    <main
      className={`pet-window pet-${state}${moodZone ? ` mood-${moodZone}` : ''}${windowState.locked ? ' pet-locked' : ''}`}
      onMouseDown={drag}
      onClick={() => { api.recordActivity().catch(() => {}); }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenuOpen((open) => !open);
      }}
      title={getStatusTooltip(state)}
    >
      <div className="pet-canvas-container">
        {resolvedSrc !== null ? (
          <PetScene
            imageUrl={resolvedSrc}
            mood={moodZone ?? 'idle'}
            sceneKind={undefined}
            decorations={<>
              <MoodAura mood={moodZone ?? 'idle'} />
              <MoodFace mood={moodZone ?? 'idle'} />
            </>}
          />
        ) : (
          <AvatarRenderer visualState={visualState} />
        )}

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

        <PetInlineChat onMessage={setChatMessage} />
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
