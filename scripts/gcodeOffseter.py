import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from shapely.geometry import LineString, Point as ShapelyPoint
from scipy.interpolate import splprep, splev, interp1d, Rbf
from matplotlib.collections import LineCollection

# ---- Settings ----
probe_csv = "probed_points"
groove_depth = 5     # Groove cut depth (mm)
outcut_depth = 12    # Outcut depth (mm)
cut_step = 1         # Step-down increment (mm)
arc_steps = 100

# ---- 1. Read probe points ----
overlay_df = pd.read_csv(probe_csv)
overlay_df.columns = [c.lower() for c in overlay_df.columns]
valid_overlay = overlay_df[(overlay_df["z"] != -65)].copy()
if len(valid_overlay) < 3:
    raise Exception("Not enough valid probe points after filtering.")

# ---- 2. Segment definitions ----
def arc_g2(x0, y0, x1, y1, I, J, steps=arc_steps):  # CW arc
    cx, cy = x0 + I, y0 + J
    r = np.hypot(I, J)
    a0 = np.arctan2(y0 - cy, x0 - cx)
    a1 = np.arctan2(y1 - cy, x1 - cx)
    if a1 > a0:
        a1 -= 2 * np.pi
    angles = np.linspace(a0, a1, steps)
    xs = cx + r * np.cos(angles)
    ys = cy + r * np.sin(angles)
    xs[-1], ys[-1] = x1, y1
    return xs, ys

def arc_g3(x0, y0, x1, y1, I, J, steps=arc_steps):  # CCW arc
    cx, cy = x0 + I, y0 + J
    r = np.hypot(I, J)
    a0 = np.arctan2(y0 - cy, x0 - cx)
    a1 = np.arctan2(y1 - cy, x1 - cx)
    if a1 <= a0:
        a1 += 2 * np.pi
    angles = np.linspace(a0, a1, steps)
    xs = cx + r * np.cos(angles)
    ys = cy + r * np.sin(angles)
    xs[-1], ys[-1] = x1, y1
    return xs, ys

# Groove profile (small path)
groove_segments = [
    ((-82.9245, 463.367), (82.9245, 463.367), None, None),
    ((82.9245, 463.367), (122.7678, 447.1684), 0, -57.1),
    ((122.7678, 447.1684), (248.2686, 324.9141), None, None),
    ((248.2686, 324.9141), (265.5254, 284.0128), -39.8433, -40.9014),
    ((265.5254, 284.0128), (265.5254, -181.3969), None, None),
    ((265.5254, -181.3969), (191.0202, -309.336), -147.1, 0),
    ((191.0202, -309.336), (-67.3318, -455.9293), None, None),
    ((-67.3318, -455.9293), (-95.511, -463.367), -28.1792, 49.6623),
    ((-95.511, -463.367), (-200.4254, -463.367), None, None),
    ((-200.4254, -463.367), (-265.5254, -398.267), 0, 65.1),
    ((-265.5254, -398.267), (-265.5254, 284.0128), None, None),
    ((-265.5254, 284.0128), (-248.2686, 324.9141), 57.1, 0),
    ((-248.2686, 324.9141), (-122.7678, 447.1684), None, None),
    ((-122.7678, 447.1684), (-82.9245, 463.367), 39.8433, -40.9014)
]

# Outcut profile (large, octagonal path)
outcut_segments = [
    ((-300.9254, -460.267), (-262.4254, -498.767), 38.5, 0.0),
    ((-262.4254, -498.767), (262.4254, -498.767), None, None),
    ((262.4254, -498.767), (300.9254, -460.267), 0.0, 38.5),
    ((300.9254, -460.267), (300.9254, 315.2341), None, None),
    ((300.9254, 315.2341), (295.3343, 328.4859), -18.5, 0.0),
    ((295.3343, 328.4859), (125.9188, 493.5188), None, None),
    ((125.9188, 493.5188), (113.0099, 498.767), -12.9089, -13.2518),
    ((113.0099, 498.767), (-113.0099, 498.767), None, None),
    ((-113.0099, 498.767), (-125.9188, 493.5188), 0.0, -18.5),
    ((-125.9188, 493.5188), (-295.3343, 328.4859), None, None),
    ((-295.3343, 328.4859), (-300.9254, 315.2341), 12.9089, -13.2518),
    ((-300.9254, 315.2341), (-300.9254, -460.267), None, None)
]

