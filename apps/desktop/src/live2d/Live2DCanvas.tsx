import * as PIXI from 'pixi.js';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useRef } from 'react';
import { STATE_TO_EXPR, STATE_TO_MOTION, type PetState } from './stateMap';

const CANVAS_WIDTH = 240;
const CANVAS_HEIGHT = 320;
const MODEL_URL = '/live2d/hiyori/hiyori_pro_en/runtime/hiyori_pro_t11.model3.json';
const CUBISM_CORE_URL = '/live2d/core/live2dcubismcore.min.js';
const MOTION_PRIORITY_NORMAL = 2;
const scriptPromises = new Map<string, Promise<void>>();

declare global {
  interface Window {
    Live2DCubismCore?: unknown;
  }
}

type Live2DModelType = {
  expression?: (id?: string | number) => Promise<boolean>;
  motion?: (group: string, index?: number, priority?: number) => Promise<boolean>;
  focus?: (x: number, y: number, instant?: boolean) => void;
  internalModel?: {
    focusController?: {
      focus: (x: number, y: number, instant?: boolean) => void;
    };
  };
  scale: PIXI.ObservablePoint;
  position: PIXI.ObservablePoint;
  getLocalBounds: () => PIXI.Rectangle;
  destroy: () => void;
};

type CursorPosition = {
  x: number;
  y: number;
};

type WindowMetrics = {
  x: number;
  y: number;
  scaleFactor: number;
};

type FollowTarget = {
  x: number;
  y: number;
};

