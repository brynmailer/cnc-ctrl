import numpy as np
import csv
import matplotlib.pyplot as plt

# PARAMETERS
n = 79   # inward offset in mm for inner points
mm = 64  # inward offset in mm for expected points (different concentric level)
x = [3, 1, 4, 2, 2, 2, 3]  # number of equidistant points per edge (list)
y = 109   # outward perpendicular offset
z = 10   # exclusion distance from edge vertices

edges = [
    [(181.8933179509, -295.2244330957), (-76.4586113317, -441.8177197673)],
    [(-96.198871072, -447.0280352992), (-201.11323921, -447.0280352992)],
    [(-249.11323921, -399.0280352992), (-249.11323921, 283.2517331502)],
    [(-237.0244766326, 311.9041820078), (-111.5236626445, 434.1584100271)],
    [(-83.6124252219, 445.5059611695), (82.236659807, 445.5059611695)],
    [(110.1478972296, 434.1584100271), (235.6487112177, 311.9041820078)],
    [(247.7374737951, 283.2517331502), (247.7374737951, -182.1579585746)],
]

def unit_vector(v):    return v / np.linalg.norm(v)                                                                                                                         

def normal_vector(v):
    return np.array([-v[1], v[0]])

def offset_edge(p1, p2, distance):
    v = np.array(p2) - np.array(p1)
    n = unit_vector(normal_vector(v)) * distance
    return [tuple(np.array(p1) + n), tuple(np.array(p2) + n)]

def generate_inner_edges(edges, offset_dist):
    return [offset_edge(p1, p2, -offset_dist) for p1, p2 in edges]

def generate_equidistant_points(p1, p2, num_points, exclude_dist):
    p1 = np.array(p1)
    p2 = np.array(p2)
    total_length = np.linalg.norm(p2 - p1)
    direction = unit_vector(p2 - p1)
    spacing = total_length / (num_points + 1)                                                                                                                                        
    
    points = []
    for i in range(1, num_points + 1):
        point = p1 + direction * spacing * i
        if spacing * i > exclude_dist and spacing * (num_points + 1 - i) > exclude_dist:
            points.append(tuple(point))
    return points

def offset_points_outward(points, p1, p2, dist):
    v = np.array(p2) - np.array(p1)
    normal = unit_vector(normal_vector(v))                                                                                                                                           
    return [tuple(np.array(pt) + dist * normal) for pt in points]
def mirror_points_y_axis(points):
    """Mirror points across the y-axis (negate x-coordinates)"""
    return [(-x, y) for x, y in points]

# Validate input
if len(x) != len(edges):
    raise ValueError(f"Length of points list ({len(x)}) must match number of edges ({len(edges)})")

# Generate offset edges
inner_edges = generate_inner_edges(edges, n)
expected_edges = generate_inner_edges(edges, mm)

# Generate points with individual counts per edge
inner_points = []
expected_points = []
outer_points = []

for i, ((inner_p1, inner_p2), (exp_p1, exp_p2)) in enumerate(zip(inner_edges, expected_edges)):
    num_points_for_edge = x[i]  # Get the specific number of points for this edge
    
    # Generate points on inner edge
    inner_eq_points = generate_equidistant_points(inner_p1, inner_p2, num_points_for_edge, z)
    inner_points.extend(inner_eq_points)
    
    # Generate points on expected edge (same spacing pattern but different offset)
    expected_eq_points = generate_equidistant_points(exp_p1, exp_p2, num_points_for_edge, z)
    expected_points.extend(expected_eq_points)
    
    # Generate outer points (perpendicular offset from inner points)
    outer_points.extend(offset_points_outward(inner_eq_points, inner_p1, inner_p2, y))

# Create mirrored versions
inner_points_mirrored = mirror_points_y_axis(inner_points)
expected_points_mirrored = mirror_points_y_axis(expected_points)
outer_points_mirrored = mirror_points_y_axis(outer_points)

# Save original files with 'aconcentric' prefix
with open('aconcentric_inner.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['x', 'y'])
    writer.writerows(inner_points)

with open('aconcentric_expected.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerows(expected_points)

with open('aconcentric_outer.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['x', 'y'])
    writer.writerows(outer_points)

# Save mirrored files with 'bconcentric' prefix
with open('bconcentric_inner.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['x', 'y'])
    writer.writerows(inner_points_mirrored)

with open('bconcentric_expected.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerows(expected_points_mirrored)

with open('bconcentric_outer.csv', 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['x', 'y'])
    writer.writerows(outer_points_mirrored)

def plot_shape(edges, color='black', label=None):
    for p1, p2 in edges:
        xs, ys = zip(p1, p2)
        plt.plot(xs, ys, color=color, linewidth=2)
    if label:
        mid = edges[len(edges) // 2]
        midx = (mid[0][0] + mid[1][0]) / 2
        midy = (mid[0][1] + mid[1][1]) / 2
        plt.text(midx, midy, label, color=color)

def plot_points(points, color, label=None, alpha=1.0):
    if points:  # Check if points exist
        xs, ys = zip(*points)
        plt.scatter(xs, ys, color=color, s=20, label=label, alpha=alpha)

# Visualize both original and mirrored
plt.figure(figsize=(16, 10))
plt.axis('equal')

# Original shape
plot_shape(edges, color='gray', label='Original')

# Offset shapes
plot_shape(inner_edges, color='blue', label=f'Inner Offset ({n}mm)')
plot_shape(expected_edges, color='green', label=f'Expected Offset ({mm}mm)')

# Original points (A set)
plot_points(inner_points, color='blue', label='A Inner points')
plot_points(expected_points, color='green', label='A Expected points')
plot_points(outer_points, color='red', label='A Outer points')

# Mirrored points (B set)
plot_points(inner_points_mirrored, color='darkblue', label='B Inner points (mirrored)', alpha=0.7)
plot_points(expected_points_mirrored, color='darkgreen', label='B Expected points (mirrored)', alpha=0.7)
plot_points(outer_points_mirrored, color='darkred', label='B Outer points (mirrored)', alpha=0.7)

plt.legend()
plt.title(f"Concentric Shapes and Points (Original and Y-axis Mirrored)\nPoints per edge: {x}")
plt.grid(True)
plt.show()

# Print summary
print(f"Generated {len(inner_points)} inner points, {len(expected_points)} expected points, and {len(outer_points)} outer points")
print(f"Points per edge: {x}")
print(f"Inner offset: {n}mm, Expected offset: {mm}mm, Outer perpendicular offset: {y}mm")
total_points_expected = sum(x)
print(f"Expected total points per set: {total_points_expected}")
if len(inner_points) < total_points_expected:
    print(f"Note: Some points were excluded due to the {z}mm exclusion distance from vertices")

print("\nFiles created:")
print("Original set (A): aconcentric_inner.csv, aconcentric_expected.csv, aconcentric_outer.csv")
print("Mirrored set (B): bconcentric_inner.csv, bconcentric_expected.csv, bconcentric_outer.csv")