def build_path(segments, arc_func, arc_steps=arc_steps):
    path_x, path_y = [], []
    for (x0, y0), (x1, y1), I, J in segments:
        if I is None:
            xs = np.linspace(x0, x1, arc_steps)
            ys = np.linspace(y0, y1, arc_steps)
        else:
            xs, ys = arc_func(x0, y0, x1, y1, I, J, steps=arc_steps)
        if len(path_x) > 0:
            xs, ys = xs[1:], ys[1:]
        path_x.extend(xs)
        path_y.extend(ys)
    return np.array(path_x), np.array(path_y)

# ---- 3. Center paths ----
def center_path(x, y, valid_overlay):
    bbox_center_x = (valid_overlay["x"].min() + valid_overlay["x"].max()) / 2
    bbox_center_y = (valid_overlay["y"].min() + valid_overlay["y"].max()) / 2
    path_center_x = (np.min(x) + np.max(x)) / 2
    path_center_y = (np.min(y) + np.max(y)) / 2
    offset_x = bbox_center_x - path_center_x
    offset_y = bbox_center_y - path_center_y
    return x + offset_x, y + offset_y

groove_x, groove_y = build_path(groove_segments, arc_g2)
outcut_x, outcut_y = build_path(outcut_segments, arc_g3)
groove_x, groove_y = center_path(groove_x, groove_y, valid_overlay)
outcut_x, outcut_y = center_path(outcut_x, outcut_y, valid_overlay)
orig_groove_x, orig_groove_y = np.copy(groove_x), np.copy(groove_y)
orig_outcut_x, orig_outcut_y = np.copy(outcut_x), np.copy(outcut_y)

# ---- 4. Groove Warping ----
baseline = 80.0
gcode_path = LineString(list(zip(groove_x, groove_y)))
rbf = Rbf(valid_overlay["x"], valid_overlay["y"],
    [ShapelyPoint(row["x"], row["y"]).distance(gcode_path.interpolate(gcode_path.project(ShapelyPoint(row["x"], row["y"])))) - baseline
    for idx, row in valid_overlay.iterrows()], function='linear')

warped_x, warped_y = [], []
for i in range(len(groove_x)):
    x, y = groove_x[i], groove_y[i]
    if i == 0:
        dx, dy = groove_x[i+1] - x, groove_y[i+1] - y
    elif i == len(groove_x)-1:
        dx, dy = x - groove_x[i-1], y - groove_y[i-1]
    else:
        dx, dy = groove_x[i+1] - groove_x[i-1], groove_y[i+1] - groove_y[i-1]
    tangent = np.array([dx, dy])
    norm = np.linalg.norm(tangent)
    if norm == 0:
        nx, ny = 0, 1
    else:
        tangent /= norm
        nx, ny = -tangent[1], tangent[0]
    delta = rbf(x, y)
    warped_x.append(x + delta * nx)
    warped_y.append(y + delta * ny)

def clean_path(x, y):
    points = np.column_stack([x, y])
    points = points[~np.isnan(points).any(axis=1)]
    points = points[~np.isinf(points).any(axis=1)]
    diffs = np.diff(points, axis=0)
    mask = np.any(diffs != 0, axis=1)
    cleaned = np.vstack([points[0], points[1:][mask]])
    return cleaned[:,0], cleaned[:,1]

warped_x, warped_y = clean_path(np.array(warped_x), np.array(warped_y))

# Spline smoothing for groove
tck, u = splprep([warped_x, warped_y], s=5.0, per=True)
unew = np.linspace(0, 1.0, 1000)
smooth_groove_x, smooth_groove_y = splev(unew, tck)
gcode_path = LineString(list(zip(smooth_groove_x, smooth_groove_y)))

