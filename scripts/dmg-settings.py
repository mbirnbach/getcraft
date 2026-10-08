# dmgbuild settings for GetCraft's DMG: the background in assets/dmg/ with the app in the left
# box and the Applications shortcut in the right one. Used by scripts/build-dmg.sh.
import os.path

app = defines["app"]
app_name = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}

# The volume icon shown on the desktop and in Finder's sidebar.
icon = defines["icon"]

# Window content: 768 × 512 points, matching the background (1536 × 1024 for Retina).
background = defines["background"]
# The window size includes the ~28 pt title bar.
window_rect = ((200, 120), (768, 540))
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
default_view = "icon-view"
include_icon_view_settings = True

icon_size = 112
text_size = 13
# Centres of the two boxes in the background, nudged up so icon and label sit inside the box.
icon_locations = {
    app_name: (187, 238),
    "Applications": (581, 238),
}
