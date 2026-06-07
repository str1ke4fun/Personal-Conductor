/**
 * Custom theme engine — VSCode/terminal-style color customization.
 *
 * Users can override a curated set of CSS custom properties (backgrounds, text,
 * accents, signal colors) for the light and dark themes independently. Overrides
 * are persisted in localStorage and applied by injecting a <style> element that
 * redefines the variables on :root (light) and .dark, so they win over app.css
 * without touching it. A `storage` listener keeps every Tauri window in sync.
 */

export type ThemeVariant = 'light' | 'dark';

export interface ThemeTokenDef {
  /** CSS custom property name, without the leading -- */
  var: string;
  label: string;
  group: string;
  /** built-in defaults per variant, used for the color picker baseline + reset */
  defaults: Record<ThemeVariant, string>;
}

/** Per-variant map of tokenVar -> hex color override. */
export type ThemeOverrides = Record<ThemeVariant, Record<string, string>>;

const STORAGE_KEY = 'conductor-custom-theme-v1';
const STYLE_EL_ID = 'conductor-custom-theme-style';

/**
 * The editable palette. Defaults mirror the values in app.css :root / .dark so
 * the pickers open on the real current color. Keep groups small and meaningful.
 */
export const THEME_TOKENS: ThemeTokenDef[] = [
  // ── Surfaces ──
  { var: 'bg-primary', label: '主背景', group: '背景', defaults: { light: '#ffffff', dark: '#0d1117' } },
  { var: 'bg-secondary', label: '次背景', group: '背景', defaults: { light: '#f5f5f5', dark: '#161b22' } },
  { var: 'bg-tertiary', label: '三级背景', group: '背景', defaults: { light: '#e8e8e8', dark: '#1c2128' } },
  { var: 'bg-code', label: '代码块背景', group: '背景', defaults: { light: '#f6f8fa', dark: '#161b22' } },
  { var: 'border-color', label: '边框/分隔线', group: '背景', defaults: { light: '#e5e7eb', dark: '#30363d' } },

  // ── Text ──
  { var: 'text-primary', label: '主文字', group: '文字', defaults: { light: '#1f2937', dark: '#e0e0e0' } },
  { var: 'text-secondary', label: '次文字', group: '文字', defaults: { light: '#6b7280', dark: '#8b949e' } },

  // ── Accents ──
  { var: 'accent-blue', label: '主强调（蓝）', group: '强调', defaults: { light: '#3b82f6', dark: '#2f81f7' } },
  { var: 'accent-green', label: '成功（绿）', group: '强调', defaults: { light: '#22c55e', dark: '#3fb950' } },
  { var: 'accent-orange', label: '警示（橙）', group: '强调', defaults: { light: '#f59e0b', dark: '#d29922' } },
  { var: 'danger', label: '危险（红）', group: '强调', defaults: { light: '#ef4444', dark: '#f85149' } },
  { var: 'user-bubble-bg', label: '用户气泡', group: '强调', defaults: { light: '#8b5cf6', dark: '#1f6feb' } },

  // ── Signal tones (panels + graph) ──
  { var: 'sig-running', label: '执行中', group: '状态信号', defaults: { light: '#16a34a', dark: '#c8ff6b' } },
  { var: 'sig-done', label: '完成', group: '状态信号', defaults: { light: '#0d9488', dark: '#5eead4' } },
  { var: 'sig-warn', label: '待处理', group: '状态信号', defaults: { light: '#d97706', dark: '#ffb454' } },
  { var: 'sig-error', label: '错误', group: '状态信号', defaults: { light: '#e11d48', dark: '#fb7185' } },
];

const TOKEN_VARS = new Set(THEME_TOKENS.map((t) => t.var));

function emptyOverrides(): ThemeOverrides {
  return { light: {}, dark: {} };
}

function sanitizeColor(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const v = value.trim();
  // Accept #rgb / #rrggbb / #rrggbbaa and a couple of functional forms.
  if (/^#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(v)) return v;
  if (/^(rgb|rgba|hsl|hsla)\([0-9.,%\s/]+\)$/.test(v)) return v;
  return null;
}

export function loadThemeOverrides(): ThemeOverrides {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return emptyOverrides();
    const parsed = JSON.parse(raw) as Partial<ThemeOverrides>;
    const result = emptyOverrides();
    (['light', 'dark'] as ThemeVariant[]).forEach((variant) => {
      const src = parsed?.[variant];
      if (src && typeof src === 'object') {
        for (const [key, value] of Object.entries(src)) {
          if (!TOKEN_VARS.has(key)) continue;
          const color = sanitizeColor(value);
          if (color) result[variant][key] = color;
        }
      }
    });
    return result;
  } catch {
    return emptyOverrides();
  }
}

export function saveThemeOverrides(overrides: ThemeOverrides): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
  } catch {
    // storage may be unavailable; applying still works for this session
  }
  applyThemeOverrides(overrides);
  // Notify same-window listeners (storage event only fires cross-document).
  window.dispatchEvent(new CustomEvent('conductor-theme-changed'));
}

function buildCss(overrides: ThemeOverrides): string {
  const block = (selector: string, vars: Record<string, string>) => {
    const lines = Object.entries(vars)
      .filter(([key]) => TOKEN_VARS.has(key))
      .map(([key, value]) => `  --${key}: ${value};`)
      .join('\n');
    return lines ? `${selector} {\n${lines}\n}` : '';
  };
  return [
    block(':root', overrides.light),
    block('.dark', overrides.dark),
  ]
    .filter(Boolean)
    .join('\n');
}

/** Inject (or update) the override <style> element. */
export function applyThemeOverrides(overrides: ThemeOverrides): void {
  if (typeof document === 'undefined') return;
  let styleEl = document.getElementById(STYLE_EL_ID) as HTMLStyleElement | null;
  const css = buildCss(overrides);
  if (!css) {
    if (styleEl) styleEl.textContent = '';
    return;
  }
  if (!styleEl) {
    styleEl = document.createElement('style');
    styleEl.id = STYLE_EL_ID;
    document.head.appendChild(styleEl);
  }
  styleEl.textContent = css;
}

/**
 * Call once at window startup: applies stored overrides and wires cross-window
 * sync. Returns a disposer.
 */
export function initCustomTheme(): () => void {
  applyThemeOverrides(loadThemeOverrides());
  const onStorage = (e: StorageEvent) => {
    if (e.key === STORAGE_KEY) applyThemeOverrides(loadThemeOverrides());
  };
  window.addEventListener('storage', onStorage);
  return () => window.removeEventListener('storage', onStorage);
}

export function resolvedTokenColor(
  overrides: ThemeOverrides,
  variant: ThemeVariant,
  token: ThemeTokenDef,
): string {
  return overrides[variant][token.var] ?? token.defaults[variant];
}
