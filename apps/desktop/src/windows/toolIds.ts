export function normalizeToolId(toolId: string): string {
  return toolId.replace(/__/g, '.');
}

export function isToolId(toolId: string, ...candidates: string[]): boolean {
  const normalized = normalizeToolId(toolId);
  return candidates.includes(toolId) || candidates.includes(normalized);
}
