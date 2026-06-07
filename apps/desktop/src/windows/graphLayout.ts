import type { GoalGraphSnapshot } from '../ipc/invoke';

/**
 * Dependency-free, deterministic force-directed layout for the reasoning graph.
 *
 * We have no graph library in the bundle (see package.json), so this module
 * turns a GoalGraphSnapshot into positioned nodes + edges that a plain SVG can
 * render. The simulation is seeded purely from node order, so the same snapshot
 * always produces the same picture — important for a panel that polls every few
 * seconds and must not jitter on every refresh.
 */

export type GraphNodeKind = 'goal' | 'intent' | 'fact' | 'hint';

export interface LaidOutNode {
  id: string;
  kind: GraphNodeKind;
  label: string;
  sublabel?: string;
  status?: string;
  /** simulated position in layout space (roughly -0.5..1.5 of viewBox) */
  x: number;
  y: number;
  /** node radius in layout units */
  r: number;
  degree: number;
}

export interface LaidOutEdge {
  id: string;
  source: string;
  target: string;
  relation: string;
  /** true when both endpoints resolved to real nodes */
  resolved: boolean;
}

export interface GraphLayout {
  nodes: LaidOutNode[];
  edges: LaidOutEdge[];
  width: number;
  height: number;
  /** adjacency for hover-neighborhood highlighting */
  neighbors: Map<string, Set<string>>;
}

const WIDTH = 760;
const HEIGHT = 520;

const KIND_RADIUS: Record<GraphNodeKind, number> = {
  goal: 26,
  intent: 18,
  fact: 12,
  hint: 14,
};

function truncate(value: string, max: number): string {
  const v = value.trim();
  return v.length > max ? `${v.slice(0, max - 1)}…` : v;
}

/** Deterministic pseudo-random in [0,1) seeded by an integer. */
function seeded(n: number): number {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453;
  return x - Math.floor(x);
}

export const GOAL_ROOT_ID = '__goal_root__';

/**
 * Build a node/edge graph from the snapshot, then relax it with a short
 * force simulation (repulsion + spring + gravity) over a fixed iteration count.
 */
export function layoutReasoningGraph(snapshot: GoalGraphSnapshot): GraphLayout {
  const facts = snapshot.facts ?? [];
  const intents = snapshot.intents ?? [];
  const hints = snapshot.hints ?? [];
  const rawEdges = snapshot.edges ?? [];

  const nodes: LaidOutNode[] = [];
  const byId = new Map<string, LaidOutNode>();
  const register = (node: LaidOutNode) => {
    if (byId.has(node.id)) return;
    byId.set(node.id, node);
    nodes.push(node);
  };

  // Synthetic root so a disconnected graph still reads as "about one goal".
  register({
    id: GOAL_ROOT_ID,
    kind: 'goal',
    label: truncate(snapshot.goal_title || '目标', 22),
    sublabel: snapshot.objective ? truncate(snapshot.objective, 60) : undefined,
    x: WIDTH / 2,
    y: HEIGHT / 2,
    r: KIND_RADIUS.goal,
    degree: 0,
  });

  intents.forEach((intent, i) => {
    const id = intent.id || `intent-${i}`;
    register({
      id,
      kind: 'intent',
      label: truncate(intent.title || intent.id || `意图 ${i + 1}`, 26),
      sublabel: intent.instruction ? truncate(intent.instruction, 80) : undefined,
      status: intent.status,
      x: 0,
      y: 0,
      r: KIND_RADIUS.intent,
      degree: 0,
    });
  });

  facts.forEach((fact, i) => {
    const id = fact.id || fact.key || `fact-${i}`;
    register({
      id,
      kind: 'fact',
      label: truncate(fact.summary || fact.key || fact.id || `事实 ${i + 1}`, 30),
      sublabel: fact.category,
      x: 0,
      y: 0,
      r: KIND_RADIUS.fact,
      degree: 0,
    });
  });

  hints.forEach((hint, i) => {
    const id = hint.id || `hint-${i}`;
    register({
      id,
      kind: 'hint',
      label: truncate(hint.content || `提示 ${i + 1}`, 30),
      sublabel: hint.hint_kind,
      x: 0,
      y: 0,
      r: KIND_RADIUS.hint,
      degree: 0,
    });
  });

  const neighbors = new Map<string, Set<string>>();
  const link = (a: string, b: string) => {
    if (!neighbors.has(a)) neighbors.set(a, new Set());
    if (!neighbors.has(b)) neighbors.set(b, new Set());
    neighbors.get(a)!.add(b);
    neighbors.get(b)!.add(a);
  };

  const edges: LaidOutEdge[] = [];
  rawEdges.forEach((edge, i) => {
    const source = byId.has(edge.from) ? edge.from : null;
    const target = byId.has(edge.to) ? edge.to : null;
    if (source && target) {
      edges.push({
        id: edge.id || `edge-${i}`,
        source,
        target,
        relation: edge.relation || 'related',
        resolved: true,
      });
      link(source, target);
    }
  });

  // Anchor any node that has no explicit edge to the goal root so the graph is
  // a single connected constellation rather than scattered islands.
  for (const node of nodes) {
    if (node.id === GOAL_ROOT_ID) continue;
    const hasEdge = neighbors.get(node.id)?.size;
    if (!hasEdge) {
      edges.push({
        id: `root:${node.id}`,
        source: GOAL_ROOT_ID,
        target: node.id,
        relation: node.kind === 'intent' ? 'intent' : node.kind === 'hint' ? 'hint' : 'fact',
        resolved: true,
      });
      link(GOAL_ROOT_ID, node.id);
    }
  }

  for (const node of nodes) {
    node.degree = neighbors.get(node.id)?.size ?? 0;
  }

  // Seed positions on concentric rings by kind, deterministically scattered.
  const ringByKind: Record<GraphNodeKind, number> = {
    goal: 0,
    intent: 150,
    fact: 250,
    hint: 200,
  };
  let idx = 0;
  for (const node of nodes) {
    if (node.id === GOAL_ROOT_ID) {
      node.x = WIDTH / 2;
      node.y = HEIGHT / 2;
      continue;
    }
    const angle = seeded(idx + 1) * Math.PI * 2;
    const ring = ringByKind[node.kind] * (0.7 + seeded(idx + 7) * 0.6);
    node.x = WIDTH / 2 + Math.cos(angle) * ring;
    node.y = HEIGHT / 2 + Math.sin(angle) * ring;
    idx += 1;
  }

  simulate(nodes, edges, neighbors);

  // Normalize into the viewBox with padding.
  const pad = 36;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const n of nodes) {
    minX = Math.min(minX, n.x - n.r);
    minY = Math.min(minY, n.y - n.r);
    maxX = Math.max(maxX, n.x + n.r);
    maxY = Math.max(maxY, n.y + n.r);
  }
  const spanX = Math.max(1, maxX - minX);
  const spanY = Math.max(1, maxY - minY);
  const scale = Math.min((WIDTH - pad * 2) / spanX, (HEIGHT - pad * 2) / spanY, 1.4);
  const offX = (WIDTH - spanX * scale) / 2 - minX * scale;
  const offY = (HEIGHT - spanY * scale) / 2 - minY * scale;
  for (const n of nodes) {
    n.x = n.x * scale + offX;
    n.y = n.y * scale + offY;
  }

  return { nodes, edges, width: WIDTH, height: HEIGHT, neighbors };
}

