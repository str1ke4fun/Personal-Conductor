import { useEffect, useMemo, useState } from 'react';
import {
  THEME_TOKENS,
  type ThemeOverrides,
  type ThemeVariant,
  type ThemeTokenDef,
  loadThemeOverrides,
  saveThemeOverrides,
  resolvedTokenColor,
} from '../styles/customTheme';

/** Group tokens by their `group` field, preserving definition order. */
function groupTokens(): Array<{ group: string; tokens: ThemeTokenDef[] }> {
  const order: string[] = [];
  const map = new Map<string, ThemeTokenDef[]>();
  for (const t of THEME_TOKENS) {
    if (!map.has(t.group)) {
      map.set(t.group, []);
      order.push(t.group);
    }
    map.get(t.group)!.push(t);
  }
  return order.map((group) => ({ group, tokens: map.get(group)! }));
}

/** Normalize any stored color to a `#rrggbb` the native color input accepts. */
function toPickerHex(value: string): string {
  const v = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(v)) return v;
  if (/^#[0-9a-fA-F]{3}$/.test(v)) {
    return `#${v[1]}${v[1]}${v[2]}${v[2]}${v[3]}${v[3]}`;
  }
  if (/^#[0-9a-fA-F]{8}$/.test(v)) return v.slice(0, 7);
  return '#888888'; // functional colors (rgb/hsl) can't drive <input type=color>
}

export function ThemeSettings() {
  const [overrides, setOverrides] = useState<ThemeOverrides>(() => loadThemeOverrides());
  const [variant, setVariant] = useState<ThemeVariant>(() =>
    document.documentElement.classList.contains('dark') ? 'dark' : 'light',
  );
  const groups = useMemo(groupTokens, []);

  // Persist + apply live on every change so the whole app updates instantly.
  useEffect(() => {
    saveThemeOverrides(overrides);
  }, [overrides]);

  const customizedCount = useMemo(
    () => Object.keys(overrides.light).length + Object.keys(overrides.dark).length,
    [overrides],
  );

  const setToken = (token: ThemeTokenDef, color: string) => {
    setOverrides((prev) => ({
      ...prev,
      [variant]: { ...prev[variant], [token.var]: color },
    }));
  };

  const resetToken = (token: ThemeTokenDef) => {
    setOverrides((prev) => {
      const next = { ...prev[variant] };
      delete next[token.var];
      return { ...prev, [variant]: next };
    });
  };

  const resetVariant = () => {
    setOverrides((prev) => ({ ...prev, [variant]: {} }));
  };

  const resetAll = () => setOverrides({ light: {}, dark: {} });

  return (
    <section className="settings-section theme-settings">
      <h3>主题配色</h3>
      <p className="settings-section-desc">
        像 VSCode / 终端那样自定义背景与字体颜色。亮色和暗色主题分别保存，改动会立即应用到所有窗口。
      </p>

      <div className="theme-variant-switch">
        <button
          type="button"
          className={variant === 'light' ? 'active' : ''}
          onClick={() => setVariant('light')}
        >
          ☀️ 亮色
        </button>
        <button
          type="button"
          className={variant === 'dark' ? 'active' : ''}
          onClick={() => setVariant('dark')}
        >
          🌙 暗色
        </button>
        <span className="theme-variant-note">
          {customizedCount > 0 ? `已自定义 ${customizedCount} 项` : '全部为默认'}
        </span>
      </div>

      {groups.map(({ group, tokens }) => (
        <div className="theme-group" key={group}>
          <h4 className="theme-group-title">{group}</h4>
          <div className="theme-token-grid">
            {tokens.map((token) => {
              const current = resolvedTokenColor(overrides, variant, token);
              const isCustom = token.var in overrides[variant];
              return (
                <div className={`theme-token ${isCustom ? 'is-custom' : ''}`} key={token.var}>
                  <label className="theme-token-swatch" title={`--${token.var}`}>
                    <input
                      type="color"
                      value={toPickerHex(current)}
                      onChange={(e) => setToken(token, e.target.value)}
                    />
                    <span className="theme-swatch-preview" style={{ background: current }} />
                  </label>
                  <div className="theme-token-meta">
                    <span className="theme-token-label">{token.label}</span>
                    <input
                      className="theme-token-hex"
                      type="text"
                      value={current}
                      spellCheck={false}
                      onChange={(e) => setToken(token, e.target.value)}
                    />
                  </div>
                  {isCustom && (
                    <button
                      type="button"
                      className="theme-token-reset"
                      title="恢复默认"
                      onClick={() => resetToken(token)}
                    >
                      ↺
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      ))}

      <div className="theme-actions">
        <button type="button" className="settings-test-btn" onClick={resetVariant}>
          重置当前主题
        </button>
        <button type="button" className="settings-test-btn" onClick={resetAll}>
          全部恢复默认
        </button>
      </div>
    </section>
  );
}
