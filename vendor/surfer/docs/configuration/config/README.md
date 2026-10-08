# Config file

This page documents the user configuration file loaded by Surfer on native builds.
You only need to specify the settings you want to change; any omitted setting keeps its default value.

The complete default configuration lives in [default_config.toml](https://gitlab.com/surfer-project/surfer/-/blob/main/default_config.toml?ref_type=heads).

## Example

```toml
theme = "dark+"
default_variable_name_type = "Global"
snap_distance = 10

[default_time_format]
format = "SI"
show_space = true
show_unit = true

[layout]
show_ticks = false
hierarchy_style = "Tree"
waveforms_text_size = 12.0

[behavior]
arrow_key_bindings = "Scroll"
primary_button_drag_behavior = "Measure"
```

## Load order

On native builds, configuration is loaded in this order, with later sources overriding earlier ones:

1. Built-in defaults from `default_config.toml`
2. The per-user `config.toml` in Surfer's configuration directory
3. Deprecated `surfer.toml` in the current working directory, if present
4. Any `.surfer/config.toml` files found from the filesystem root down to the current directory
5. Environment variables with the `SURFER` prefix

## Top-level settings

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `default_variable_name_type` | `"Unique"` | `Local`, `Unique`, `Global` | Default signal name display style. |
| `default_clock_highlight_type` | `"Line"` | `Line`, `Cycle`, `None` | Default clock highlighting mode. |
| `snap_distance` | `6` | non-negative number | Cursor snap distance in pixels. |
| `theme` | `""` | theme name | Theme to load. Leave empty to use the built-in default theme. |
| `undo_stack_size` | `50` | integer | Maximum number of undo steps to keep. |
| `autoreload_files` | `"Ask"` | `Always`, `Never`, `Ask` | What to do when loaded waveform files change on disk. |
| `autoload_sibling_state_files` | `"Ask"` | `Always`, `Never`, `Ask` | Whether matching state files should be loaded automatically. |
| `animation_time` | `0.1` | non-negative number | Duration of UI animations in seconds. |
| `animation_enabled` | `true` | boolean | Enable or disable UI animations entirely. |
| `show_divider_text` | `false` | boolean | Show divider labels inline in the waveform area. |
| `max_url_length` | `65534` | integer | Maximum URL length used for remote connections. Useful when a proxy enforces a limit. |

The remaining top-level keys are tables documented below: `default_time_format`, `layout`, `gesture`, `behavior`, `wcp`, `server`, and `shortcuts`.

## `[default_time_format]`

Controls how time values are rendered in the UI.

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `format` | `"No"` | `No`, `Locale`, `SI` | Numeric formatting style. `Locale` uses the current locale. `SI` groups digits using SI-style spacing. |
| `show_space` | `true` | boolean | Insert a space between the numeric part and the unit. |
| `show_unit` | `true` | boolean | Show the time unit suffix. |

## `[layout]`

Controls the initial UI layout and waveform rendering behavior.

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `show_hierarchy` | `true` | boolean | Show the hierarchy panel. |
| `show_menu` | `true` | boolean | Show the menu bar. |
| `show_toolbar` | `true` | boolean | Show the toolbar. |
| `show_ticks` | `true` | boolean | Show vertical tick lines in the waveform area. |
| `show_tooltip` | `true` | boolean | Show tooltips for variables. |
| `show_scope_tooltip` | `false` | boolean | Show tooltips for scopes. |
| `show_overview` | `true` | boolean | Show the overview panel. |
| `show_statusbar` | `true` | boolean | Show the status bar. |
| `show_variable_indices` | `true` | boolean | Show signal indices in the variable list when available. |
| `show_variable_direction` | `true` | boolean | Show direction icons or indicators for variables. |
| `show_default_timeline` | `true` | boolean | Add a timeline row by default. |
| `show_empty_scopes` | `false` | boolean | Show scopes that contain no visible items. |
| `show_hierarchy_icons` | `false` | boolean | Show scope and variable icons in the hierarchy. |
| `parameter_display_location` | `"Scopes"` | `Variables`, `Scopes`, `Tooltips`, `None` | Where parameter values are displayed in the hierarchy UI. |
| `window_width` | `1920` | integer | Initial window width in pixels. |
| `window_height` | `1080` | integer | Initial window height in pixels. |
| `align_names_right` | `false` | boolean | Right-align names in the item list. |
| `hierarchy_style` | `"Separate"` | `Separate`, `Tree`, `Variables` | Layout style used for the hierarchy and variable list. |
| `waveforms_text_size` | `11.0` | non-negative number | Text size for waveform values, in points. |
| `waveforms_line_height` | `16.0` | non-negative number | Base line height for waveforms, in points. |
| `waveforms_gap` | `2.5` | non-negative number | Vertical gap above and below waveform traces. Basically, how far the background is drawn. |
| `waveforms_line_height_multiples` | `[1, 2, 4, 8, 16]` | list of non-negative numbers | Available line-height multipliers for taller rows. |
| `transactions_line_height` | `30.0` | non-negative number | Line height for transaction streams. |
| `zoom_factors` | `[0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 2.5]` | list of non-negative numbers | Available UI zoom factors. |
| `default_zoom_factor` | `1.0` | non-negative number | Initial UI zoom factor. |
| `highlight_focused` | `false` | boolean | Highlight the waveform of the focused item. |
| `move_focus_on_inserted_marker` | `true` | boolean | Move focus to newly inserted markers. |
| `fill_high_values` | `true` | boolean | Fill the high state in boolean waveforms. |
| `use_dinotrace_style` | `false` | boolean | Use Dinotrace-style digital waveform drawing. This means no upper line and a bold lower line for all zeros vector values and a bold upper line for all ones vector values.|
| `transition_value` | `"Next"` | `Previous`, `Next`, `Both` | Which value to show when the cursor is exactly on a transition. |

## `[gesture]`

Controls the radial mouse-gesture overlay shown when using gesture mode.

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `size` | `300` | non-negative number | Size of the gesture help overlay. |
| `deadzone` | `20` | non-negative number | Minimum squared drag distance before a gesture action is triggered. |
| `background_radius` | `1.35` | non-negative number | Background circle radius as a factor of `size / 2`. |
| `background_gamma` | `0.75` | number between `0` and `1` | Background opacity factor. Lower values are more opaque. |

### `[gesture.mapping]`

Maps each drag direction to a gesture action.

Supported actions are `Cancel`, `ZoomIn`, `ZoomOut`, `ZoomToFit`, `GoToEnd`, and `GoToStart`.

| Direction | Default |
| --- | --- |
| `north` | `"Cancel"` |
| `northeast` | `"ZoomOut"` |
| `east` | `"ZoomIn"` |
| `southeast` | `"GoToEnd"` |
| `south` | `"Cancel"` |
| `southwest` | `"GoToStart"` |
| `west` | `"ZoomIn"` |
| `northwest` | `"ZoomToFit"` |

## `[behavior]`

Controls a small set of interaction defaults.

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `keep_during_reload` | `true` | boolean | Keep variables/items when they are unavailable after a reload. |
| `arrow_key_bindings` | `"Edge"` | `Edge`, `Scroll` | Make left/right arrow keys jump between edges or scroll the viewport. |
| `primary_button_drag_behavior` | `"Cursor"` | `Cursor`, `Measure` | Default behavior for primary-button dragging. Holding Shift temporarily selects the other mode. |

## `[wcp]`

Waveform Control Protocol server settings.

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `autostart` | `false` | boolean | Start the WCP server automatically on launch. |
| `address` | `"127.0.0.1:54321"` | `host:port` string | Bind address for the WCP server. |

## `[server]`

Settings for Surver's HTTP server.

| Key | Default | Values | Description |
| --- | --- | --- | --- |
| `bind_address` | `"127.0.0.1"` | host or IP string | Address to bind the server to. |
| `port` | `8911` | integer | TCP port to listen on. |

## `[shortcuts]`

The `shortcuts` table maps an action name to a list of key chords. Each value is an array of strings such as `"Command+O"` or `"PageDown"`, where each value in the list is one shortcut, not a sequence. Hence, each action can have multiple alternative shortcuts.

`Command` corresponds to ⌘ on Mac and `Ctrl` on all other platforms. For a list of key names, see [Key](https://docs.rs/egui/latest/egui/enum.Key.html).

The default configuration defines these actions:

| Action | Default binding |
| --- | --- |
| `open_file` | `Command+O` |
| `switch_file` | `Command+Shift+O` |
| `undo` | `Command+Z`, `U` |
| `redo` | `Command+Shift+Z`, `Command+Y` |
| `toggle_side_panel` | `B` |
| `toggle_toolbar` | `T` |
| `goto_end` | `E` |
| `goto_start` | `S` |
| `goto_top` | `Home` |
| `goto_bottom` | `End` |
| `save_state_file` | `Command+S` |
| `group_new` | `G` |
| `item_focus` | `F` |
| `select_all` | `Command+A` |
| `select_toggle` | `A` |
| `reload_waveform` | `R` |
| `zoom_in` | `Plus`, `Equals` |
| `zoom_out` | `Minus` |
| `ui_zoom_in` | `Command+Plus` |
| `ui_zoom_out` | `Command+Minus` |
| `scroll_up` | `PageUp` |
| `scroll_down` | `PageDown` |
| `delete_selected` | `Delete`, `X` |
| `marker_add` | `M` |
| `toggle_menu` | `Alt+M` |
| `show_command_prompt` | `Space` |
| `rename_item` | `F2` |
| `divider_add` | `D` |
| `zoom_to_fit` | `Shift+F` |
| `go_to_time` | `Command+G` |

## Notes

- Floating-point values that are documented as non-negative are clamped to `0` if a negative value is provided.
- Values documented as being between `0` and `1` are clamped to that range.
- Theme files are documented separately in the configuration section's themes page.
