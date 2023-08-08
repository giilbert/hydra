from utils import cmd, bail, print_header
from dataclasses import dataclass
from termcolor import colored
import cache
import json


@dataclass
class ImageSpec:
    name: str
    dockerfile: str
    command: str

    @classmethod
    def from_dict(cls, data: dict):
        return cls(data.get("name"), data.get("dockerfile"), data.get("command"))


def build():
    print(f"------------ {colored('Building images', 'magenta')} ------------")

    with open("deploy/base/_meta.json", "r") as f:
        metadatas: list[ImageSpec] = json.load(f)
        metadatas = [ImageSpec.from_dict(metadata) for metadata in metadatas]

    for metadata in metadatas:
        build_metadata(metadata)

    print(f"-----------------------------------------")


def build_single(name: str):
    with open("deploy/base/_meta.json", "r") as f:
        metadatas: list[ImageSpec] = json.load(f)
        metadatas = [ImageSpec.from_dict(metadata) for metadata in metadatas]

    for metadata in metadatas:
        if metadata.name == name:
            build_metadata(metadata)
            return


def build_metadata(metadata: ImageSpec):
    if cache.is_fs_cached(
        f"image-{metadata.name}",
        [f"deploy/base/{metadata.dockerfile}", "crates/hydrad", "crates/shared"],
    ):
        print(
            f"{colored('>', 'magenta')} cache {colored('hit', 'magenta')} (`image-{metadata.name}`)"
        )
        return

    print(
        f"{colored('x', 'red')} cache {colored('miss', 'magenta')} (`image-{metadata.name}`)"
    )

    print(f"> Building image `{metadata.name}`")
    if (
        cmd(
            f"docker build --file ./deploy/base/{metadata.dockerfile} -t hydra-{metadata.name} ./deploy/base"
        )
        != 0
    ):
        bail(f"docker build for `{metadata.name}` exited with a non-zero exit code.")

    print("> Saving image to tarball")
    if (
        cmd(f"docker save hydra-{metadata.name} > images/hydra-{metadata.name}.tar")
        != 0
    ):
        bail(f"docker save for `{metadata.name}` exited with a non-zero exit code.")

    cache.add_to_cache(
        f"image-{metadata.name}",
        [f"deploy/base/{metadata.dockerfile}", "crates/hydrad"],
    )
