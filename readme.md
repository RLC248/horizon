# Dimension transition proof for Horizon PR #19292

Game and focused test source `8d407e85147b50641c4b6aeee8ea5b295d059136`. Both binaries ran on Windows x64 with prepared Minecraft 26.3 resources. The Windows client used Vulkan and the integrated server in isolated test worlds.

The [proof mod](https://github.com/RLC248/horizon/blob/8b19c87087558c2cb9e09e68c65a08683dabd034/validation/19292-probe/src/lib.rs) registers `transition_proof:loading` for incoming Overworld and End loads. It draws its screen ID, the result of `event.cause(host)` and `event.reason` in the ordinary draw callback. These PNGs are unmodified game frames.

| Action | Standard single-player | Developer single-player |
| --- | --- | --- |
| Open a fresh single-player world | [Login / Other](login.png) | |
| Walk into an End portal block in the Overworld | [Travel / EndPortal](end-portal-travel.png) | [Travel / EndPortal](developer-end-portal-travel.png) |
| Die in the End and click Respawn to return to the Overworld | [DeathRespawn / Other](death-respawn.png) | [DeathRespawn / Other](developer-death-respawn.png) |

[Portal block before entering](end-portal-block.png), [arrival in the End](end-arrival.png) and [death screen in the End](end-death-screen.png).

A command placed the End portal block and prepared a surface to walk on. The player was positioned three blocks before the portal in the Overworld and walked into the portal block. The movement task returned `the world closed` as the client switched dimensions. The client then reported `minecraft:the_end`, showed The End advancement and remained in the same single-player session. The player switched to Survival and used `/kill @s` in the End. The ordinary Respawn button brought the player back to the Overworld with 20 health. A same-dimension death and respawn was also exercised in both modes. The cross-dimension case supplied the death-respawn loading frame.

[Command output, control events and artifact hashes](proof.json). The JSON retains the relevant fields from the control replies. [Local focused test output](focused-test-output.txt).
