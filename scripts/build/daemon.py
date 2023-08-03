from utils import cmd, bail
import cache


@cache.fs_cache("hydrad", ["deploy/hydrad.dockerfile", "crates/hydrad"])
def build():
    if cmd("docker build --file ./deploy/hydrad.dockerfile -t hydrad .") != 0:
        bail("docker build exited with a non-zero exit code.")