function simulate(
  nodes: LaidOutNode[],
  edges: LaidOutEdge[],
  neighbors: Map<string, Set<string>>,
): void {
  const ITER = 160;
  const REPULSION = 9000;
  const SPRING = 0.035;
  const SPRING_LEN = 110;
  const GRAVITY = 0.012;
  const cx = WIDTH / 2;
  const cy = HEIGHT / 2;
  const pos = new Map(nodes.map((n) => [n.id, n]));

  for (let step = 0; step < ITER; step += 1) {
    const cooling = 1 - step / ITER;
    const disp = new Map<string, { dx: number; dy: number }>();
    for (const n of nodes) disp.set(n.id, { dx: 0, dy: 0 });

    // Repulsion (O(n²); node counts here are small — tens, not thousands).
    for (let i = 0; i < nodes.length; i += 1) {
      for (let j = i + 1; j < nodes.length; j += 1) {
        const a = nodes[i];
        const b = nodes[j];
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        let dist2 = dx * dx + dy * dy;
        if (dist2 < 0.01) {
          dx = seeded(i * 31 + j) - 0.5;
          dy = seeded(j * 17 + i) - 0.5;
          dist2 = 0.01;
        }
        const force = REPULSION / dist2;
        const dist = Math.sqrt(dist2);
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        const da = disp.get(a.id)!;
        const db = disp.get(b.id)!;
        da.dx += fx; da.dy += fy;
        db.dx -= fx; db.dy -= fy;
      }
    }

    // Springs along edges.
    for (const e of edges) {
      const a = pos.get(e.source)!;
      const b = pos.get(e.target)!;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
      const force = SPRING * (dist - SPRING_LEN);
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      const da = disp.get(a.id)!;
      const db = disp.get(b.id)!;
      da.dx += fx; da.dy += fy;
      db.dx -= fx; db.dy -= fy;
    }

    // Gravity toward center keeps things compact.
    for (const n of nodes) {
      const d = disp.get(n.id)!;
      d.dx += (cx - n.x) * GRAVITY;
      d.dy += (cy - n.y) * GRAVITY;
    }

    const maxStep = 28 * cooling + 1;
    for (const n of nodes) {
      if (n.id === GOAL_ROOT_ID) continue; // pin root to center-ish
      const d = disp.get(n.id)!;
      const len = Math.sqrt(d.dx * d.dx + d.dy * d.dy) || 1;
      const clamped = Math.min(len, maxStep);
      n.x += (d.dx / len) * clamped;
      n.y += (d.dy / len) * clamped;
    }
  }

  void neighbors;
}
