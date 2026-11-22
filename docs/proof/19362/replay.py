"""Exercise the exact-head game through two real LAN client processes."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import time

HEAD = "ff9bb08f875ac1e8a5268412015b8b97a8881df2"
GUEST = "LANMenuGuest"
MACHINE = "lan_menu_proof:machine"
CHEST_MACHINE = "lan_menu_proof:chest_machine"


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


class Horizon:
    def __init__(self, name, output, args):
        self.name = name
        self.root = (output / name).resolve()
        self.root.mkdir()
        self.endpoint = self.root / "control.json"
        self.log = self.root / "process.log"
        self.ctl = args.bins / "horizon-ctl.exe"
        self.executable = args.bins / "horizon-client.exe"
        self.process_args = [
            "--resources", str(args.resources), "--game-dir", str(self.root),
            "--render-distance", "4", "--no-pause", "--test-terrain",
            "--width", "1280", "--height", "720", "--control-socket", str(self.endpoint),
            "--dev-mod", str(args.mod), "--name", name,
        ]
        self.transcript = output / "commands.ndjson"
        if not args.windowed:
            self.process_args.append("--offscreen")
        self.env = dict(os.environ)
        for name, folder in {
            "HORIZON_HOME": "home", "HORIZON_LAUNCHER_HOME": "launcher",
            "XDG_DATA_HOME": "data", "XDG_CONFIG_HOME": "config",
            "HORIZON_LAUNCHER_TRASH": "trash", "HORIZON_LAUNCHER_CLIPBOARD": "clipboard",
            "HORIZON_LAUNCHER_DESKTOP": "desktop", "TMP": "tmp", "TEMP": "tmp",
        }.items():
            directory = self.root / folder
            directory.mkdir(exist_ok=True)
            self.env[name] = str(directory)
        self.env.update(
            HORIZON_LAUNCHER_NO_SELF_UPDATE="1",
            HORIZON_NO_AUDIO="1", HORIZON_NO_DISCORD="1", HORIZON_TEST_LOOPBACK="1",
            RUST_LOG="info",
        )
        self.process = None

    def start(self, *args):
        command = [str(self.executable), *self.process_args, *args]
        with self.log.open("wb") as log:
            self.process = subprocess.Popen(
                command, cwd=self.root, env=self.env, stdin=subprocess.DEVNULL,
                stdout=log, stderr=subprocess.STDOUT,
            )
        self.until(self.endpoint.is_file, bool, seconds=180)

    def until(self, observe, accept, seconds=60):
        deadline = time.monotonic() + seconds
        while True:
            if self.process.poll() is not None:
                raise RuntimeError(f"{self.name} exited with {self.process.returncode}. See {self.log}.")
            last = observe()
            if accept(last):
                return last
            if time.monotonic() >= deadline:
                raise RuntimeError(f"{self.name}: expected state never arrived, last {last!r}.")
            time.sleep(0.3)

    def call(self, *words, timeout=60):
        command = [str(self.ctl), "--socket", str(self.endpoint), "--timeout", str(timeout),
                   *(str(word) for word in words)]
        result = subprocess.run(command, cwd=self.root, env=self.env, capture_output=True,
                                encoding="utf-8", errors="replace", timeout=timeout + 10)
        stdout = result.stdout.strip().splitlines()
        reply = json.loads(stdout[-1]) if stdout else {"error": result.stderr.strip()}
        with self.transcript.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps({"client": self.name, "command": words, "response": reply}) + "\n")
        if result.returncode or "error" in reply:
            raise RuntimeError(f"{self.name} {' '.join(map(str, words))}: {reply}")
        return reply

    def stop(self):
        if self.process is None or self.process.poll() is not None:
            return
        try:
            self.call("quit", timeout=10)
            self.process.wait(20)
        except Exception:
            if self.process.poll() is None:
                self.process.terminate()
                self.process.wait(15)

    def cmd(self, text):
        return self.call("command", text)

    def shot(self, path):
        self.call("screenshot", str(path))
        assert path.stat().st_size > 4096, f"Screenshot too small: {path}"


def storage_slots(screen):
    return [slot for slot in screen["slots"] if 300 <= slot["slot"] < 500]


def storage_geometry(ui, limit=2):
    ids = {"slot-0", *(f"slot-{300 + i}" for i in range(limit))}
    slots = {
        widget["id"]: widget["bounds"] for widget in ui["widgets"] if widget["id"] in ids
    }
    anchor_x, anchor_y, _, _ = slots.pop("slot-0")
    return {slot: [x - anchor_x, y - anchor_y, w, h] for slot, (x, y, w, h) in slots.items()}


def menu_title(screen):
    return "".join(part["text"] for part in screen.get("title", []))


def assert_same_block_clicks(block_click, item_click, pos, block):
    for click in (block_click, item_click):
        assert click["block"]["pos"] == list(pos), click
        assert click["block"]["block"] == block and not click["changed"], click


def position(game):
    return game.call("state")["player"]["position"]


def verify_provenance(args):
    identity = json.loads((args.bins / "source.json").read_text(encoding="utf-8-sig"))
    assert identity["source"] == HEAD, identity
    for filename, expected in identity["sha256"].items():
        assert sha256(args.bins / filename) == expected.lower(), filename
    mod_hashes = args.mod.with_name("SHA256SUMS")
    if mod_hashes.exists():
        for line in mod_hashes.read_text(encoding="utf-8").splitlines():
            expected, filename = line.split()
            origin = args.mod.with_name(filename)
            assert sha256(origin) == expected.lower(), filename
            if filename == "horizon-wasm-std.wasm":
                assert sha256(args.bins / filename) == expected.lower(), filename
    return identity


def sneak_use_on_block(host, guest, pos):
    guest.call("wait", 8)
    guest.call("key", "shift", "down")
    try:
        state = guest.until(
            lambda: host.call("lan", "status"),
            lambda status: any(player["name"] == GUEST and player["sneaking"]
                               for player in status["guests"]),
            seconds=12,
        )
        click = guest.call("interact", *pos)
        guest.call("wait-for", "screen", "container", "--timeout", 30)
        return click
    finally:
        guest.call("key", "shift", "up")


def replay_bare_open_screen(host, guest, output, pos):
    host.cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} {CHEST_MACHINE}")
    host.cmd(f"item replace block {pos[0]} {pos[1]} {pos[2]} container.0 with minecraft:iron_ingot 13")
    host.cmd(f"item replace entity {GUEST} hotbar.5 with minecraft:book")
    host.cmd(f"item replace entity {GUEST} hotbar.6 with minecraft:coal 9")
    guest.until(lambda: guest.call("inventory"), lambda inv: inv["hotbar"][6]["item"] == "minecraft:coal")

    guest.call("key", "esc", "tap")
    guest.call("wait-for", "screen", "playing", "--timeout", 30)
    guest.call("select", 1)
    block_click = guest.call("interact", *pos)
    guest.call("wait-for", "screen", "container", "--timeout", 30)
    block_screen = guest.until(
        lambda: guest.call("screen"),
        lambda screen: bool(storage_slots(screen)) and storage_slots(screen)[0].get("count") == 13,
    )
    assert [slot["slot"] for slot in storage_slots(block_screen)] == [300], block_screen
    assert storage_slots(block_screen)[0]["item"] == "minecraft:iron_ingot", block_screen
    block_geometry = storage_geometry(guest.call("ui"), limit=9)
    assert block_geometry == {"slot-300": [27, -100, 16, 16]}, block_geometry
    guest.shot(output / "guest-origin-chest-block.png")

    guest.call("key", "esc", "tap")
    guest.call("wait-for", "screen", "playing", "--timeout", 30)
    assert guest.call("select", 6)["held"]["item"] == "minecraft:book"
    item_click = sneak_use_on_block(host, guest, pos)
    assert_same_block_clicks(block_click, item_click, pos, CHEST_MACHINE)
    item_screen = guest.call("screen")
    assert menu_title(item_screen) == "Held book pouch", item_screen
    assert len(item_screen["slots"]) == 45, item_screen
    assert [slot["slot"] for slot in storage_slots(item_screen)] == list(range(300, 309)), item_screen
    item_geometry = storage_geometry(guest.call("ui"), limit=9)
    assert len(item_geometry) == 9, item_geometry
    first_y = item_geometry["slot-300"][1]
    assert all(item_geometry[f"slot-{300 + i}"] == [18 * i, first_y, 16, 16] for i in range(9)), item_geometry
    guest.call("click-slot", 6)
    guest.until(lambda: guest.call("screen"),
                lambda screen: (screen.get("carried") or {}).get("item") == "minecraft:coal")
    guest.call("click-slot", 300)
    item_filled = guest.until(
        lambda: guest.call("screen"),
        lambda screen: storage_slots(screen)[0].get("item") == "minecraft:coal",
    )
    assert storage_slots(item_filled)[0]["count"] == 9
    guest.shot(output / "guest-bare-stock-chest.png")
    return {
        "block": {"id": CHEST_MACHINE, "position": pos}, "menu_type": "minecraft:generic_9x1",
        "block_click": block_click, "held_item_sneak_click_same_block": item_click,
        "block_menu": {"title": menu_title(block_screen), "slots": storage_slots(block_screen),
                       "slot_bounds_relative_to_first_hotbar_slot": block_geometry},
        "held_item_menu": {"title": menu_title(item_filled), "total_slot_count": len(item_filled["slots"]),
                           "container_slots": storage_slots(item_filled),
                           "slot_bounds_relative_to_first_hotbar_slot": item_geometry},
    }


def replay(host, guest, output, identity, args):
    host.start("--world", "LAN Menu Proof", "--seed", "5", "--creative")
    host.call("wait-ready", 240, timeout=260)
    x, y, z = (math.floor(value) for value in position(host))
    b = (x, y, z + 2)
    host.cmd("defaultgamemode survival")
    host.cmd("gamerule spawn_mobs false")
    host.cmd("time set day")
    host.cmd(f"fill {x - 5} {y - 1} {z - 5} {x + 5} {y - 1} {z + 7} minecraft:stone")
    host.cmd(f"fill {x - 5} {y} {z - 5} {x + 5} {y + 4} {z + 7} minecraft:air")
    host.cmd(f"setblock {b[0]} {b[1]} {b[2]} {MACHINE}")
    host.cmd(f"item replace block {b[0]} {b[1]} {b[2]} container.0 with minecraft:gold_ingot 11")
    host.cmd(f"tp @s {x + 3.5} {y} {z + 2.5}")
    port = host.call("lan", "open", 0, "commands")["port"]
    guest.start("--server", f"127.0.0.1:{port}")
    guest.call("wait-for", "loaded", "--timeout", 240, timeout=260)
    host_status = guest.until(
        lambda: host.call("lan", "status"),
        lambda status: status["open"] and any(
            player["name"] == GUEST and player["placed"] and player["loaded"] for player in status["guests"]
        ),
    )

    guest_position = (x + 0.5, y, z + 0.5)
    host.cmd(f"gamemode survival {GUEST}")
    host.cmd(f"tp {GUEST} {' '.join(map(str, guest_position))}")
    guest.until(lambda: position(guest), lambda pos: math.dist(pos, guest_position) < 0.2)
    host.cmd(f"item replace entity {GUEST} hotbar.1 with minecraft:emerald 3")
    host.cmd(f"item replace entity {GUEST} hotbar.2 with minecraft:diamond 5")
    host.cmd(f"item replace entity {GUEST} hotbar.3 with minecraft:paper")
    guest.until(lambda: guest.call("inventory"), lambda inv: inv["hotbar"][3]["item"] == "minecraft:paper")
    guest.call("select", 1)

    host.call("look-at", x + 0.5, y + 1.0, z + 1.0)
    host.shot(output / "host-lan-world.png")
    block_click = guest.call("interact", *b)
    guest.call("wait-for", "screen", "container", "--timeout", 30)
    block_screen = guest.until(
        lambda: guest.call("screen"),
        lambda screen: bool(storage_slots(screen)) and storage_slots(screen)[0].get("count") == 11,
    )
    assert [slot["slot"] for slot in storage_slots(block_screen)] == [300], block_screen
    assert storage_slots(block_screen)[0]["item"] == "minecraft:gold_ingot", block_screen
    block_ui = guest.call("ui")
    block_geometry = storage_geometry(block_ui)
    assert block_geometry == {"slot-300": [27, -100, 16, 16]}, block_geometry
    guest.shot(output / "guest-block-menu.png")
    guest.call("key", "esc", "tap")
    guest.call("wait-for", "screen", "playing", "--timeout", 30)
    assert guest.call("select", 4)["held"]["item"] == "minecraft:paper"
    item_click = sneak_use_on_block(host, guest, b)
    assert_same_block_clicks(block_click, item_click, b, MACHINE)
    item_screen = guest.call("screen")
    assert menu_title(item_screen) == "Held paper pouch", item_screen
    assert storage_slots(item_screen) == [{"slot": 300, "item": None}, {"slot": 301, "item": None}], item_screen
    item_ui = guest.call("ui")
    item_geometry = storage_geometry(item_ui)
    assert item_geometry == {"slot-300": [11, -105, 16, 16], "slot-301": [112, -88, 16, 16]}, item_geometry

    markers = [(1, 300, "minecraft:emerald", 3), (2, 301, "minecraft:diamond", 5)]
    for player_slot, menu_slot, item, count in markers:
        guest.call("click-slot", player_slot)
        guest.until(lambda: guest.call("screen"), lambda s: (s.get("carried") or {}).get("item") == item)
        guest.call("click-slot", menu_slot)
        guest.until(
            lambda: guest.call("screen"),
            lambda s: any(slot["slot"] == menu_slot and slot.get("item") == item
                          and slot.get("count") == count for slot in storage_slots(s)),
        )
    filled_screen = guest.call("screen")
    guest.shot(output / "guest-held-item-menu.png")
    guest.call("key", "esc", "tap")
    guest.call("wait-for", "screen", "playing", "--timeout", 30)
    sneak_use_on_block(host, guest, b)
    reopened_screen = guest.until(
        lambda: guest.call("screen"),
        lambda screen: all(
            any(slot["slot"] == menu_slot and slot.get("item") == item and slot.get("count") == count
                for slot in storage_slots(screen)) for _, menu_slot, item, count in markers
        ),
    )
    assert menu_title(reopened_screen) == "Held paper pouch", reopened_screen
    guest.shot(output / "guest-held-item-reopened.png")
    assert host.call("state")["screen"] == "playing"
    summary = {
        "source": identity, "platform": "Windows", "transport": "integrated host / real LAN guest over TCP loopback",
        "processes": {"host": host.process.pid, "guest": guest.process.pid}, "host_lan": host_status,
        "block": {"id": MACHINE, "position": b}, "menu_type": "lan_menu_proof:storage",
        "block_click": block_click, "held_item_sneak_click_same_block": item_click,
        "block_menu": {"title": menu_title(block_screen), "slots": storage_slots(block_screen),
                       "slot_bounds_relative_to_first_hotbar_slot": block_geometry},
        "held_item_menu": {"title": menu_title(item_screen), "initial_slots": storage_slots(item_screen),
                           "slot_bounds_relative_to_first_hotbar_slot": item_geometry,
                           "filled_slots": storage_slots(filled_screen), "reopened_slots": storage_slots(reopened_screen)},
        "host_stayed_in_world": True, "mod_sha256": sha256(args.mod), "mod_source": args.mod_source,
        "screenshots": {p.name: sha256(p) for p in sorted(output.glob("*.png"))},
    }
    write_json(output / "block-ui.json", block_ui)
    write_json(output / "item-ui.json", item_ui)
    write_json(output / "result.json", summary)
    print("Custom mod menu pair verified.", flush=True)
    if args.bare:
        try:
            summary["bare_open_screen"] = replay_bare_open_screen(host, guest, output, (x + 1, y, z + 2))
            summary["screenshots"] = {p.name: sha256(p) for p in sorted(output.glob("*.png"))}
            write_json(output / "result.json", summary)
        except Exception as error:
            write_json(output / "bare-case-error.json", {"error": str(error)})
            raise
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--bins", required=True, type=lambda p: Path(p).resolve())
    parser.add_argument("--mod", required=True, type=lambda p: Path(p).resolve())
    parser.add_argument("--mod-source", required=True, help="Full source commit for the proof mod")
    parser.add_argument("--resources", required=True, type=lambda p: Path(p).resolve())
    parser.add_argument("--out", required=True, type=lambda p: Path(p).resolve())
    parser.add_argument("--windowed", action="store_true")
    parser.add_argument("--bare", action="store_true")
    args = parser.parse_args()
    identity = verify_provenance(args)
    args.out.mkdir()
    host = Horizon("LANMenuHost", args.out, args)
    guest = Horizon(GUEST, args.out, args)
    try:
        replay(host, guest, args.out, identity, args)
    finally:
        guest.stop()
        host.stop()
