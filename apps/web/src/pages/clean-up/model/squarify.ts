/** A rectangle in the frame's own units. */
export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

interface Cell {
  /** The weight's index in the input. */
  readonly index: number;
  /** Its share of the frame's area. */
  readonly area: number;
}

function sum(cells: readonly Cell[]): number {
  return cells.reduce((total, cell) => total + cell.area, 0);
}

/** The worst aspect ratio in `row` if it is laid along a side of length `side`. */
function worst(row: readonly Cell[], side: number): number {
  const total = sum(row);
  const areas = row.map((cell) => cell.area);
  const largest = Math.max(...areas);
  const smallest = Math.min(...areas);
  return Math.max(
    (side * side * largest) / (total * total),
    (total * total) / (side * side * smallest),
  );
}

/**
 * Lays `row` along the shorter side of `rest`, writing each cell's rectangle
 * into `out`, and returns what is left. The last row fills `rest` exactly, so
 * rounding never leaves a gap at the frame's edge.
 */
function place(row: readonly Cell[], rest: Rect, last: boolean, out: Rect[]): Rect {
  const total = sum(row);
  const across = rest.width >= rest.height;
  const length = across ? rest.height : rest.width;
  const depth = last ? (across ? rest.width : rest.height) : total / length;
  let offset = 0;
  row.forEach((cell, n) => {
    const extent = n === row.length - 1 ? length - offset : (cell.area / total) * length;
    out[cell.index] = across
      ? { x: rest.x, y: rest.y + offset, width: depth, height: extent }
      : { x: rest.x + offset, y: rest.y, width: extent, height: depth };
    offset += extent;
  });
  return across
    ? { x: rest.x + depth, y: rest.y, width: rest.width - depth, height: rest.height }
    : { x: rest.x, y: rest.y + depth, width: rest.width, height: rest.height - depth };
}

/**
 * A squarified treemap (Bruls, Huizing and van Wijk, 2000): one rectangle per
 * weight, in the input's order, each with an area proportional to its weight
 * and as close to square as the order allows. Larger weights are placed
 * first, from the frame's top left. A weight of zero or less, or a frame with
 * no area, gets an empty rectangle at the frame's origin.
 */
export function squarify(weights: readonly number[], frame: Rect): Rect[] {
  const out: Rect[] = weights.map(() => ({ x: frame.x, y: frame.y, width: 0, height: 0 }));
  const total = weights.reduce((acc, weight) => acc + Math.max(weight, 0), 0);
  if (total <= 0 || frame.width <= 0 || frame.height <= 0) {
    return out;
  }
  const scale = (frame.width * frame.height) / total;
  const cells = weights
    .map((weight, index): Cell => ({ index, area: Math.max(weight, 0) * scale }))
    .filter((cell) => cell.area > 0)
    .toSorted((a, b) => b.area - a.area || a.index - b.index);

  let rest = frame;
  let row: Cell[] = [];
  for (const [n, cell] of cells.entries()) {
    const side = Math.min(rest.width, rest.height);
    if (row.length > 0 && worst([...row, cell], side) > worst(row, side)) {
      rest = place(row, rest, false, out);
      row = [];
    }
    row.push(cell);
    if (n === cells.length - 1) {
      place(row, rest, true, out);
    }
  }
  return out;
}
