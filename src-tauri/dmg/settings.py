"""Finder layout in points. dmgbuild supplies the paths in defines."""
format = "UDZO"
files = [defines["app"]]
symlinks = {"Applications": "/Applications"}
icon = defines["icon"]
background = defines["background"]
# Default Finder on macOS 15.8.2 uses 28 points for its title bar.
window_rect = ((100, 100), (640, 428))
icon_locations = {"VisualSubForge.app": (176, 186), "Applications": (464, 186)}
default_view = "icon-view"
arrange_by = None
icon_size = 128
text_size = 13
label_pos = "bottom"
show_icon_preview = False
show_toolbar = show_sidebar = show_pathbar = show_status_bar = show_tab_view = False