function isPetState(value: unknown): value is PetState {
  return typeof value === 'string' && value in STATE_TO_MOTION;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

async function loadScript(src: string) {
  const existingPromise = scriptPromises.get(src);
  if (existingPromise) {
    await existingPromise;
    return;
  }

  const promise = new Promise<void>((resolve, reject) => {
    const script = document.createElement('script');
    script.src = src;
    script.async = true;
    script.onload = () => {
      script.dataset.loaded = 'true';
      resolve();
    };
    script.onerror = () => reject(new Error(`Failed to load script: ${src}`));
    document.head.appendChild(script);
  });
  scriptPromises.set(src, promise);
  await promise;
}

async function ensureCubismCore() {
  if (window.Live2DCubismCore) return;
  await loadScript(CUBISM_CORE_URL);
  if (!window.Live2DCubismCore) {
    throw new Error(`Cubism 4 runtime did not initialize from ${CUBISM_CORE_URL}`);
  }
}

export function Live2DCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const errorRef = useRef<HTMLDivElement>(null);
  const modelRef = useRef<Live2DModelType | null>(null);
  const appRef = useRef<PIXI.Application | null>(null);
  const currentStateRef = useRef<PetState>('idle');
  const windowMetricsRef = useRef<WindowMetrics>({ x: 0, y: 0, scaleFactor: 1 });
  const pendingCursorRef = useRef<CursorPosition | null>(null);
  const followTargetRef = useRef<FollowTarget>({ x: 0, y: 0 });
  const focusFrameRef = useRef(0);

  const applyState = (state: PetState) => {
    const model = modelRef.current;
    if (!model) return;
    const expression = STATE_TO_EXPR[state];
    const motion = STATE_TO_MOTION[state];
    if (expression && model.expression) {
      void model.expression(expression).catch((err) => console.error('Live2D expression failed:', err));
    }
    if (motion && model.motion) {
      void model
        .motion(motion.group, motion.index, MOTION_PRIORITY_NORMAL)
        .catch((err) => console.error('Live2D motion failed:', err));
    }
  };

  useEffect(() => {
    let destroyed = false;
    let unlistenState: (() => void) | undefined;
    let unlistenCursor: (() => void) | undefined;
    let unlistenMoved: (() => void) | undefined;
    let updateModel: (() => void) | undefined;
    const appWindow = getCurrentWindow();

    async function mount() {
      if (!canvasRef.current) return;
      if (errorRef.current) errorRef.current.textContent = '';

      await ensureCubismCore();
      const { Live2DModel } = await import('pixi-live2d-display/cubism4');

      const app = new PIXI.Application({
        view: canvasRef.current,
        backgroundAlpha: 0,
        clearBeforeRender: true,
        resolution: window.devicePixelRatio,
        autoDensity: true,
        width: CANVAS_WIDTH,
        height: CANVAS_HEIGHT,
      });
      appRef.current = app;
      app.ticker.maxFPS = 30;

      const model = await Live2DModel.from(MODEL_URL, {
        autoInteract: false,
        autoUpdate: false,
      });

      if (destroyed) {
        model.destroy();
        return;
      }

      const bounds = model.getLocalBounds();
      const modelWidth = bounds.width || CANVAS_WIDTH;
      const modelHeight = bounds.height || CANVAS_HEIGHT;
      const scale = Math.min((CANVAS_WIDTH - 16) / modelWidth, (CANVAS_HEIGHT - 16) / modelHeight);
      model.scale.set(scale);
      model.position.set(
        (CANVAS_WIDTH - modelWidth * scale) / 2 - bounds.x * scale,
        CANVAS_HEIGHT - 8 - (bounds.y + modelHeight) * scale
      );

      app.stage.addChild(model as unknown as PIXI.DisplayObject);
      modelRef.current = model;

      updateModel = () => {
        if (!destroyed) {
          model.update(app.ticker.deltaMS);
        }
      };
      app.ticker.add(updateModel, undefined, PIXI.UPDATE_PRIORITY.HIGH);

      applyState(currentStateRef.current);

      unlistenState = await listen<unknown>('pet_state', (event) => {
        if (!isPetState(event.payload)) return;
        currentStateRef.current = event.payload;
        applyState(event.payload);
      });

      unlistenCursor = await listen<CursorPosition>('cursor_position', (event) => {
        handleScreenCursorMove(event.payload);
      });
    }

    async function refreshWindowMetrics() {
      try {
        const [pos, scaleFactor] = await Promise.all([appWindow.outerPosition(), appWindow.scaleFactor()]);
        windowMetricsRef.current = { x: pos.x, y: pos.y, scaleFactor };
      } catch (err) {
        console.error('Failed to get window position:', err);
        windowMetricsRef.current = { x: 0, y: 0, scaleFactor: 1 };
      }
    }

    function focusCanvasPoint(x: number, y: number) {
      const target = canvasPointToFollowTarget(x, y);
      followTargetRef.current = target;
      const model = modelRef.current;
      const focusController = model?.internalModel?.focusController;
      if (focusController) {
        focusController.focus(target.x, target.y);
      } else if (model?.focus) {
        try {
          model.focus(x, y, true);
        } catch (err) {
          console.error('model.focus failed:', err);
        }
      }
    }

    function canvasPointToFollowTarget(x: number, y: number): FollowTarget {
      return {
        x: clamp((x / CANVAS_WIDTH) * 2 - 1, -1, 1),
        y: clamp(1 - (y / CANVAS_HEIGHT) * 2, -1, 1),
      };
    }

    function screenToCanvasPoint(position: CursorPosition) {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const { x, y, scaleFactor } = windowMetricsRef.current;
      const rect = canvas.getBoundingClientRect();
      const clientX = (position.x - x) / scaleFactor;
      const clientY = (position.y - y) / scaleFactor;

      return {
        x: ((clientX - rect.left) / Math.max(rect.width, 1)) * CANVAS_WIDTH,
        y: ((clientY - rect.top) / Math.max(rect.height, 1)) * CANVAS_HEIGHT,
      };
    }

    function flushPendingCursor() {
      focusFrameRef.current = 0;
      const position = pendingCursorRef.current;
      pendingCursorRef.current = null;
      if (!position || !modelRef.current) return;
      const point = screenToCanvasPoint(position);
      if (point) focusCanvasPoint(point.x, point.y);
    }

    function handleScreenCursorMove(position: CursorPosition) {
      pendingCursorRef.current = position;
      if (!focusFrameRef.current) {
        focusFrameRef.current = window.requestAnimationFrame(flushPendingCursor);
      }
    }

    function handleMouseMove(event: MouseEvent) {
      const canvas = canvasRef.current;
      if (!modelRef.current || !canvas) return;
      const rect = canvas.getBoundingClientRect();
      focusCanvasPoint(event.clientX - rect.left, event.clientY - rect.top);
    }

    mount().catch((error) => {
      console.error('Live2D failed to mount', error);
      if (errorRef.current) {
        errorRef.current.textContent = error instanceof Error ? error.message : String(error);
      }
    });
    refreshWindowMetrics();
    appWindow
      .onMoved((event) => {
        const current = windowMetricsRef.current;
        windowMetricsRef.current = { ...current, x: event.payload.x, y: event.payload.y };
      })
      .then((dispose) => {
        unlistenMoved = dispose;
      })
      .catch((err) => console.error('Failed to setup onMoved:', err));
    const unlistenScale = appWindow.onScaleChanged((event) => {
      const current = windowMetricsRef.current;
      windowMetricsRef.current = { ...current, scaleFactor: event.payload.scaleFactor };
    });
    window.addEventListener('resize', refreshWindowMetrics);
    window.addEventListener('mousemove', handleMouseMove);
    
    return () => {
      destroyed = true;
      window.removeEventListener('resize', refreshWindowMetrics);
      window.removeEventListener('mousemove', handleMouseMove);
      if (focusFrameRef.current) cancelAnimationFrame(focusFrameRef.current);
      unlistenState?.();
      unlistenCursor?.();
      unlistenMoved?.();
      if (updateModel) appRef.current?.ticker.remove(updateModel);
      unlistenScale.then((dispose) => dispose()).catch(() => {});
      modelRef.current = null;
      appRef.current?.destroy(false, { children: true, texture: false, baseTexture: false });
    };
  }, []);

  return (
    <>
      <canvas ref={canvasRef} className="live2d-canvas" />
      <div ref={errorRef} className="live2d-error" />
    </>
  );
}
