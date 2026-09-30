import type { CanvasTool } from './free-canvas.ts';
// Common actions stay visible; editing and spatial settings are disclosed at their point of use.
export function canvasControls(tool: CanvasTool): string[] {
  return tool === 'pen' || tool === 'eraser'
    ? ['select', 'pen', 'eraser', 'undo', 'send']
    : ['select', 'pen', 'add'];
}
export function spatialControls(expanded: boolean, tool: CanvasTool): string[] {
  return expanded
    ? [
        'pin',
        'smaller',
        'larger',
        'nearer',
        'farther',
        'recenter',
        'team',
        'exit',
      ]
    : tool === 'pen' || tool === 'eraser'
      ? ['select', 'eraser', 'undo', 'send']
      : ['select', 'pen', 'voice', 'more'];
}

export function initialSpatialMode(
  mode: string | null,
  comparison: boolean,
): 'workspace' | 'focus' | 'orbit' {
  if (mode === 'workspace' || mode === 'focus' || mode === 'orbit') return mode;
  return comparison ? 'workspace' : 'orbit';
}