# Depth interpolation along groove
probe_proj_dist = [gcode_path.project(ShapelyPoint(row["x"], row["y"])) for idx, row in valid_overlay.iterrows()]
path_proj_dist = [gcode_path.project(ShapelyPoint(x, y)) for x, y in zip(smooth_groove_x, smooth_groove_y)]
sort_idx = np.argsort(probe_proj_dist)
probe_proj_dist_sorted = np.array(probe_proj_dist)[sort_idx]
z_values_sorted = np.array(valid_overlay["z"])[sort_idx]
z_interp = interp1d(probe_proj_dist_sorted, z_values_sorted, kind='linear', fill_value='extrapolate')
surface_z = z_interp(path_proj_dist)  # Tank surface at each groove path point

# ---- 5. Outcut Warping (use groove delta for same fraction of path) ----
groove_path_obj = LineString(list(zip(smooth_groove_x, smooth_groove_y)))
outcut_path_obj = LineString(list(zip(outcut_x, outcut_y)))
groove_len = groove_path_obj.length
outcut_len = outcut_path_obj.length
n_samples = 1000
groove_distances = np.linspace(0, groove_len, n_samples)
outcut_distances = np.linspace(0, outcut_len, n_samples)
groove_pts = np.array([groove_path_obj.interpolate(d) for d in groove_distances])
outcut_pts = np.array([outcut_path_obj.interpolate(d) for d in outcut_distances])
groove_frac = groove_distances / groove_len
outcut_frac = outcut_distances / outcut_len

# Deformation delta for each groove point
groove_delta = np.array([rbf(pt.x, pt.y) for pt in groove_pts])
delta_interp = interp1d(np.linspace(0, 1, len(groove_delta)), groove_delta, kind='linear', fill_value='extrapolate')
outcut_delta = delta_interp(outcut_frac)

# Offset outcut by delta in normal-to-tank-center direction
tank_center_x = (valid_overlay["x"].min() + valid_overlay["x"].max()) / 2
tank_center_y = (valid_overlay["y"].min() + valid_overlay["y"].max()) / 2
outcut_warped_x, outcut_warped_y = [], []
for pt, delta in zip(outcut_pts, outcut_delta):
    x0, y0 = pt.x, pt.y
    angle = np.arctan2(y0 - tank_center_y, x0 - tank_center_x)
    r_base = np.hypot(x0 - tank_center_x, y0 - tank_center_y)
    new_r = r_base + delta
    wx = tank_center_x + new_r * np.cos(angle)
    wy = tank_center_y + new_r * np.sin(angle)
    outcut_warped_x.append(wx)
    outcut_warped_y.append(wy)
outcut_warped_x, outcut_warped_y = np.array(outcut_warped_x), np.array(outcut_warped_y)

# Depth along outcut
outcut_path_obj_warped = LineString(list(zip(outcut_warped_x, outcut_warped_y)))
outcut_proj_dist = [outcut_path_obj_warped.project(ShapelyPoint(x, y)) for x, y in zip(outcut_warped_x, outcut_warped_y)]
outcut_z = z_interp(np.clip(outcut_proj_dist, probe_proj_dist_sorted[0], probe_proj_dist_sorted[-1]))
final_outcut_z = outcut_z - outcut_depth

# ---- 6. Drill points, center and warp (use groove delta at nearest groove fraction) ----
drill_points = np.array([
    [-68.2026, -469.0015], [-159.6754, -473.617], [-242.903, -460.1448],
    [-275.0254, -391.0001], [-275.0254, -281.0001], [-275.0254, -168.5001],
    [-275.0254, -56.0001], [-275.0254, 56.4999], [-275.0254, 168.9999], [-275.0254, 281.4999],
    [-222.0695, 364.0144], [-141.4845, 442.5147], [-54.25, 473.617], [55.75, 473.617],
    [142.9845, 442.5147], [223.5695, 364.0144], [276.5254, 281.4999], [276.5254, 168.9999],
    [276.5254, 56.4999], [276.5254, -56.0001], [276.5254, -168.5001],
    [259.9683, -251.6983], [203.1362, -314.6718], [111.9234, -366.4275], [20.7106, -418.1832]
])

