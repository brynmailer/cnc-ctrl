import numpy as np
import matplotlib.pyplot as plt

# Load the CSV file
coords = np.loadtxt("InsideV1_coords.csv", delimiter=',', skiprows=1)

# Plot
plt.figure(figsize=(8, 8))
plt.plot(coords[:, 0], coords[:, 1], 'o-', markersize=3)
plt.title("Plot of Extracted DXF Coordinates")
plt.xlabel("X")
plt.ylabel("Y")
plt.gca().set_aspect('equal', adjustable='box')
plt.grid(True)
plt.show()
