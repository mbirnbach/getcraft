# Privacy policy

GetCraft has no accounts, no analytics, no telemetry and no crash reporting. It never sends
information about you, your computer or your files to the GetCraft project or to anyone else.

## What GetCraft connects to

GetCraft has to look for new versions and download apps, so it connects to these services,
all run by [GitHub, Inc.](https://github.com):

| Address | Why | When |
|---|---|---|
| `raw.githubusercontent.com` | Download the release index (which apps exist and their latest versions) and app icons | At start and then at the check interval you choose in Settings (every 6 hours by default) |
| `api.github.com` | Ask GitHub directly for the latest releases, only if the release index is unavailable | Same as above |
| `github.com` and GitHub's download servers (`*.githubusercontent.com`) | Download the apps you install or update, their checksum files, and updates of GetCraft itself | When an app is installed or updated |

Like any web server, GitHub receives your IP address, the time and the address of each request,
and the user agent `GetCraft/<version>`. GitHub's handling of this data is described in the
[GitHub General Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).
GetCraft does not send cookies, identifiers or any other data with these requests.

Links such as *View on GitHub* or *What's new* open in your web browser; GetCraft does not
follow them itself.

To stop the automatic checks, quit GetCraft (from the menu bar or system tray) and turn off
*Start GetCraft when I log in* in Settings. Updates of a single app can be turned off from that
app's ••• menu (*Don't check*).

## What GetCraft stores on your computer

| What | macOS | Windows | Linux |
|---|---|---|---|
| Settings, the list of installed apps (`state.json`) and a log of the last two runs (`getcraft.log`, `getcraft.old.log`) | `~/Library/Application Support/GetCraft` | `%APPDATA%\GetCraft` | `~/.config/GetCraft` |
| Download cache and the release index cache | `~/Library/Caches/GetCraft` | `%LOCALAPPDATA%\GetCraft` | `~/.cache/GetCraft` |
| Login item, only if you turned on *Start GetCraft when I log in* | `~/Library/LaunchAgents/net.brnbch.getcraft.plist` | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (value `GetCraft`) | `~/.config/autostart/GetCraft.desktop` |

Nothing of this leaves your computer. How to remove all of it is described in the README under
[Uninstalling GetCraft](README.md#uninstalling-getcraft).

## Questions

Open an issue at <https://github.com/mbirnbach/getcraft/issues>.
