# Reading List

Subsystems and APIs to understand for developing on Debian + KDE.

## Learning Order

1. D-Bus — IPC backbone, everything talks over it
2. systemd — services, targets, loginctl
3. KConfig + Kirigami — enough to build a KDE app
4. XDG Desktop Portal — modern sandboxed system access
5. Solid / KIO — hardware and filesystem access
6. Wayland protocols — compositor-level features

## Core Linux/Debian

| Subsystem              | What it does                          | API/Interface                                               |
| ---------------------- | ------------------------------------- | ----------------------------------------------------------- |
| systemd                | Init, services, timers, targets       | `systemctl`, D-Bus (`org.freedesktop.systemd1`), unit files |
| apt/dpkg               | Package management                    | CLI, `libapt-pkg`, PackageKit D-Bus API                     |
| polkit                 | Privilege escalation for GUI apps     | D-Bus (`org.freedesktop.PolicyKit1`), `.policy` XML files   |
| logind                 | Session/seat/user mgmt, power actions | D-Bus (`org.freedesktop.login1`), `loginctl`                |
| udev                   | Device hotplug, hardware events       | udev rules, `libudev`, `udevadm`                            |
| NetworkManager         | Network config (Wi-Fi, VPN, etc.)     | D-Bus, `nmcli`, `libnm`                                     |
| PipeWire / WirePlumber | Audio + video routing                 | PipeWire API, `pw-cli`, `wpctl`                             |
| BlueZ                  | Bluetooth                             | D-Bus (`org.bluez`), `bluetoothctl`                         |
| UPower                 | Battery/power info                    | D-Bus (`org.freedesktop.UPower`)                            |
| AccountsService        | User account management               | D-Bus (`org.freedesktop.Accounts`)                          |
| CUPS                   | Printing                              | IPP, `lpadmin`                                              |
| colord                 | Color management / display profiles   | D-Bus (`org.freedesktop.ColorManager`)                      |

## Wayland / Display

| Subsystem          | What it does                                        | API/Interface                                  |
| ------------------ | --------------------------------------------------- | ---------------------------------------------- |
| Wayland protocol   | Client ↔ compositor communication                   | `libwayland-client`, protocol XML extensions   |
| KWin               | Window management, compositing, effects             | KWin scripting (JS/QML), D-Bus, KConfig        |
| XDG Desktop Portal | Sandboxed app access (files, screen, notifications) | D-Bus (`org.freedesktop.portal.*`)             |
| Layer Shell        | Panels, docks, overlays                             | `wlr-layer-shell` / `ext-layer-shell` protocol |
| xdg-shell          | Standard window lifecycle                           | Built into Wayland                             |

## KDE Frameworks

| Framework        | What it does                                             |
| ---------------- | -------------------------------------------------------- |
| KConfig          | Settings read/write (INI files in `~/.config/`)          |
| Kirigami         | Convergent QML UI framework                              |
| KIO              | Virtual filesystem, async file ops, network transparency |
| Solid            | Hardware discovery (wraps udev, UPower)                  |
| KNotifications   | Desktop notifications                                    |
| KAuth            | Polkit integration for privilege escalation              |
| KXMLGUI          | Menu bars, toolbars, keyboard shortcuts                  |
| KService         | `.desktop` file parsing, app discovery                   |
| KPackage         | Plugin/addon packaging system                            |
| Plasma Framework | Plasmoid/widget API, DataEngines, themes                 |
| NetworkManagerQt | Qt wrapper around NetworkManager D-Bus                   |
| BluezQt          | Qt wrapper around BlueZ D-Bus                            |
| ModemManagerQt   | Qt wrapper around ModemManager D-Bus                     |

## Freedesktop Standards

| Spec                  | What it does                                                        |
| --------------------- | ------------------------------------------------------------------- |
| XDG Base Directory    | Where configs, data, cache go (`~/.config`, `~/.local/share`, etc.) |
| Desktop Entry         | `.desktop` files — app launchers, MIME associations                 |
| Desktop Notifications | `org.freedesktop.Notifications` D-Bus interface                     |
| MPRIS                 | Media player control interface                                      |
| XDG Autostart         | Apps that launch at login                                           |
| Trash spec            | File deletion/restore                                               |
| MIME apps             | Default app associations                                            |
| Icon Theme            | Icon theme structure and lookup                                     |

## Official Docs

- KDE Developer Hub: https://develop.kde.org/docs/
- KDE API Reference: https://api.kde.org
- Wayland Protocols: https://wayland.app/protocols/
- Freedesktop Specs: https://specifications.freedesktop.org/
- D-Bus Tutorial: https://dbus.freedesktop.org/doc/dbus-tutorial.html
- systemd Man Pages: https://www.freedesktop.org/software/systemd/man/
