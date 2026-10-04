import { ANALYSIS_CHART_MAX_DEPTH } from '@/lib/models/analysis';
import type {
  AnalysisDirectoryNode,
  AnalysisRemainderSelection,
  AnalysisResult,
  DirectoryEntryInfo,
} from '@/lib/models/analysis';

export interface SunburstSector {
  key: string;
  name: string;
  path: string | null;
  bytes: number;
  fileCount: number;
  depth: number;
  branchPath: string | null;
  parentPath: string;
  parentBytes: number;
  colorIndex: number;
  entry: DirectoryEntryInfo | null;
  arc: string;
  labelTransform: string;
  labelLength: number;
  remainder?: AnalysisRemainderSelection;
}

interface LayoutNode {
  name: string;
  path: string | null;
  bytes: number;
  fileCount: number;
  entry: DirectoryEntryInfo | null;
  children: AnalysisDirectoryNode[];
  files: DirectoryEntryInfo[];
  isDirectory: boolean;
}

const TAU = Math.PI * 2;
const INNER_RADIUS = 66;
const OUTER_RADIUS = 232;

function point(radius: number, angle: number) {
  return `${(Math.sin(angle) * radius).toFixed(3)},${(-Math.cos(angle) * radius).toFixed(3)}`;
}

function arc(inner: number, outer: number, start: number, end: number): string {
  const gap = Math.min(0.006, (end - start) / 5);
  const from = start + gap;
  const to = Math.min(end - gap, from + TAU - 0.0001);
  const large = to - from > Math.PI ? 1 : 0;
  return `M${point(outer, from)}A${outer},${outer} 0 ${large} 1 ${point(outer, to)}L${point(inner, to)}A${inner},${inner} 0 ${large} 0 ${point(inner, from)}Z`;
}

/** Preserve byte proportions while letting directory branches end at their last visible level. */
export function layout(result: AnalysisResult, depthLimit: number): SunburstSector[] {
  if (!(result.totalBytes > 0)) return [];
  const levels = Math.max(2, Math.min(ANALYSIS_CHART_MAX_DEPTH, Math.round(depthLimit)));
  const ringWidth = (OUTER_RADIUS - INNER_RADIUS) / levels;
  const hierarchy = new Map((result.directoryHierarchy ?? []).map(node => [node.path, node]));
  const roots: LayoutNode[] = result.entries.map(entry => ({
    ...entry,
    entry,
    children: hierarchy.get(entry.path)?.children ?? [],
    files: hierarchy.get(entry.path)?.files ?? [],
  }));
  const sectors: SunburstSector[] = [];

  function partition(
    nodes: LayoutNode[],
    total: number,
    start: number,
    end: number,
    depth: number,
    branch: string | null,
    color: number,
    parentPath: string
  ) {
    const span = end - start;
    // A sector must remain large enough for hit testing. Keep omitted bytes in
    // the parent total rather than redistributing them to visible children.
    const visible = nodes
      .filter(node => node.bytes > 0 && (node.bytes / total) * span >= 0.015)
      .sort((left, right) => right.bytes - left.bytes || (left.path ?? '').localeCompare(right.path ?? ''));
    const nodeBytes = visible.reduce((sum, node) => sum + node.bytes, 0);
    const denominator = Math.max(total, nodeBytes);
    const remainder = Math.max(0, total - nodeBytes);
    const grouped: LayoutNode[] =
      remainder > 0 && depth === 1
        ? [
            ...visible,
            {
              name: '',
              path: null,
              bytes: remainder,
              fileCount: 0,
              entry: null,
              children: [],
              files: [],
              isDirectory: false,
            },
          ]
        : visible;
    let angle = start;
    grouped.forEach((node, index) => {
      const next = angle + (node.bytes / denominator) * span;
      const branchPath = depth === 1 ? node.path : branch;
      const colorIndex = depth === 1 ? index % 5 : color;
      const radius = INNER_RADIUS + (depth - 0.5) * ringWidth;
      const middle = (angle + next) / 2;
      const rotation = (middle * 180) / Math.PI + (middle > Math.PI / 2 && middle < (3 * Math.PI) / 2 ? 180 : 0);
      // Straight tangential labels must fit both the angular span and the outer
      // ring boundary; arc length alone lets long names cross adjacent rings.
      const outerLabelRadius = INNER_RADIUS + depth * ringWidth - 7;
      const tangentWidth = 2 * Math.sqrt(Math.max(0, outerLabelRadius ** 2 - radius ** 2));
      const angularWidth = 2 * (radius - 5) * Math.tan(Math.min((next - angle) / 2, Math.PI / 2 - 0.01));
      sectors.push({
        key: node.path ?? `${parentPath}:remainder:${depth}`,
        name: node.name,
        path: node.path,
        bytes: node.bytes,
        fileCount: node.fileCount,
        depth,
        branchPath,
        parentPath,
        parentBytes: total,
        colorIndex,
        entry: node.entry,
        remainder:
          node.path === null
            ? {
                parentPath,
                bytes: node.bytes,
                visiblePaths: visible.flatMap(child => (child.path ? [child.path] : [])),
              }
            : undefined,
        arc: arc(INNER_RADIUS + (depth - 1) * ringWidth + 1, INNER_RADIUS + depth * ringWidth - 1, angle, next),
        labelTransform: `translate(${point(radius, middle)}) rotate(${rotation})`,
        labelLength: Math.floor((Math.min(tangentWidth, angularWidth) - 12) / 6),
      });
      // Only directories can expand into another level. Unprojected bytes stay
      // in their parent sector instead of implying an additional remainder ring.
      if (node.path && node.isDirectory && (node.children.length > 0 || node.files.length > 0) && depth < levels) {
        partition(
          [
            ...node.children.map(child => ({ ...child, files: child.files ?? [], entry: null, isDirectory: true })),
            ...node.files.map(file => ({ ...file, entry: file, children: [], files: [] })),
          ],
          node.bytes,
          angle,
          next,
          depth + 1,
          branchPath,
          colorIndex,
          node.path
        );
      }
      angle = next;
    });
  }
  partition(roots, result.totalBytes, 0, TAU, 1, null, 0, result.root);
  return sectors;
}