bbox_center_x = (valid_overlay["x"].min() + valid_overlay["x"].max()) / 2
bbox_center_y = (valid_overlay["y"].min() + valid_overlay["y"].max()) / 2
drill_center_x = (np.min(drill_points[:,0]) + np.max(drill_points[:,0])) / 2
drill_center_y = (np.min(drill_points[:,1]) + np.max(drill_points[:,1])) / 2
drill_offset_x = bbox_center_x - drill_center_x
drill_offset_y = bbox_center_y - drill_center_y
centered_drill_points = drill_points + np.array([drill_offset_x, drill_offset_y])

# Warp drill points with groove delta at closest groove fraction
warped_drill_points = []
drill_surface_z = []
for x0, y0 in centered_drill_points:
    # Find closest groove fraction to the projected distance on groove
    d_proj = gcode_path.project(ShapelyPoint(x0, y0))
    groove_frac_pt = d_proj / groove_len
    # Interpolate the deformation at that fraction
    delta = delta_interp(groove_frac_pt)
    # Offset in direction normal to tank center
    angle = np.arctan2(y0 - tank_center_y, x0 - tank_center_x)
    r_base = np.hypot(x0 - tank_center_x, y0 - tank_center_y)
    wx = tank_center_x + (r_base + delta) * np.cos(angle)
    wy = tank_center_y + (r_base + delta) * np.sin(angle)
    warped_drill_points.append([wx, wy])
    # Interpolate local surface Z for correct hole top
    z0 = z_interp(np.clip(d_proj, probe_proj_dist_sorted[0], probe_proj_dist_sorted[-1]))
    drill_surface_z.append(z0)
warped_drill_points = np.array(warped_drill_points)
drill_surface_z = np.array(drill_surface_z)

# ---- 7. Plot everything ----
plt.figure(figsize=(13, 13))
# Warped groove
points_g = np.array([smooth_groove_x, smooth_groove_y]).T.reshape(-1, 1, 2)
segments_g = np.concatenate([points_g[:-1], points_g[1:]], axis=1)
norm_g = plt.Normalize(np.min(surface_z-groove_depth), np.max(surface_z))
lc_g = LineCollection(segments_g, cmap='viridis', norm=norm_g)
final_groove_z = surface_z - groove_depth
lc_g.set_array(final_groove_z)
lc_g.set_linewidth(2)
plt.gca().add_collection(lc_g)

# Warped outcut
points_o = np.array([outcut_warped_x, outcut_warped_y]).T.reshape(-1, 1, 2)
segments_o = np.concatenate([points_o[:-1], points_o[1:]], axis=1)
norm_o = plt.Normalize(np.min(final_outcut_z), np.max(outcut_z))
lc_o = LineCollection(segments_o, cmap='plasma', norm=norm_o)
lc_o.set_array(final_outcut_z)
lc_o.set_linewidth(2)
plt.gca().add_collection(lc_o)

# Plot original (centered) and warped drill points for comparison
plt.scatter(centered_drill_points[:,0], centered_drill_points[:,1],
    marker='o', facecolors='none', edgecolors='r', s=120, linewidths=2, label='Original Drill Points (centered)', zorder=11)
plt.scatter(warped_drill_points[:,0], warped_drill_points[:,1],
    marker='x', color='black', s=120, linewidths=2, label='Warped Drill Points', zorder=12)

plt.plot(orig_groove_x, orig_groove_y, '--', color='gray', linewidth=1.2, label="Original Groove Path")
plt.plot(orig_outcut_x, orig_outcut_y, 'r--', linewidth=1.2, label="Original Outcut (dashed)")

plt.scatter(valid_overlay["x"], valid_overlay["y"], c=valid_overlay["z"], cmap='viridis', edgecolors='k', label='Probe Points', zorder=10, norm=norm_g)
plt.colorbar(lc_g, label="Groove Z (mm, deepest pass)", pad=0.01)
plt.colorbar(lc_o, label="Outcut Z (mm, deepest pass)", pad=0.04)
plt.gca().set_aspect('equal')
plt.xlabel("X (mm)")
plt.ylabel("Y (mm)")
plt.title(f"Groove and Outcut, Warped Depth\n(Color: final cut Z at each location)")
plt.legend()
plt.grid(True)
plt.tight_layout()
plt.show()

