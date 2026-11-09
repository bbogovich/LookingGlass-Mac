# Build and play Looking Glass on macOS

This fork has no Mac download. You build your own copy from source, and it runs only on your Mac, using your own American McGee's Alice game files. The game is an unfinished preview: see [Mac known issues](MACOS_KNOWN_ISSUES.md) and the [general limitations](KNOWN_ISSUES.md).

## What you need

- A Mac with Apple Silicon. Intel Macs are not supported.
- macOS 26 or newer.
- About 5 GB of free space for the build tools' output, the game files and saves.
- Your game files. See [where to get them](INSTALL.md#where-to-get-the-game-files).

## 1. Install the tools

In Terminal, install the Xcode Command Line Tools:

```sh
xcode-select --install
```

Then install Rust from [rustup.rs](https://rustup.rs), or with Homebrew:

```sh
brew install rustup
rustup default stable
```

The project needs Rust 1.86 or newer. You do not need an Apple Developer account.

## 2. Get the source

The Mac code is on the `macos-port` branch, not on `main`:

```sh
git clone --branch macos-port https://github.com/bbogovich/LookingGlass-Mac.git
cd LookingGlass-Mac
```

## 3. Build the app

```sh
python3 tools/build_macos.py --install
```

This builds `Looking Glass.app`, copies it to the `Applications` folder in your home folder, and signs it for your Mac. There is no Gatekeeper prompt, because you built it yourself.

Without `--install`, the app stays at `build/macos/Looking Glass.app`.

### Building the game files into the app (optional)

If you put your `Alice1` folder inside the source folder before you build, the build copies the game files into the app. The app then runs without any setup and uses your game portrait as its icon. You can also name a folder or archive:

```sh
python3 tools/build_macos.py --install --data ~/Downloads/Alice1_2011_vanilla.7z
```

Use `--no-data` to leave them out, or `--language de` to pick a language (`en`, `de`, `fr`, `es`, `nl`).

**An app built this way contains the game files. Keep it to yourself and do not share it.**

## 4. First launch

Open **Looking Glass** from your Applications folder. If the app has no game files, the **Looking Glass Setup** window opens. It looks in Downloads and beside the app, and it accepts:

- **Choose Folder**: the `Alice1` folder, or its `bin/base` folder.
- **Choose Archive**: the 2011 `.7z` archive, or a `.zip` of the game folder.
- Dragging either one onto the window.
- **Download**: an optional download from Internet Archive. It asks each time whether you are entitled to the files, and it defaults to not downloading. The file is about 933 MB, and its size and SHA-256 are checked before use.

Your original files are never changed. Pick a language, then choose **Play**.

## Playing

The [controls](CONTROLS.md) are the same as on Windows, with these Mac differences:

| Action | Mac |
| --- | --- |
| Full screen or window | **Command+Return**, or the green window button |
| Quit | **Command+Q** |
| Open Setup again | Hold **Option** while opening the game |
| Quick save, quick load | **F5**, **F9** (**Fn+F5**, **Fn+F9** on most laptops) |
| Restart level | **Home** (**Fn+Left Arrow** on a laptop) |

You can rebind keys in the game's options. Xbox controllers have been tested; other controllers supported by macOS should work but have not been tried.

Holding Option also closes the Finder window you opened the game from. That is normal Finder behaviour: press Option just after you double-click if you want to keep it open.

## Updating

```sh
git pull
python3 tools/build_macos.py --install
```

Rebuilding replaces the app and leaves your saves and settings alone.

## Where your files are

Everything is in `~/Library/Application Support/Looking Glass/private`:

| Item | Location |
| --- | --- |
| Saves | `saves/` |
| Settings | `preferences.json` |
| Game files imported by Setup | `game-data/` |
| Downloaded archive | `downloads/` |

To open the folder, choose **Go > Go to Folder** in Finder and paste the first path above. To remove the game, delete the app and that `Looking Glass` folder. Deleting the folder also deletes your saves.

Saves belong to the game files they were made with, so saves from the 2000 CD files show as unusable with the 2011 files, and the other way round. Switching back to the original files makes them usable again.

## Troubleshooting

| Message or problem | What to do |
| --- | --- |
| `the Xcode Command Line Tools are missing` | Run `xcode-select --install`, then build again |
| `Rust was not found` | Install Rust as in step 1, then open a new Terminal window |
| `macOS 26.0 or newer is required` | Update macOS |
| A Rust compile error about the language version | Run `rustup update stable` |
| Setup says files are missing | Choose the `Alice1` folder itself, or its `bin/base` folder |
| Setup warns about `pak5_mod.pk3` | Your files are from the 2000 CD. The game runs, but the 2011 set is the one the project supports |
| The game shows a generic icon | The app was built without game files. It works normally |
| Function keys change volume or brightness | Hold **Fn**, or rebind the action in the game's options |

The upstream project does not support the Mac version, so please do not report Mac problems there.
