import numpy as np
import matplotlib.pyplot as plt

def read_points_csv(filename):
    return np.loadtxt(filename, delimiter=',')

def plot_shapes(original, offset, highlight_points=5):
    fig, ax = plt.subplots(figsize=(8, 8))

    # Close shapes if not already closed
    if not np.allclose(original[0], original[-1]):
        original = np.vstack([original, original[0]])
    if not np.allclose(offset[0], offset[-1]):
        offset = np.vstack([offset, offset[0]])

    # Plot the shapes
    ax.plot(original[:, 0], original[:, 1], 'k-', label='Original Shape')
    ax.plot(offset[:, 0], offset[:, 1], 'c--', label='Offset Shape')

    # Highlight corresponding points
    colors = ['red', 'green', 'blue', 'orange', 'purple']
    for i in range(min(highlight_points, len(original) - 1)):
        ax.plot(*original[i], 'o', color=colors[i % len(colors)], label=f'Original {i}')
        ax.plot(*offset[i], 's', color=colors[i % len(colors)], label=f'Offset {i}')
        ax.plot([original[i, 0], offset[i, 0]], [original[i, 1], offset[i, 1]],
                color=colors[i % len(colors)], linestyle='dotted')

    ax.set_aspect('equal')
    ax.legend()
    ax.set_title('Original vs. Offset Shape')
    plt.grid(True)
    plt.show()

def main():
    original_csv = 'base.csv'
    offset_csv = 'concentric.csv'

    original_points = read_points_csv(original_csv)
    offset_points = read_points_csv(offset_csv)

    plot_shapes(original_points, offset_points)

if __name__ == '__main__':
    main()
