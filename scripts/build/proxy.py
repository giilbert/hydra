from utils import cmd, bail
import cache


@cache.fs_cache(
    "hydra-proxy",
    [
        "deploy/proxy.dockerfile",
        "crates/hydra-proxy",
        "crates/shared",
        "deploy",
    ],
)
def build():
    if cmd("docker build --file ./deploy/proxy.dockerfile -t hydra-proxy .") != 0:
        bail("docker build exited with a non-zero exit code.")
