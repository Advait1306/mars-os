// MarsOS Branding Extension — replaces Activities button with a custom SVG icon
// Compatible with GNOME Shell 48 (ESM module format)

import St from 'gi://St';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class MarsBrandingExtension extends Extension {
    _button = null;

    enable() {
        // Hide the original Activities button
        const activities = Main.panel.statusArea.activities;
        if (activities)
            activities.container.hide();

        // Create a new panel button with our icon
        this._button = new PanelMenu.Button(0.0, 'MarsOS', true);

        const iconPath = GLib.build_filenamev([this.path, 'icons', 'mars-icon.svg']);
        // SVG is 98x128 — scale to fit panel height (~20px) keeping aspect ratio
        const height = 20;
        const width = Math.round(height * 98 / 128); // ≈ 15px
        const icon = new St.Icon({
            gicon: Gio.icon_new_for_string(iconPath),
            icon_size: height,
            style: `icon-size: ${height}px; width: ${width}px; height: ${height}px;`,
        });

        this._button.add_child(icon);

        // Click toggles the Activities overview
        this._button.connect('event', (_actor, event) => {
            if (event.type() === imports.gi.Clutter.EventType.BUTTON_RELEASE ||
                event.type() === imports.gi.Clutter.EventType.TOUCH_END) {
                Main.overview.toggle();
                return imports.gi.Clutter.EVENT_STOP;
            }
            return imports.gi.Clutter.EVENT_PROPAGATE;
        });

        // Insert at the very start of the left panel box
        Main.panel.addToStatusArea('mars-branding', this._button, 0, 'left');
    }

    disable() {
        if (this._button) {
            this._button.destroy();
            this._button = null;
        }

        // Restore the original Activities button
        const activities = Main.panel.statusArea.activities;
        if (activities)
            activities.container.show();
    }
}
