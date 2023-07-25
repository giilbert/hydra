from utils import cmd, bail


def build():
    if cmd("docker build --file ./deploy/server.dockerfile -t hydra-server .") != 0:
        bail("docker build exited with a non-zero exit code.")
