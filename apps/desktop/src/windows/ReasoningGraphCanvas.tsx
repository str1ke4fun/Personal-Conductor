import React, { useMemo, useRef, useState, useCallback, useEffect } from 'react';
import type { GoalGraphSnapshot } from '../ipc/invoke';
import {
  layoutReasoningGraph,
  GOAL_ROOT_ID,
  type GraphLayout,
  type LaidOutNode,
  type GraphNodeKind,
} from './graphLayout';

interface ReasoningGraphCanvasProps {
  snapshot: GoalGraphSnapshot;
}

const KIND_LABEL: Record<GraphNodeKind, string> = {
  goal: '目标',
  intent: '意图',
  fact: '事实',
  hint: '提示',
};

const INTENT_STATUS_LABEL: Record<string, string> = {
  open: '开放',
  proposed: '待派',
  queued: '排队',
  claimed: '认领',
  running: '执行中',
  review_ready: '待审',
  accepted: '已完成',
  completed: '已完成',
  failed: '失败',
  blocked: '阻塞',
};

function nodeClass(kind: GraphNodeKind): string {
  return `rgraph-node rgraph-node--${kind}`;
}

/** A curved-ish straight link; we offset control to fan parallel edges apart. */
function edgePath(a: LaidOutNode, b: LaidOutNode): string {
  const mx = (a.x + b.x) / 2;
  const my = (a.y + b.y) / 2;
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.sqrt(dx * dx + dy * dy) || 1;
  // gentle perpendicular bow so overlapping links stay legible
  const bow = Math.min(len * 0.12, 26);
  const cx = mx + (-dy / len) * bow;
  const cy = my + (dx / len) * bow;
  return `M ${a.x} ${a.y} Q ${cx} ${cy} ${b.x} ${b.y}`;
}

