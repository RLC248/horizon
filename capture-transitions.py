"""Capture genuine transition screens in an isolated Horizon single-player run."""
from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time


class Session:
    def __init__(self, binaries: Path, resources: Path, mod: Path, output: Path, developer: bool):
        self.output = output
        self.output.mkdir()
        self.game_dir = output / "game"
        self.game_dir.mkdir()
        mod_dir = self.game_dir / "horizon-mods"
        mod_dir.mkdir()
        shutil.copyfile(mod, mod_dir / mod.name)
        (self.game_dir / "horizon-mods.txt").write_text(
            "mod.transition_proof:true\n", encoding="utf-8"
        )
        (self.game_dir / "options.txt").write_text(
            "lang:en_us\nguiScale:3\nrenderDistance:6\nsimulationDistance:5\n",
            encoding="utf-8",
        )

        self.cli = binaries / "horizon-ctl.exe"
        self.endpoint = output / "control.json"
        self.command_log = (output / "commands.jsonl").open("w", encoding="utf-8")
        self.log_lock = threading.Lock()
        args = [
            str(binaries / "horizon-client.exe"),
            "--offscreen",
            "--no-pause",
            "--width", "960",
            "--height", "540",
            "--resources", str(resources),
            "--control-socket", str(self.endpoint),
            "--game-dir", str(self.game_dir),
            "--name", "TransitionProof",
            "--timeout-seconds", "560",
        ]
        if developer:
            args.append("--developer-single-player")
        env = dict(os.environ)
        for variable, folder in {
            "HORIZON_HOME": "home",
            "HORIZON_LAUNCHER_HOME": "launcher",
            "HORIZON_LAUNCHER_TRASH": "trash",
            "HORIZON_LAUNCHER_CLIPBOARD": "clipboard",
            "HORIZON_LAUNCHER_DESKTOP": "desktop",
            "XDG_DATA_HOME": "data",
            "XDG_CONFIG_HOME": "config",
            "TMP": "tmp",
            "TEMP": "tmp",
        }.items():
            path = output / folder
            path.mkdir(exist_ok=True)
            env[variable] = str(path)
        env.update({
            "HORIZON_NO_AUDIO": "1",
            "HORIZON_LAUNCHER_NO_SELF_UPDATE": "1",
            "HORIZON_NO_DISCORD": "1",
            "HORIZON_TEST_LOOPBACK": "1",
        })
        env.setdefault("WGPU_BACKEND", "vulkan")
        self.env = env
        self.record("launch", args)
        self.process_log = (output / "process.log").open("wb")
        self.process = subprocess.Popen(
            args,
            cwd=self.game_dir,
            stdout=self.process_log,
            stderr=subprocess.STDOUT,
            env=env,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )

    def record(self, command, reply):
        entry = {"cmd": command, "reply": reply}
        with self.log_lock:
            print(json.dumps(entry, ensure_ascii=False), file=self.command_log, flush=True)
        if not command.startswith("screenshot "):
            print(json.dumps(entry, ensure_ascii=False), flush=True)

    def connect(self, timeout=60):
        wait_until = time.monotonic() + timeout
        while not self.endpoint.exists() and time.monotonic() < wait_until:
            if self.process.poll() is not None:
                raise RuntimeError(f"Game exited with {self.process.returncode}; see process.log")
            time.sleep(0.1)
        address = json.loads(self.endpoint.read_text(encoding="utf-8"))
        stream = socket.create_connection(("127.0.0.1", address["port"]), timeout=timeout)
        stream.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        reader = stream.makefile("rb")
        stream.sendall(f'auth {address["token"]}\n'.encode())
        if not json.loads(reader.readline()).get("ok"):
            reader.close()
            stream.close()
            raise RuntimeError("The scratch game's control endpoint refused authentication")
        return stream, reader

    def run(self, command, timeout=60, allow_error=False):
        verb, _, arguments = command.partition(" ")
        if verb in {"command", "screenshot", "click"}:
            words = [verb, arguments]
        else:
            words = command.split()
        invocation = [str(self.cli), "--socket", str(self.endpoint), "--timeout", str(timeout)]
        with self.log_lock:
            print(json.dumps({"horizon-ctl": words}),
                  file=self.command_log, flush=True)
        result = subprocess.run(
            invocation + words,
            cwd=self.game_dir,
            env=self.env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=timeout + 10,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        reply = json.loads(result.stdout.strip().splitlines()[-1]) if result.stdout.strip() else {
            "error": result.stderr.strip() or f"horizon-ctl exited {result.returncode}"
        }
        self.record(command, reply)
        if (result.returncode or "error" in reply) and not allow_error:
            raise RuntimeError(f"{command}: {reply}")
        return reply

    def shot(self, name):
        path = self.output / f"{name}.png"
        reply = self.run(f"screenshot {path}", allow_error=True)
        return path if "error" not in reply and path.is_file() else None

    def capture(self, name, action, max_seconds):
        """Take genuine rendered frames before, during, and after the real control action."""
        frames = self.output / name
        frames.mkdir()
        started = threading.Event()
        canceled = threading.Event()

        def screenshots():
            try:
                stream, reader = self.connect(30)
                with stream, reader:
                    end = time.monotonic() + max_seconds
                    frame = 0
                    while not canceled.is_set() and time.monotonic() < end:
                        path = frames / f"{frame:04d}.png"
                        command = f"screenshot {path}"
                        stream.sendall(f"{command}\n".encode())
                        reply = json.loads(reader.readline())
                        self.record(command, reply)
                        frame += 1
                        started.set()
                        canceled.wait(0.015)
            finally:
                started.set()

        worker = threading.Thread(target=screenshots, daemon=True)
        worker.start()
        if not started.wait(35):
            raise RuntimeError(f"{name}: no capture start")
        try:
            action()
            time.sleep(0.5)
        finally:
            canceled.set()
            worker.join(timeout=35)

    def close(self):
        try:
            if self.process.poll() is None:
                self.run("quit", timeout=20, allow_error=True)
                self.process.wait(timeout=20)
        finally:
            if self.process.poll() is None:
                self.process.terminate()
                self.process.wait(timeout=8)
            self.process_log.close()
            self.command_log.close()


def setup_and_exercise(session, world):
    stream, reader = session.connect(timeout=90)
    reader.close()
    stream.close()
    session.run("hello")
    session.run("wait-for screen title --timeout 90", timeout=100, allow_error=True)

    def open_world():
        session.run(f"open {world} 248 creative")
        session.run("wait-ready 400", timeout=420)

    session.capture("login", open_world, max_seconds=90)
    state = session.run("state")
    assert state["screen"] == "playing" and state["dimension"] == "minecraft:overworld", state

    session.run("command gamerule spawn_mobs false")
    session.run("command gamemode survival")
    session.run("command kill @s")
    session.run("wait-for screen dead --timeout 20")
    dead = session.run("state")
    assert dead["screen"] == "dead", dead
    session.run("wait 25")
    death_widgets = session.run("ui")["widgets"]
    respawn = next(widget for widget in death_widgets if widget.get("label") == "Respawn")
    assert respawn["enabled"], respawn
    session.shot("death-screen")

    def die_and_respawn():
        session.run(f'click {respawn["id"]}')
        session.run("wait-for screen playing --timeout 45", timeout=50)

    session.capture("respawn", die_and_respawn, max_seconds=13)
    alive = session.run("state")
    assert alive["screen"] == "playing" and alive["player"]["health"] > 0, alive
    assert alive["dimension"] == "minecraft:overworld", alive
    session.shot("overworld-after-respawn")

    x, y, z = (math.floor(axis) for axis in alive["player"]["position"])
    session.run("command gamemode creative")
    session.run(f"command fill {x - 4} {y - 1} {z - 3} {x + 4} {y - 1} {z + 6} minecraft:stone")
    session.run(f"command fill {x - 4} {y} {z - 3} {x + 4} {y + 4} {z + 6} minecraft:air")
    session.run(f"command setblock {x} {y} {z + 3} minecraft:end_portal")
    portal = session.run(f"block {x} {y} {z + 3}")
    assert portal["block"] == "minecraft:end_portal", portal
    session.run(f"command tp @s {x + 0.5} {y} {z + 0.5}")
    session.run("look 0 25")
    session.shot("physical-end-portal")

    def walk_into_end_portal():
        movement = session.run("move forward 1.2", allow_error=True)
        if movement.get("error") not in (None, "the world closed"):
            raise RuntimeError(f"Portal approach failed: {movement}")
        end = time.monotonic() + 30
        while time.monotonic() < end:
            state = session.run("state")
            if state["dimension"] == "minecraft:the_end" and state["screen"] == "playing":
                session.run("wait-for loaded --timeout 90", timeout=100)
                return
            time.sleep(0.25)
        raise RuntimeError("Walking into the End portal did not reach the End")

    session.capture("end-portal", walk_into_end_portal, max_seconds=14)
    session.shot("end-arrival")
    session.run("command gamemode survival")
    session.run("command kill @s")
    session.run("wait-for screen dead --timeout 20")
    session.run("wait 25")
    dead_in_end = session.run("state")
    assert dead_in_end["dimension"] == "minecraft:the_end", dead_in_end
    widgets = session.run("ui")["widgets"]
    respawn_in_end = next(widget for widget in widgets if widget.get("label") == "Respawn")
    assert respawn_in_end["enabled"], respawn_in_end
    session.shot("end-death-screen")

    def respawn_in_overworld():
        session.run(f'click {respawn_in_end["id"]}')
        deadline = time.monotonic() + 35
        while time.monotonic() < deadline:
            state = session.run("state")
            player = state.get("player")
            if (
                state["screen"] == "playing"
                and state["dimension"] == "minecraft:overworld"
                and player and player["health"] > 0
            ):
                session.run("wait-for loaded --timeout 60", timeout=70)
                return
            time.sleep(0.15)
        raise RuntimeError("Respawn after dying in the End did not reach the Overworld")

    session.capture("end-death-respawn", respawn_in_overworld, max_seconds=15)
    session.shot("home-after-end-death")
    session.run("events --limit 50")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binaries", required=True, type=Path)
    parser.add_argument("--resources", required=True, type=Path)
    parser.add_argument("--mod", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--developer", action="store_true")
    args = parser.parse_args()
    session = Session(args.binaries.resolve(), args.resources.resolve(),
                      args.mod.resolve(), args.output.resolve(), args.developer)
    try:
        setup_and_exercise(session, "TransitionProof")
    finally:
        session.close()


if __name__ == "__main__":
    main()
