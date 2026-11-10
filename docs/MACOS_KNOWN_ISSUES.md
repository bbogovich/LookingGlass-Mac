# Mac known issues

Problems specific to the Mac build, or found while testing it. The [general limitations](KNOWN_ISSUES.md) apply as well. See [Build and play on macOS](INSTALL-macOS.md) for setup.

## Mac only

| Issue | Workaround |
| --- | --- |
| Co-op between a Mac and a Windows PC has not been tested, and neither has co-op between two Macs. Two windows on one Mac connect over the internet and play. | |
| If the game is force-quit or crashes while hosting co-op, the tunnel helper (`cloudflared`) keeps running. Upstream's Windows build does the same. | Quit it in Activity Monitor |
| Holding **Option** to open Setup, then choosing **Quit** in Setup, still starts the game when game files are already set up. | Quit the game afterwards |
| If the game cannot start, for example because a game file is damaged, its icon bounces and it closes without a message. | Hold Option while opening it to run Setup, or run `"/Applications/Looking Glass.app/Contents/MacOS/looking-glass"` in Terminal to see the error |
| Switching from full screen to a window in the game's settings may not apply the chosen window size. | Pick the size again, or use Command+Return |
| Laptop function keys control brightness and volume. | Hold **Fn** for F5, F9 and others, or rebind them |
| Tested with an Xbox controller only. | |
| Not yet tested: 120 Hz displays, external displays, sleep and wake, unplugging a controller mid-game. | |

## Found on Mac, also present upstream

These come from the shared game code, not the Mac port, and none has been reported upstream yet.

| Issue | Workaround |
| --- | --- |
| Walking can stop dead where a slope meets a steep face (for example in Herbaceous Path, `garden2`). | Run, jump, or turn slightly |
| Using a lever while the Pocket Watch has stopped time does nothing. Seen in an automated route; not yet confirmed in play. | Wait for time to restart |
| Some automated checks fail on this data set in the same way on upstream: chess, burrow and resident render checks, several co-op routes, and the Fortress portal check. They are listed in the fork's plan. | Not player-facing as far as known |
