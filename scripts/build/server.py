from utils import cmd, bail
import cache


@cache.fs_cache("hydra-server", ["deploy/server.dockerfile", "crates/hydra-server"])
def build():
    if cmd("docker build --file ./deploy/server.dockerfile -t hydra-server .") != 0:
        bail("docker build exited with a non-zero exit code.")
