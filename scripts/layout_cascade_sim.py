"""Simulate the vertical cascade of `doc/design_incremental_layout.md` (Step 5b)
over a `.cnnd` corpus and print the disturbance statistics quoted there.

For every node in every top-level network, a node-sized rect (160 x 83,
inflated by the gap on all sides) is dropped exactly onto that node's position
with the node itself removed from the obstacle set, and the cascade is run
against the remaining nodes. Reported per corpus: how many nodes each drop
pushed, and the final x-extent of the pushed set relative to the rect.

Node boxes use `node_layout::estimate_node_height` (58 + max(22 * params, 25))
with the argument count as the parameter count, and real `CommentData`
dimensions for comments. Bodies are not entered, matching the corpus
measurement in the design document.

Usage:
    python scripts/layout_cascade_sim.py from_mechadense.cnnd demolib/baselib_with_demos.cnnd
"""

import json
import statistics
import sys

NODE_WIDTH = 160.0
NODE_HEIGHT = 83.0
GAP = 20.0  # DEFAULT_VERTICAL_GAP; also used as the inflation on every side


def load(path):
    """Top-level networks as {id: [x, y, w, h]}, skipping networks with < 2 nodes."""
    doc = json.load(open(path, encoding="utf-8"))
    networks = []
    for entry in doc["node_networks"]:
        network = entry[1] if isinstance(entry, list) else entry
        nodes = network["nodes"]
        nodes = list(nodes.values()) if isinstance(nodes, dict) else nodes
        boxes = {}
        for node in nodes:
            if node.get("node_type_name") == "Comment":
                data = node.get("data", {})
                w, h = float(data.get("width", 200)), float(data.get("height", 100))
            else:
                params = len(node.get("arguments") or [])
                w, h = NODE_WIDTH, 58 + max(22 * params, 25)
            x, y = node["position"]
            boxes[int(node["id"])] = [float(x), float(y), w, h]
        if len(boxes) >= 2:
            networks.append(boxes)
    return networks


def overlaps(a, b, axis):
    """Strict interval overlap on one axis (0 = x, 1 = y)."""
    size = 2 + axis
    return a[axis] < b[axis] + b[size] and b[axis] < a[axis] + a[size]


def cascade(boxes, rect):
    """Run the Step 5b cascade for `rect`; mutates `boxes`. Returns (pushed, extent ratio)."""
    rect_cy = rect[1] + rect[3] / 2
    wave = [i for i, b in boxes.items() if overlaps(b, rect, 0) and overlaps(b, rect, 1)]
    wave.sort(key=lambda i: abs(boxes[i][1] + boxes[i][3] / 2 - rect_cy))
    direction = {i: "up" if boxes[i][1] + boxes[i][3] / 2 < rect_cy else "down" for i in wave}
    queue = [(i, None) for i in wave]
    moved = set()
    while queue:
        n, blocker_id = queue.pop(0)
        b = boxes[n]
        blocker = rect if blocker_id is None else boxes[blocker_id]
        if direction[n] == "down":
            ny = blocker[1] + blocker[3] + GAP
            if ny > b[1]:
                b[1] = ny
                moved.add(n)
        else:
            ny = blocker[1] - GAP - b[3]
            if ny < b[1]:
                b[1] = ny
                moved.add(n)
        n_cy = b[1] + b[3] / 2
        for m, c in boxes.items():
            if m == n or not (overlaps(b, c, 0) and overlaps(b, c, 1)):
                continue
            m_cy = c[1] + c[3] / 2
            if (direction[n] == "down" and m_cy > n_cy) or (direction[n] == "up" and m_cy < n_cy):
                direction[m] = direction[n]
                queue.append((m, n))
    left = min([rect[0]] + [boxes[i][0] for i in moved])
    right = max([rect[0] + rect[2]] + [boxes[i][0] + boxes[i][2] for i in moved])
    return len(moved), (right - left) / rect[2]


def run(path):
    counts, ratios = [], []
    for boxes in load(path):
        for k, (x, y, _w, _h) in list(boxes.items()):
            obstacles = {i: list(b) for i, b in boxes.items() if i != k}
            rect = [x - GAP, y - GAP, NODE_WIDTH + 2 * GAP, NODE_HEIGHT + 2 * GAP]
            pushed, ratio = cascade(obstacles, rect)
            counts.append(pushed)
            ratios.append(ratio)
    n = len(counts)
    ordered = sorted(ratios)
    pct = lambda p: ordered[min(n - 1, int(p * n))]
    share = lambda pred: sum(1 for c in counts if pred(c)) / n
    print(f"{path}: drops={n}")
    print(f"  pushing nothing {share(lambda c: c == 0):.1%}, <= 1 node {share(lambda c: c <= 1):.1%}, "
          f"<= 5 nodes {share(lambda c: c <= 5):.1%}, max pushed {max(counts)}")
    print(f"  extent / rect width: median {statistics.median(ratios):.2f}, p90 {pct(0.9):.2f}, "
          f"p99 {pct(0.99):.2f}, max {max(ratios):.2f}")


if __name__ == "__main__":
    for corpus in sys.argv[1:] or ["from_mechadense.cnnd", "demolib/baselib_with_demos.cnnd"]:
        run(corpus)
