import math
import os
import shutil
import subprocess
from PIL import Image, ImageDraw, ImageFilter, ImageOps

def create_diagonal_gradient(width, height, color1, color2):
    base = Image.new("RGBA", (width, height))
    draw = ImageDraw.Draw(base)
    max_val = width + height
    for i in range(0, max_val + 1, 2):
        ratio = i / max_val
        r = int(color1[0] * (1 - ratio) + color2[0] * ratio)
        g = int(color1[1] * (1 - ratio) + color2[1] * ratio)
        b = int(color1[2] * (1 - ratio) + color2[2] * ratio)
        
        x1 = min(i, width - 1)
        y1 = max(0, i - (width - 1))
        x2 = max(0, i - (height - 1))
        y2 = min(i, height - 1)
        
        draw.line([(x1, y1), (x2, y2)], fill=(r, g, b, 255), width=3)
    return base

def draw_icon():
    scale = 4
    size = 1024 * scale
    
    # Colors
    bg_color_top = (28, 30, 43)      # Rich dark purple-blue slate
    bg_color_bot = (8, 9, 12)        # Deep near-black
    
    shell_color_top = (110, 122, 160) # Sleek anodized aluminum highlight
    shell_color_bot = (32, 35, 48)    # Deep shadow gray-blue
    
    screen_color = (18, 19, 26)       # Dark faceplate
    neon_cyan = (0, 242, 254)         # Neon cyan glow
    neon_white = (220, 253, 255)      # Intense hot core
    
    # 1. Background
    canvas = create_diagonal_gradient(size, size, bg_color_top, bg_color_bot)
    
    # 2. Build the Robot Silhouette Mask (head + antenna)
    shell_mask = Image.new("L", (size, size), 0)
    shell_draw = ImageDraw.Draw(shell_mask)
    
    head_center = (2048, 2380)
    head_radius = 820
    
    # Draw head
    shell_draw.ellipse(
        [
            (head_center[0] - head_radius, head_center[1] - head_radius),
            (head_center[0] + head_radius, head_center[1] + head_radius)
        ],
        fill=255
    )
    
    # 'P' loop coords
    loop_center = (2308, 800)
    loop_outer_r = 340
    loop_inner_r = 160
    
    stem_left = 1968
    stem_right = 2128
    stem_top = 800
    stem_bot = 1650
    
    # Draw antenna stem
    shell_draw.rectangle([(stem_left, stem_top), (stem_right, stem_bot)], fill=255)
    
    # Draw antenna outer loop
    shell_draw.ellipse(
        [
            (loop_center[0] - loop_outer_r, loop_center[1] - loop_outer_r),
            (loop_center[0] + loop_outer_r, loop_center[1] + loop_outer_r)
        ],
        fill=255
    )
    
    # Cutout the inner loop
    shell_draw.ellipse(
        [
            (loop_center[0] - loop_inner_r, loop_center[1] - loop_inner_r),
            (loop_center[0] + loop_inner_r, loop_center[1] + loop_inner_r)
        ],
        fill=0
    )
    
    # 3. Create & Apply Drop Shadow
    shadow_mask = shell_mask.filter(ImageFilter.GaussianBlur(80))
    shadow_layer = Image.new("RGBA", (size, size), (4, 5, 8, 180))
    shadow_offset = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow_offset.paste(shadow_layer, (0, 60), shadow_mask)
    canvas.alpha_composite(shadow_offset)
    
    # 4. Create Shell Layer with diagonal gradient
    shell_layer = create_diagonal_gradient(size, size, shell_color_top, shell_color_bot)
    
    # Paste shell onto canvas
    canvas.paste(shell_layer, (0, 0), shell_mask)
    
    # Subtle highlight
    highlight_layer = Image.new("RGBA", (size, size), (255, 255, 255, 25))
    canvas.paste(highlight_layer, (-8, -8), mask=shell_mask)
    
    # 5. Inner Screen (Faceplate)
    screen_mask = Image.new("L", (size, size), 0)
    screen_draw = ImageDraw.Draw(screen_mask)
    screen_radius = 520
    screen_draw.ellipse(
        [
            (head_center[0] - screen_radius, head_center[1] - screen_radius),
            (head_center[0] + screen_radius, head_center[1] + screen_radius)
        ],
        fill=255
    )
    
    # Paste dark screen color
    screen_layer = Image.new("RGBA", (size, size), screen_color + (255,))
    canvas.paste(screen_layer, (0, 0), screen_mask)
    
    # Screen border neon cyan line
    draw = ImageDraw.Draw(canvas)
    draw.ellipse(
        [
            (head_center[0] - screen_radius, head_center[1] - screen_radius),
            (head_center[0] + screen_radius, head_center[1] + screen_radius)
        ],
        outline=neon_cyan,
        width=16
    )
    
    # 6. Neon Glow in 'P' loop
    neon_tube_r = 250
    glow_layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow_layer)
    glow_draw.ellipse(
        [
            (loop_center[0] - neon_tube_r, loop_center[1] - neon_tube_r),
            (loop_center[0] + neon_tube_r, loop_center[1] + neon_tube_r)
        ],
        outline=neon_cyan,
        width=70
    )
    glow_layer = glow_layer.filter(ImageFilter.GaussianBlur(60))
    canvas.alpha_composite(glow_layer)
    
    # White-hot tube core
    draw.ellipse(
        [
            (loop_center[0] - neon_tube_r, loop_center[1] - neon_tube_r),
            (loop_center[0] + neon_tube_r, loop_center[1] + neon_tube_r)
        ],
        outline=neon_white,
        width=20
    )
    
    # 7. Eyes (Simple circular glowing LEDs)
    eye_offset_x = 200
    eye_offset_y = 110
    eye_r = 70
    
    left_eye_center = (head_center[0] - eye_offset_x, head_center[1] - eye_offset_y)
    right_eye_center = (head_center[0] + eye_offset_x, head_center[1] - eye_offset_y)
    
    # Eye glows
    eye_glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    eg_draw = ImageDraw.Draw(eye_glow)
    for center in [left_eye_center, right_eye_center]:
        eg_draw.ellipse(
            [(center[0] - eye_r - 50, center[1] - eye_r - 50), (center[0] + eye_r + 50, center[1] + eye_r + 50)],
            fill=(neon_cyan[0], neon_cyan[1], neon_cyan[2], 50)
        )
    eye_glow = eye_glow.filter(ImageFilter.GaussianBlur(25))
    canvas.alpha_composite(eye_glow)
    
    # Eye cores
    for center in [left_eye_center, right_eye_center]:
        draw.ellipse(
            [(center[0] - eye_r, center[1] - eye_r), (center[0] + eye_r, center[1] + eye_r)],
            fill=neon_cyan
        )
        draw.ellipse(
            [(center[0] - eye_r + 22, center[1] - eye_r + 22), (center[0] + eye_r - 22, center[1] + eye_r - 22)],
            fill=(255, 255, 255, 255)
        )
        
    # 8. Mouth (Minimalist abstract smiling arc)
    mouth_box = [
        (head_center[0] - 120, head_center[1] + 50),
        (head_center[0] + 120, head_center[1] + 250)
    ]
    
    # Mouth glow
    mouth_glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    mg_draw = ImageDraw.Draw(mouth_glow)
    mg_draw.arc(mouth_box, start=20, end=160, fill=neon_cyan, width=40)
    mouth_glow = mouth_glow.filter(ImageFilter.GaussianBlur(20))
    canvas.alpha_composite(mouth_glow)
    
    # Mouth core
    draw.arc(mouth_box, start=20, end=160, fill=neon_white, width=16)
    
    # Downscale to 1024x1024 with LANCZOS
    final_img = canvas.resize((1024, 1024), Image.Resampling.LANCZOS)
    
    # Save the master PNG
    output_dir = "packages/gui/assets/app"
    os.makedirs(output_dir, exist_ok=True)
    master_png_path = os.path.join(output_dir, "AppIcon.png")
    final_img.save(master_png_path, "PNG")
    print(f"Master icon saved to {master_png_path}")
    
    # 9. Generate macOS .icns using iconutil
    iconset_dir = "target/tmp/AppIcon.iconset"
    os.makedirs(iconset_dir, exist_ok=True)
    
    sizes = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ]
    
    print("Generating resolutions for iconset...")
    for filename, resolution in sizes:
        resized_img = final_img.resize((resolution, resolution), Image.Resampling.LANCZOS)
        resized_img.save(os.path.join(iconset_dir, filename), "PNG")
        
    icns_path = os.path.join(output_dir, "AppIcon.icns")
    print("Compiling .icns file...")
    try:
        subprocess.run(["iconutil", "-c", "icns", "-o", icns_path, iconset_dir], check=True)
        print(f"macOS Icon bundle successfully compiled to {icns_path}")
    except Exception as e:
        print(f"Warning: Failed to compile .icns via iconutil: {e}")
        print("This is normal if you are not running on macOS or iconutil is not in path.")
        
    # Clean up iconset temp directory
    shutil.rmtree(iconset_dir)
    print("Cleaned up temporary iconset files.")

if __name__ == "__main__":
    draw_icon()
