import numpy as np
import csv

# Define edges with start and end points
edges = [
    [(247.7374737951, 283.2517331502), (247.7374737951, -182.1579585746)],     # east
    [(181.8933179509, -295.2244330957), (-76.4586113317, -441.8177197673)],    # southeast
    [(-96.198871072, -447.0280352992), (-201.11323921, -447.0280352992)],      # south
    [(-249.11323921, -399.0280352992), (-249.11323921, 283.2517331502)],       # west
    [(-237.0244766326, 311.9041820078), (-111.5236626445, 434.1584100271)],    # northwest
    [(-83.6124252219, 445.5059611695), (82.236659807, 445.5059611695)],        # north
    [(110.1478972296, 434.1584100271), (235.6487112177, 311.9041820078)],      # northeast
]

# Parameters
points_per_edge = 3     # number of interpolated points per edge (after exclusion)
exclude_margin_mm = 50   # how many mm to exclude from each end

all_points = []
for start, end in edges:
    start = np.array(start)
    end = np.array(end)
    edge_vec = end - start
    edge_len = np.linalg.norm(edge_vec)

    # Skip edge if too short
    if 2 * exclude_margin_mm >= edge_len:
        print(f"Skipping edge too short to trim: {start} to {end}")
        continue

    # Adjust start and end points inward by exclude_margin_mm
    direction = edge_vec / edge_len
    trimmed_start = start + exclude_margin_mm * direction
    trimmed_end = end - exclude_margin_mm * direction
    trimmed_vec = trimmed_end - trimmed_start

    # Interpolate points across trimmed edge
    t_vals = np.linspace(0, 1, points_per_edge)
    points = [trimmed_start + t * trimmed_vec for t in t_vals]
    all_points.extend(points)

# Save to CSV
output_file = "interpolated_trimmed_edges.csv"
with open(output_file, 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(["x", "y"])  # Header
    writer.writerows(points for points in all_points)

print(f"Saved {len(all_points)} trimmed/interpolated points to {output_file}")