export const ReasoningGraphCanvas: React.FC<ReasoningGraphCanvasProps> = ({ snapshot }) => {
  // Recompute layout only when the graph identity actually changes, so the
  // 8s polling refresh doesn't reshuffle the constellation under the user.
  const layoutKey = useMemo(() => {
    const factIds = (snapshot.facts ?? []).map((f) => f.id).join(',');
    const intentIds = (snapshot.intents ?? []).map((i) => `${i.id}:${i.status}`).join(',');
    const hintIds = (snapshot.hints ?? []).map((h) => h.id).join(',');
    const edgeIds = (snapshot.edges ?? []).map((e) => e.id).join(',');
    return `${snapshot.goal_id}|${factIds}|${intentIds}|${hintIds}|${edgeIds}`;
  }, [snapshot]);

  const layout: GraphLayout = useMemo(
    () => layoutReasoningGraph(snapshot),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [layoutKey],
  );

  const nodeById = useMemo(() => {
    const m = new Map<string, LaidOutNode>();
    for (const n of layout.nodes) m.set(n.id, n);
    return m;
  }, [layout]);

  const [hovered, setHovered] = useState<string | null>(null);
  const [pinned, setPinned] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);

  const { width, height } = layout;

  // Pan + zoom are kept in a ref and written straight to the SVG viewBox so a
  // drag never triggers a React re-render of the (potentially dozens of) nodes.
  const svgRef = useRef<SVGSVGElement | null>(null);
  const viewRef = useRef({ x: 0, y: 0, k: 1 });
  const rafRef = useRef<number | null>(null);
  const dragRef = useRef<{ px: number; py: number; vx: number; vy: number } | null>(null);

  const applyView = useCallback(() => {
    rafRef.current = null;
    const svg = svgRef.current;
    if (!svg) return;
    const { x, y, k } = viewRef.current;
    const vbW = width / k;
    const vbH = height / k;
    const vbX = (width - vbW) / 2 - x;
    const vbY = (height - vbH) / 2 - y;
    svg.setAttribute('viewBox', `${vbX} ${vbY} ${vbW} ${vbH}`);
  }, [width, height]);

  const scheduleView = useCallback(() => {
    if (rafRef.current != null) return;
    rafRef.current = requestAnimationFrame(applyView);
  }, [applyView]);

  const active = pinned ?? hovered;
  const activeNeighbors = useMemo(() => {
    if (!active) return null;
    const set = layout.neighbors.get(active) ?? new Set<string>();
    return new Set([active, ...set]);
  }, [active, layout]);

  const focusNode = active ? nodeById.get(active) ?? null : null;

  const resetView = useCallback(() => {
    viewRef.current = { x: 0, y: 0, k: 1 };
    scheduleView();
  }, [scheduleView]);

  const zoomBy = useCallback((factor: number) => {
    const v = viewRef.current;
    viewRef.current = { ...v, k: Math.min(2.6, Math.max(0.5, v.k * factor)) };
    scheduleView();
  }, [scheduleView]);

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    zoomBy(e.deltaY < 0 ? 1.12 : 0.89);
  }, [zoomBy]);

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    (e.target as Element).setPointerCapture?.(e.pointerId);
    const v = viewRef.current;
    dragRef.current = { px: e.clientX, py: e.clientY, vx: v.x, vy: v.y };
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    const k = viewRef.current.k;
    const dx = (e.clientX - d.px) / k;
    const dy = (e.clientY - d.py) / k;
    viewRef.current = { ...viewRef.current, x: d.vx + dx, y: d.vy + dy };
    scheduleView();
  }, [scheduleView]);

  const onPointerUp = useCallback(() => {
    dragRef.current = null;
  }, []);

  // Initial viewBox + cleanup of any pending frame.
  useEffect(() => {
    applyView();
    return () => {
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    };
  }, [applyView]);

  const initialVbW = width;
  const initialVbH = height;
  const initialVbX = 0;
  const initialVbY = 0;

  if (layout.nodes.length <= 1 && layout.edges.length === 0) {
    return (
      <div className="rgraph-empty">
        <div className="rgraph-empty-orb" />
        <p>推理图谱尚未生成。</p>
        <span>Agent 开始观察与规划后，事实与意图会在这里连成网络。</span>
      </div>
    );
  }

  const counts = {
    intent: layout.nodes.filter((n) => n.kind === 'intent').length,
    fact: layout.nodes.filter((n) => n.kind === 'fact').length,
    hint: layout.nodes.filter((n) => n.kind === 'hint').length,
    edge: layout.edges.filter((e) => !e.id.startsWith('root:')).length,
  };

  const body = (
    <div className={`rgraph ${expanded ? 'rgraph--expanded' : ''}`}>
      <div className="rgraph-toolbar">
        <div className="rgraph-legend">
          <span className="rgraph-legend-item"><i className="rgraph-swatch rgraph-swatch--goal" />目标</span>
          <span className="rgraph-legend-item"><i className="rgraph-swatch rgraph-swatch--intent" />意图 {counts.intent}</span>
          <span className="rgraph-legend-item"><i className="rgraph-swatch rgraph-swatch--fact" />事实 {counts.fact}</span>
          {counts.hint > 0 && (
            <span className="rgraph-legend-item"><i className="rgraph-swatch rgraph-swatch--hint" />提示 {counts.hint}</span>
          )}
        </div>
        <div className="rgraph-controls">
          <button type="button" onClick={() => zoomBy(1.15)} title="放大">＋</button>
          <button type="button" onClick={() => zoomBy(0.87)} title="缩小">－</button>
          <button type="button" onClick={resetView} title="复位">⤢</button>
          <button type="button" onClick={() => setExpanded((x) => !x)} title={expanded ? '退出全屏' : '全屏'}>
            {expanded ? '✕' : '⛶'}
          </button>
        </div>
      </div>

      <div className="rgraph-stage">
        <svg
          ref={svgRef}
          className="rgraph-svg"
          viewBox={`${initialVbX} ${initialVbY} ${initialVbW} ${initialVbH}`}
          preserveAspectRatio="xMidYMid meet"
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerUp}
        >
          <defs>
            <radialGradient id="rgraph-goal-grad" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stopColor="var(--rg-goal)" stopOpacity="0.9" />
              <stop offset="100%" stopColor="var(--rg-goal)" stopOpacity="0.35" />
            </radialGradient>
          </defs>

          {/* Edges */}
          <g className="rgraph-edges">
            {layout.edges.map((edge) => {
              const a = nodeById.get(edge.source);
              const b = nodeById.get(edge.target);
              if (!a || !b) return null;
              const dim = activeNeighbors
                ? !(activeNeighbors.has(edge.source) && activeNeighbors.has(edge.target))
                : false;
              const lit = activeNeighbors
                ? activeNeighbors.has(edge.source) && activeNeighbors.has(edge.target)
                : false;
              return (
                <path
                  key={edge.id}
                  d={edgePath(a, b)}
                  className={`rgraph-edge ${dim ? 'rgraph-edge--dim' : ''} ${lit ? 'rgraph-edge--lit' : ''}`}
                  fill="none"
                />
              );
            })}
          </g>

          {/* Nodes */}
          <g className="rgraph-nodes">
            {layout.nodes.map((node) => {
              const isFocus = active === node.id;
              const dim = activeNeighbors ? !activeNeighbors.has(node.id) : false;
              const isRoot = node.id === GOAL_ROOT_ID;
              return (
                <g
                  key={node.id}
                  className={`${nodeClass(node.kind)} ${isFocus ? 'is-focus' : ''} ${dim ? 'is-dim' : ''}`}
                  transform={`translate(${node.x} ${node.y})`}
                  onMouseEnter={() => setHovered(node.id)}
                  onMouseLeave={() => setHovered(null)}
                  onClick={(e) => {
                    e.stopPropagation();
                    setPinned((p) => (p === node.id ? null : node.id));
                  }}
                  data-status={node.status ?? ''}
                >
                  <circle
                    className="rgraph-node-halo"
                    r={node.r + 6}
                  />
                  <circle
                    className="rgraph-node-core"
                    r={node.r}
                    fill={isRoot ? 'url(#rgraph-goal-grad)' : undefined}
                  />
                  {(node.kind === 'goal' || node.kind === 'intent' || isFocus || !dim) && (
                    <text
                      className="rgraph-node-label"
                      y={node.r + 13}
                      textAnchor="middle"
                    >
                      {node.label}
                    </text>
                  )}
                </g>
              );
            })}
          </g>
        </svg>

        {focusNode && (
          <aside className="rgraph-detail" onClick={(e) => e.stopPropagation()}>
            <div className="rgraph-detail-head">
              <span className={`rgraph-detail-kind rgraph-detail-kind--${focusNode.kind}`}>
                {KIND_LABEL[focusNode.kind]}
              </span>
              {focusNode.status && (
                <span className="rgraph-detail-status">
                  {INTENT_STATUS_LABEL[focusNode.status] ?? focusNode.status}
                </span>
              )}
              {pinned && (
                <button className="rgraph-detail-close" onClick={() => setPinned(null)} title="取消固定">×</button>
              )}
            </div>
            <div className="rgraph-detail-title">{focusNode.label}</div>
            {focusNode.sublabel && <div className="rgraph-detail-sub">{focusNode.sublabel}</div>}
            <div className="rgraph-detail-meta">
              {focusNode.degree} 条关联 · {pinned ? '已固定' : '悬停预览'}
            </div>
          </aside>
        )}
      </div>

      <div className="rgraph-foot">
        <span>{counts.edge} 条推理关系</span>
        <span className="rgraph-foot-hint">拖拽平移 · 滚轮缩放 · 点击节点固定</span>
      </div>
    </div>
  );

  if (expanded) {
    return (
      <div className="rgraph-overlay" onClick={() => setExpanded(false)}>
        <div className="rgraph-overlay-inner" onClick={(e) => e.stopPropagation()}>
          {body}
        </div>
      </div>
    );
  }

  return body;
};

export default ReasoningGraphCanvas;