# ---- 8. Export G-code for groove, holes, outcut in a single file ----
spiral_stepdown = 2.0   # mm per spiral
hole_diameter = 8.5
tool_diameter = 7.0
hole_radius = hole_diameter / 2
tool_radius = tool_diameter / 2
offset = hole_radius - tool_radius   # 0.75mm offset from hole center

safe_height = 3.0
approach_height = 1.0
final_depth = -12.0

job_travel_height = -7.7  # mm above probe zero (safe for rapid XY moves)

with open("tank_full_job_warped.gcode", "w") as f:
    # --- 1. Warped Groove ---
    f.write("G21 ; Set units to mm\nG90 ; Absolute positioning\n")
    for pass_depth in range(1, groove_depth+1):
        zpath = surface_z - pass_depth
        f.write(f"( Groove Pass {pass_depth}: {pass_depth}mm below surface )\n")
        # Move to groove start at travel height, then plunge Z down
        f.write(f"G0 Z{job_travel_height:.3f}\n")
        f.write(f"G0 X{smooth_groove_x[0]:.3f} Y{smooth_groove_y[0]:.3f}\n")
        f.write(f"G0 Z{zpath[0]+35:.3f}\n")
        for xg, yg, zg in zip(smooth_groove_x[1:], smooth_groove_y[1:], zpath[1:]):
            f.write(f"G1 X{xg:.3f} Y{yg:.3f} Z{zg+35:.3f} F1000\n")
    # *** Rapid up after groove ***
    f.write(f"G0 Z{job_travel_height:.3f}\n")
    f.write("( End Groove )\n")

    # --- 2. Spiral-milled Holes (correct Z start) ---
    f.write("\n( Spiral-milled holes )\n")
    for i, (x, y) in enumerate(warped_drill_points):
        z_top = drill_surface_z[i]
        f.write(f"\n( Spiral Hole {i+1} at X{x:.3f} Y{y:.3f} )\n")
        # Move to hole XY at travel height, then approach Z
        f.write(f"G0 Z{job_travel_height:.3f}\n")
        f.write(f"G0 X{x:.3f} Y{y:.3f}\n")
        f.write(f"G0 Z{z_top + approach_height + 35:.3f}\n")
        # Rapid to spiral start point (offset from center) at approach Z
        start_x = x + offset
        start_y = y
        f.write(f"G0 X{start_x:.3f} Y{start_y:.3f}\n")
        current_depth = z_top
        while current_depth > z_top + final_depth:
            next_depth = max(current_depth - spiral_stepdown, z_top + final_depth)
            # Cut full circle (G3, CCW), centered at hole center (I = -offset, J = 0)
            f.write(f"G1 Z{next_depth+35:.3f} F200\n")  # Spiral down
            f.write(f"G3 X{start_x:.3f} Y{start_y:.3f} I{-offset:.3f} J0.000 F500\n")
            current_depth = next_depth
        # Retract to job travel height after each hole
        f.write(f"G0 Z{job_travel_height:.3f}\n")
    f.write("( End Holes )\n")

    # --- 3. Warped Outcut ---
    # Ensure we're at travel height before moving XY
    f.write(f"G0 Z{job_travel_height:.3f}\n")
    for pass_depth in range(1, outcut_depth+1):
        zpath = outcut_z - pass_depth
        f.write(f"( Outcut Pass {pass_depth}: {pass_depth}mm below surface )\n")
        # XY move at travel height
        f.write(f"G0 X{outcut_warped_x[0]:.3f} Y{outcut_warped_y[0]:.3f}\n")
        # Z plunge to start cut
        f.write(f"G0 Z{zpath[0]+35:.3f}\n")
        for xo, yo, zo in zip(outcut_warped_x[1:], outcut_warped_y[1:], zpath[1:]):
            f.write(f"G1 X{xo:.3f} Y{yo:.3f} Z{zo+35:.3f} F1000\n")
    # *** Final retract up ***
    f.write(f"G0 Z{job_travel_height:.3f}\n")
    f.write("M2 ; End of program\n")
