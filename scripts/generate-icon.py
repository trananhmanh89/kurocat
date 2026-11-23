"""Draws the KuroCat app icon (a red cat head) and writes every icon file the app uses.

Usage (from the repository root, needs Pillow):
    python scripts/generate-icon.py

Writes into assets/icons/: icon.png (512), 32x32.png, 128x128.png, 128x128@2x.png, 256x256.png,
icon.ico (16-256) and tray-icon.png (64). Rebuild the app afterwards so the tray picks it up.
"""

import os

from PIL import Image, ImageDraw, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICON_DIR = os.path.join(ROOT, "assets", "icons")

# The design is laid out on a 1024 x 1024 grid and drawn SUPERSAMPLE times larger for smooth edges.
GRID = 1024
SUPERSAMPLE = 4
SIZE = GRID * SUPERSAMPLE

FUR_TOP = (248, 113, 113)
FUR_BOTTOM = (220, 38, 38)
OUTLINE = (69, 10, 10)
EAR_INNER = (254, 205, 211)
EYE = (212, 240, 68)
PUPIL = (15, 23, 42)
NOSE = (255, 214, 224)
WHITE = (255, 255, 255)


def scaled(points):
    """Converts grid coordinates to supersampled pixel coordinates.

    @param points A list of (x, y) tuples on the 1024 grid.
    @return The same points in supersampled pixels.
    """
    return [(x * SUPERSAMPLE, y * SUPERSAMPLE) for x, y in points]


def box(left, top, right, bottom):
    """Converts a grid rectangle to supersampled pixels.

    @param left Left edge on the grid.
    @param top Top edge on the grid.
    @param right Right edge on the grid.
    @param bottom Bottom edge on the grid.
    @return The rectangle in supersampled pixels.
    """
    return [left * SUPERSAMPLE, top * SUPERSAMPLE, right * SUPERSAMPLE, bottom * SUPERSAMPLE]


def head_mask():
    """Draws the silhouette of the head with both ears.

    @return A greyscale mask, white inside the head.
    """
    mask = Image.new("L", (SIZE, SIZE), 0)
    draw = ImageDraw.Draw(mask)
    draw.ellipse(box(112, 300, 912, 940), fill=255)
    draw.polygon(scaled([(132, 560), (178, 92), (470, 330)]), fill=255)
    draw.polygon(scaled([(892, 560), (846, 92), (554, 330)]), fill=255)
    # Round off the ear tips a little.
    return mask.filter(ImageFilter.GaussianBlur(6 * SUPERSAMPLE)).point(lambda value: 255 if value > 128 else 0)


def vertical_gradient(top_color, bottom_color):
    """Makes a full-size image that fades from one colour at the top to another at the bottom.

    @param top_color RGB colour at the top.
    @param bottom_color RGB colour at the bottom.
    @return An RGBA image.
    """
    gradient = Image.new("RGBA", (1, 256))
    for row in range(256):
        mix = row / 255
        color = tuple(round(top_color[i] + (bottom_color[i] - top_color[i]) * mix) for i in range(3))
        gradient.putpixel((0, row), color + (255,))
    return gradient.resize((SIZE, SIZE))


def draw_icon():
    """Draws the full icon at the supersampled size.

    @return An RGBA image of SIZE x SIZE pixels.
    """
    image = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    mask = head_mask()

    # Dark outline: the silhouette grown outwards (blur, then a low threshold), under the fur.
    outline_mask = mask.filter(ImageFilter.GaussianBlur(14 * SUPERSAMPLE)).point(lambda value: 255 if value > 12 else 0)
    image.paste(Image.new("RGBA", (SIZE, SIZE), OUTLINE + (255,)), (0, 0), outline_mask)
    image.paste(vertical_gradient(FUR_TOP, FUR_BOTTOM), (0, 0), mask)

    draw = ImageDraw.Draw(image)
    # Inner ears.
    draw.polygon(scaled([(200, 470), (214, 196), (400, 352)]), fill=EAR_INNER)
    draw.polygon(scaled([(824, 470), (810, 196), (624, 352)]), fill=EAR_INNER)

    # Eyes: outline, iris, pupil and two catchlights each.
    for center_x in (330, 694):
        draw.ellipse(box(center_x - 140, 430, center_x + 140, 730), fill=OUTLINE)
        draw.ellipse(box(center_x - 120, 450, center_x + 120, 710), fill=EYE)
        draw.ellipse(box(center_x - 62, 474, center_x + 62, 686), fill=PUPIL)
        draw.ellipse(box(center_x - 46, 494, center_x + 2, 546), fill=WHITE)
        draw.ellipse(box(center_x + 16, 600, center_x + 40, 626), fill=WHITE)

    # Nose and mouth.
    draw.polygon(scaled([(476, 744), (548, 744), (512, 790)]), fill=NOSE)
    line_width = 14 * SUPERSAMPLE
    draw.arc(box(446, 760, 516, 830), start=0, end=150, fill=OUTLINE, width=line_width)
    draw.arc(box(508, 760, 578, 830), start=30, end=180, fill=OUTLINE, width=line_width)
    draw.line(scaled([(512, 788), (512, 800)]), fill=OUTLINE, width=line_width)

    # Whiskers.
    whisker_width = 10 * SUPERSAMPLE
    for start, end in (((300, 770), (90, 740)), ((300, 800), (96, 820)), ((724, 770), (934, 740)), ((724, 800), (928, 820))):
        draw.line(scaled([start, end]), fill=OUTLINE + (220,), width=whisker_width)

    return image


def main():
    """Draws the icon and writes all icon files.

    @return None.
    """
    master = draw_icon().resize((1024, 1024), Image.Resampling.LANCZOS)
    os.makedirs(ICON_DIR, exist_ok=True)
    outputs = {
        "icon.png": 512,
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "256x256.png": 256,
        "tray-icon.png": 64,
    }
    for name, size in outputs.items():
        master.resize((size, size), Image.Resampling.LANCZOS).save(os.path.join(ICON_DIR, name))
    master.resize((256, 256), Image.Resampling.LANCZOS).save(
        os.path.join(ICON_DIR, "icon.ico"),
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print("Wrote icons to", ICON_DIR)


if __name__ == "__main__":
    main()
