#!/usr/bin/env python3
"""Real nested Wayland/IPC test. Needs a running Wayland or X11 desktop."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug"


def main():
    with tempfile.TemporaryDirectory(prefix="wc-smoke-") as temp:
        control = str(Path(temp) / "control.sock")
        env = dict(os.environ, WC_SOCKET=control, XDG_CONFIG_HOME=temp)
        processes = []
        log = open(Path(temp) / "compositor.log", "w+")

        def rpc(method, params=None):
            message = {"method": method}
            if params is not None:
                message["params"] = params
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
                stream.settimeout(3)
                stream.connect(control)
                stream.sendall((json.dumps(message) + "\n").encode())
                reply = json.loads(stream.makefile("r").readline())
            return reply

        def wait_for(predicate, description):
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                if processes[0].poll() is not None:
                    raise AssertionError("Compositor stopped")
                try:
                    value = predicate()
                    if value:
                        return value
                except (OSError, KeyError, json.JSONDecodeError):
                    pass
                time.sleep(0.05)
            raise AssertionError(f"Timeout: {description}")

        def snapshot():
            reply = rpc("get")
            assert reply["error"] is None, reply
            return reply["snapshot"]

        try:
            processes.append(subprocess.Popen([BIN / "wc-compositor"], env=env, stdout=log, stderr=log))
            initial = wait_for(lambda: snapshot(), "control socket")
            child_env = dict(env, WAYLAND_DISPLAY=initial["wayland_display"])
            child_env.pop("DISPLAY", None)
            processes.append(subprocess.Popen([BIN / "wc-demo"], env=child_env, stdout=log, stderr=log))
            state = wait_for(lambda: (s if (s := snapshot())["windows"] == 1 and s["window_details"][0]["width"] > 0 else None), "first window")
            original = state["window_details"][0]
            assert rpc("place", "maximize")["error"] is None
            maximized = wait_for(lambda: (w if (w := snapshot()["window_details"][0])["width"] > original["width"] else None), "maximize configure/commit")
            assert maximized["x"] == 12 and maximized["y"] == 12
            settings = state["settings"]
            settings["gap"] = 26
            settings["background"] = "#132c29"
            settings["focus"] = "sloppy"
            reply = rpc("apply", settings)
            assert reply["error"] is None, reply
            wait_for(lambda: snapshot()["window_details"][0]["x"] == 26, "live reflow")
            path = Path(temp) / "wc/settings.toml"
            before = path.read_bytes()
            invalid = dict(settings, gap=999)
            assert rpc("apply", invalid)["error"]
            assert path.read_bytes() == before
            assert snapshot()["settings"]["gap"] == 26
            assert rpc("place", "left")["error"] is None
            left = wait_for(lambda: (w if (w := snapshot()["window_details"][0])["width"] < maximized["width"] else None), "left half")
            assert left["x"] == 26
            assert rpc("place", "right")["error"] is None
            right = wait_for(lambda: (w if (w := snapshot()["window_details"][0])["x"] > left["x"] else None), "right half")
            assert right["width"] > 0
            assert rpc("place", "fullscreen")["error"] is None
            wait_for(lambda: snapshot()["window_details"][0]["x"] == 0, "fullscreen")
            assert rpc("place", "restore")["error"] is None
            restored = wait_for(lambda: (w if (w := snapshot()["window_details"][0])["width"] == original["width"] else None), "restore")
            assert (restored["x"], restored["y"]) == (original["x"], original["y"])
            processes.append(subprocess.Popen([BIN / "wc-demo"], env=child_env, stdout=log, stderr=log))
            wait_for(lambda: snapshot()["windows"] == 2, "second window")
            old_focus = next(i for i, w in enumerate(snapshot()["window_details"]) if w["focused"])
            assert rpc("cycle")["error"] is None
            wait_for(lambda: snapshot()["window_details"][-1]["focused"], "cycle focus")
            assert old_focus == 1
            # Unknown/oversized input must leave the server available.
            assert rpc("not_a_method")["error"]
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
                stream.settimeout(3)
                stream.connect(control)
                stream.sendall(b"x" * 65537 + b"\n")
                assert json.loads(stream.makefile("r").readline())["error"]
            assert snapshot()["windows"] == 2
            assert rpc("close")["error"] is None
            wait_for(lambda: snapshot()["windows"] == 1, "close handshake")
            if os.environ.get("WC_SMOKE_GPU") == "1":
                processes.append(subprocess.Popen([BIN / "wc-settings"], env=child_env, stdout=log, stderr=log))
                wait_for(lambda: snapshot()["windows"] == 2 and snapshot()["window_details"][-1]["width"] > 0, "Blitz GPU client")
                time.sleep(0.5)
                assert processes[-1].poll() is None, "GPU client stopped"
                assert rpc("close")["error"] is None
                wait_for(lambda: snapshot()["windows"] == 1, "Blitz GPU client closes")
            processes[0].terminate()
            processes[0].wait(timeout=3)
            Path(control).unlink(missing_ok=True)  # SIGTERM does not run Rust destructors.
            processes[0] = subprocess.Popen([BIN / "wc-compositor"], env=env, stdout=log, stderr=log)
            reloaded = wait_for(lambda: snapshot(), "restart")
            assert reloaded["settings"]["gap"] == 26
            assert reloaded["settings"]["background"] == "#132c29"
            print("PASS: real Wayland clients, halves/fullscreen/restore, focus cycle, close, live settings, persistence across restart, validation, malformed/oversized IPC" + (", Blitz GPU client" if os.environ.get("WC_SMOKE_GPU") == "1" else ""))
        except Exception:
            log.flush()
            log.seek(0)
            print(log.read())
            raise
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
            for process in reversed(processes):
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            log.close()


if __name__ == "__main__":
    main()
